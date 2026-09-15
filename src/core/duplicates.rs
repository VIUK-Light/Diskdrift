//! Duplicate detection: size -> partial hash -> full hash.
//!
//! Files are only hashed after cheaper filters, and full hashing happens
//! only for candidates that share a size and a partial hash. CommonCrypto
//! (part of the system) provides SHA-256; no extra dependency.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const PARTIAL_BYTES: u64 = 64 * 1024;
const CHUNK: usize = 1024 * 1024;

#[repr(C, align(8))]
struct Sha256Ctx {
    opaque: [u8; 128],
}

unsafe extern "C" {
    fn CC_SHA256_Init(ctx: *mut Sha256Ctx) -> i32;
    fn CC_SHA256_Update(ctx: *mut Sha256Ctx, data: *const u8, len: u32) -> i32;
    fn CC_SHA256_Final(md: *mut u8, ctx: *mut Sha256Ctx) -> i32;
}

fn sha256_reader(mut reader: impl Read, max_bytes: Option<u64>) -> Option<String> {
    let mut ctx = Sha256Ctx { opaque: [0; 128] };
    if unsafe { CC_SHA256_Init(&mut ctx) } == 0 {
        return None;
    }
    let mut buffer = vec![0u8; CHUNK];
    let mut remaining = max_bytes.unwrap_or(u64::MAX);
    while remaining > 0 {
        let want = buffer.len().min(remaining.min(usize::MAX as u64) as usize);
        let read = reader.read(&mut buffer[..want]).ok()?;
        if read == 0 {
            break;
        }
        unsafe {
            CC_SHA256_Update(&mut ctx, buffer.as_ptr(), read as u32);
        }
        remaining -= read as u64;
    }
    let mut digest = [0u8; 32];
    unsafe {
        CC_SHA256_Final(digest.as_mut_ptr(), &mut ctx);
    }
    Some(hex(&digest))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn sha256_partial(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    sha256_reader(file, Some(PARTIAL_BYTES))
}

pub fn sha256_full(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    sha256_reader(file, None)
}

#[derive(Debug, Clone)]
pub struct DuplicateFile {
    pub path: PathBuf,
    pub allocated: u64,
    pub logical: u64,
    pub hard_link: bool,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub hash: String,
    pub logical: u64,
    pub allocated: u64,
    pub files: Vec<DuplicateFile>,
    pub reclaimable: u64,
    pub model: Option<ModelInfo>,
}

#[derive(Debug, Clone)]
pub struct DuplicateReport {
    pub files_considered: u64,
    pub groups: Vec<DuplicateGroup>,
    pub reclaimable_total: u64,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    logical: u64,
    allocated: u64,
    dev: u64,
    ino: u64,
    nlink: u64,
}

fn collect(
    dir: &Path,
    min_size: u64,
    models_only: bool,
    exclusions: &[PathBuf],
    out: &mut Vec<Candidate>,
    depth: usize,
) {
    if depth > 128 || out.len() >= 200_000 || exclusions.iter().any(|e| dir.starts_with(e)) {
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
            collect(&path, min_size, models_only, exclusions, out, depth + 1);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if models_only && model_info(&path).is_none() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let logical = metadata.size();
        if logical < min_size {
            continue;
        }
        out.push(Candidate {
            path,
            logical,
            allocated: metadata.blocks().saturating_mul(512),
            dev: metadata.dev(),
            ino: metadata.ino(),
            nlink: metadata.nlink(),
        });
    }
}

pub fn find_duplicates(
    roots: &[PathBuf],
    min_size: u64,
    limit: usize,
    models_only: bool,
    exclusions: &[PathBuf],
) -> DuplicateReport {
    let mut candidates = Vec::new();
    for root in roots {
        collect(root, min_size, models_only, exclusions, &mut candidates, 0);
    }
    let files_considered = candidates.len() as u64;

    // Stage 1: same logical size.
    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    for candidate in candidates {
        by_size
            .entry(candidate.logical)
            .or_default()
            .push(candidate);
    }

    // Stage 2: same partial hash (only for same-size candidates).
    let mut by_partial: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (size, group) in by_size {
        if group.len() < 2 {
            continue;
        }
        for candidate in group {
            if let Some(hash) = sha256_partial(&candidate.path) {
                by_partial.entry((size, hash)).or_default().push(candidate);
            }
        }
    }

    // Stage 3: same full hash.
    let mut by_full: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for ((size, _), group) in by_partial {
        if group.len() < 2 {
            continue;
        }
        for candidate in group {
            if let Some(hash) = sha256_full(&candidate.path) {
                by_full.entry((size, hash)).or_default().push(candidate);
            }
        }
    }

    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for ((size, hash), group) in by_full {
        if group.len() < 2 {
            continue;
        }
        let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
        let mut files = Vec::new();
        let mut smallest_allocated = u64::MAX;
        for candidate in &group {
            // Every candidate registers its inode; a repeated inode means a
            // hard link to a file already counted in this group.
            let key = (candidate.dev, candidate.ino);
            let hard_link = !seen_inodes.insert(key);
            if !hard_link {
                smallest_allocated = smallest_allocated.min(candidate.allocated);
            }
            let _ = candidate.nlink;
            files.push(DuplicateFile {
                path: candidate.path.clone(),
                allocated: candidate.allocated,
                logical: candidate.logical,
                hard_link,
            });
        }
        let distinct = seen_inodes.len() as u64;
        let reclaimable = if distinct >= 2 {
            (distinct - 1) * smallest_allocated.min(size)
        } else {
            0
        };
        if reclaimable == 0 {
            continue; // only hard links: the same file, nothing to reclaim
        }
        let model = files.first().and_then(|file| model_info(&file.path));
        groups.push(DuplicateGroup {
            hash,
            logical: size,
            allocated: smallest_allocated,
            files,
            reclaimable,
            model,
        });
    }

    groups.sort_by_key(|group| std::cmp::Reverse(group.reclaimable));
    groups.truncate(limit.max(1));
    let reclaimable_total = groups.iter().map(|group| group.reclaimable).sum();

    DuplicateReport {
        files_considered,
        groups,
        reclaimable_total,
    }
}

/// Recognise GGUF / safetensors models and split the quantization suffix.
pub fn model_info(path: &Path) -> Option<ModelInfo> {
    let extension = path.extension()?.to_string_lossy().to_lowercase();
    if extension != "gguf" && extension != "safetensors" {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let mut tokens: Vec<String> = stem.split('-').map(|t| t.to_string()).collect();
    let mut quantization = Vec::new();
    while let Some(last) = tokens.last() {
        if is_quantization(last) {
            quantization.insert(0, tokens.pop().unwrap());
        } else {
            break;
        }
    }
    let name = if tokens.is_empty() {
        stem.clone()
    } else {
        tokens.join("-")
    };
    Some(ModelInfo {
        name,
        quantization: (!quantization.is_empty()).then(|| quantization.join("-")),
    })
}

fn is_quantization(token: &str) -> bool {
    let lower = token.trim_start_matches(['.', '_']).to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some('q') | Some('f') | Some('i') => {}
        _ => return false,
    }
    lower.chars().any(|c| c.is_ascii_digit())
        && lower
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
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
                .join(format!("diskdrift-dup-{tag}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Temp(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn payload(seed: u8, len: usize) -> Vec<u8> {
        (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
    }

    #[test]
    fn finds_identical_files() {
        let tmp = Temp::new("identical");
        let data = payload(7, 200_000);
        write(&tmp.0.join("a.bin"), &data);
        write(&tmp.0.join("nested/b.bin"), &data);
        write(&tmp.0.join("other/c.bin"), &payload(9, 200_000));

        let report = find_duplicates(&[tmp.0.clone()], 1_000, 10, false, &[]);
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].files.len(), 2);
        assert!(report.groups[0].reclaimable >= 200_000);
        assert!(report.reclaimable_total >= 200_000);
    }

    #[test]
    fn partial_hash_prevents_full_hash_false_positives() {
        let tmp = Temp::new("partial");
        // Same size and same 64 KiB prefix, different tail: must not group.
        let exact = payload(1, 100_000);
        let mut near_miss = payload(1, 100_000);
        near_miss[90_000] = 255;
        write(&tmp.0.join("a.bin"), &exact);
        write(&tmp.0.join("b.bin"), &near_miss);

        let report = find_duplicates(&[tmp.0.clone()], 1_000, 10, false, &[]);
        assert!(report.groups.is_empty());

        // A true copy of a.bin (not of the near miss) must be found.
        write(&tmp.0.join("c.bin"), &exact);
        let report = find_duplicates(&[tmp.0.clone()], 1_000, 10, false, &[]);
        assert_eq!(report.groups.len(), 1, "only the truly identical pair");
        assert_eq!(report.groups[0].files.len(), 2);
        let names: Vec<String> = report.groups[0]
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"a.bin".to_string()));
        assert!(names.contains(&"c.bin".to_string()));
        assert!(!names.contains(&"b.bin".to_string()));
    }

    #[test]
    fn hard_links_are_not_duplicates() {
        let tmp = Temp::new("hardlink");
        let data = payload(3, 100_000);
        write(&tmp.0.join("a.bin"), &data);
        std::fs::hard_link(tmp.0.join("a.bin"), tmp.0.join("b.bin")).unwrap();

        let report = find_duplicates(&[tmp.0.clone()], 1_000, 10, false, &[]);
        assert!(report.groups.is_empty(), "hard links share the same blocks");
    }

    #[test]
    fn models_only_and_quantization_parsing() {
        let tmp = Temp::new("models");
        let data = payload(5, 150_000);
        write(&tmp.0.join("Qwen3-8B-Q4_K_M.gguf"), &data);
        write(&tmp.0.join("backup/Qwen3-8B-Q4_K_M.gguf"), &data);
        write(&tmp.0.join("notes.bin"), &data);

        let report = find_duplicates(&[tmp.0.clone()], 1_000, 10, true, &[]);
        assert_eq!(report.groups.len(), 1);
        let model = report.groups[0].model.as_ref().expect("model info");
        assert_eq!(model.name, "Qwen3-8B");
        assert_eq!(model.quantization.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn sha256_matches_known_vector() {
        let tmp = Temp::new("sha");
        write(&tmp.0.join("abc.bin"), b"abc");
        let hash = sha256_full(&tmp.0.join("abc.bin")).unwrap();
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
