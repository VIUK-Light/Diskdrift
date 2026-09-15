//! Deep macOS storage: volumes, local snapshots, VM/swap and system caches.
//!
//! Everything here is an estimate. Snapshot blocks are shared with the
//! volume and purgeable space is managed by macOS, so these numbers are
//! labelled rather than presented as reclaimable.

use crate::core::error::Result;
use crate::core::local_snapshot::{self, LocalSnapshot};
use crate::core::scan::{self, ScanConfig};
use crate::core::volumes::{self, VolumeInfo};
use crate::scanners;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PathSize {
    pub path: PathBuf,
    pub allocated: u64,
    pub logical: u64,
    pub files: u64,
    pub dirs: u64,
}

#[derive(Debug, Clone)]
pub struct SystemReport {
    pub volumes: Vec<VolumeInfo>,
    pub local_snapshots: Vec<LocalSnapshot>,
    pub vm: Option<PathSize>,
    pub caches: Option<PathSize>,
    pub notes: Vec<String>,
}

pub fn report(home: &Path, threads: usize) -> SystemReport {
    let volumes = volumes::volumes().unwrap_or_default();
    let local_snapshots = local_snapshot::list();
    let vm = measure(Path::new("/private/var/vm"), home, threads);
    let caches = measure(Path::new("/Library/Caches"), home, threads);

    let mut notes = Vec::new();
    notes.push(
        "Local snapshots and purgeable space are macOS-managed; their blocks are shared with the volume."
            .to_string(),
    );
    if local_snapshots.is_empty() {
        notes.push("No local snapshots found (or tmutil is unavailable).".to_string());
    }
    if vm.is_none() {
        notes.push("VM/swap directory is not readable without elevated permissions.".to_string());
    }

    SystemReport {
        volumes,
        local_snapshots,
        vm,
        caches,
        notes,
    }
}

fn measure(path: &Path, home: &Path, threads: usize) -> Option<PathSize> {
    if !path.is_dir() {
        return None;
    }
    let out = scan::run(ScanConfig {
        home,
        targets: vec![scanners::generic::target_for(path.to_path_buf())],
        threads,
        progress: None,
        exclusions: Vec::new(),
        depth_override: Some(0),
    });
    Some(PathSize {
        path: path.to_path_buf(),
        allocated: out.walk.totals.allocated,
        logical: out.walk.totals.logical,
        files: out.walk.totals.files,
        dirs: out.walk.totals.dirs,
    })
}

#[allow(dead_code)]
pub fn volumes_only() -> Result<Vec<VolumeInfo>> {
    volumes::volumes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_root_volume() {
        let home = std::env::temp_dir();
        let report = report(&home, 1);
        assert!(report
            .volumes
            .iter()
            .any(|v| v.mount_point == std::path::Path::new("/")));
        assert!(!report.notes.is_empty());
    }
}
