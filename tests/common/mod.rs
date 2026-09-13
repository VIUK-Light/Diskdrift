#![allow(dead_code)]

//! Shared helpers for integration tests. Everything is created under the
//! system temporary directory and removed on drop, so tests never depend on
//! (or touch) the real machine's data.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("diskdrift-test-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    pub fn join(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort. If a test chmod'ed a directory to 000, restore first.
        let _ = restore_permissions(&self.path);
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn restore_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if path.is_dir() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                restore_permissions(&p)?;
            }
        }
    }
    Ok(())
}

/// Create a file of exactly `len` bytes (zeros).
pub fn write_file(path: &Path, len: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(path, vec![0u8; len]).expect("write file");
}

/// Allocated bytes as reported by the same syscall the scanner uses.
pub fn allocated_bytes(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let md = std::fs::symlink_metadata(path).expect("stat file");
    md.blocks() * 512
}

pub fn logical_bytes(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let md = std::fs::symlink_metadata(path).expect("stat file");
    md.size()
}
