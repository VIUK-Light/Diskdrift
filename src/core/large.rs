//! Largest files on disk (bounded memory: only a top-N heap is kept).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LargeFile {
    pub path: PathBuf,
    pub allocated: u64,
    pub logical: u64,
}

/// Top `limit` files by allocated size under `roots`, skipping symlinks and
/// de-duplicating hard links.
pub fn largest_files(
    roots: &[PathBuf],
    limit: usize,
    min_size: u64,
    exclusions: &[PathBuf],
) -> Vec<LargeFile> {
    let limit = limit.max(1);
    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    for root in roots {
        walk(root, &mut heap, &mut seen, limit, min_size, exclusions, 0);
    }
    let mut out: Vec<LargeFile> = heap
        .into_iter()
        .map(|Reverse((allocated, path))| {
            let logical = std::fs::symlink_metadata(&path)
                .map(|m| m.len())
                .unwrap_or(allocated);
            LargeFile {
                path,
                allocated,
                logical,
            }
        })
        .collect();
    out.sort_by_key(|file| Reverse(file.allocated));
    out
}

#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    heap: &mut BinaryHeap<Reverse<(u64, PathBuf)>>,
    seen: &mut HashSet<(u64, u64)>,
    limit: usize,
    min_size: u64,
    exclusions: &[PathBuf],
    depth: usize,
) {
    if depth > 128 || exclusions.iter().any(|e| dir.starts_with(e)) {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            walk(&path, heap, seen, limit, min_size, exclusions, depth + 1);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.nlink() > 1 {
            let key = (metadata.dev(), metadata.ino());
            if !seen.insert(key) {
                continue;
            }
        }
        let allocated = metadata.blocks().saturating_mul(512);
        if allocated < min_size {
            continue;
        }
        heap.push(Reverse((allocated, path)));
        while heap.len() > limit {
            heap.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Temp(PathBuf);
    impl Temp {
        fn new(tag: &str) -> Temp {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir()
                .join(format!("diskdrift-large-{tag}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Temp(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, len: usize) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, vec![0u8; len]).unwrap();
    }

    #[test]
    fn returns_largest_first_with_limit() {
        let tmp = Temp::new("order");
        write(&tmp.0.join("small.bin"), 10_000);
        write(&tmp.0.join("big.bin"), 200_000);
        write(&tmp.0.join("nested/mid.bin"), 80_000);

        let files = largest_files(&[tmp.0.clone()], 2, 0, &[]);
        assert_eq!(files.len(), 2);
        assert!(files[0].path.ends_with("big.bin"));
        assert!(files[1].path.ends_with("mid.bin"));
        assert!(files[0].allocated >= files[1].allocated);
    }

    #[test]
    fn filters_by_min_size_and_hardlinks() {
        let tmp = Temp::new("filters");
        let file = tmp.0.join("a.bin");
        write(&file, 50_000);
        std::fs::hard_link(&file, tmp.0.join("hard.bin")).unwrap();
        write(&tmp.0.join("tiny.bin"), 10);

        // min_size compares allocated (on-disk) bytes, so 8 KiB excludes the
        // 4 KiB tiny file while keeping the 50 KB one.
        let files = largest_files(&[tmp.0.clone()], 10, 8_000, &[]);
        assert_eq!(files.len(), 1, "hard links and tiny files are skipped");
        let name = files[0]
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name == "a.bin" || name == "hard.bin",
            "hard link pair must be counted once, got {name}"
        );
        assert!(files[0].allocated >= 50_000);
    }
}
