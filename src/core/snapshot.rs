//! Snapshot data model. No file contents are ever stored — only aggregates.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatVal {
    pub logical: u64,
    pub allocated: u64,
    pub files: u64,
    pub dirs: u64,
}

impl CatVal {
    pub fn add(&mut self, other: &CatVal) {
        self.logical = self.logical.saturating_add(other.logical);
        self.allocated = self.allocated.saturating_add(other.allocated);
        self.files = self.files.saturating_add(other.files);
        self.dirs = self.dirs.saturating_add(other.dirs);
    }

    pub fn is_zero(&self) -> bool {
        self.logical == 0 && self.allocated == 0 && self.files == 0 && self.dirs == 0
    }

    pub fn net_allocated(&self, old: &CatVal) -> i64 {
        self.allocated as i64 - old.allocated as i64
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    pub id: i64,
    pub created_at: String,
    pub created_at_local: String,
    pub total_logical_bytes: u64,
    pub total_allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
    pub symlink_count: u64,
    pub skipped_count: u64,
    pub duration_ms: u64,
    pub app_version: String,
}

#[derive(Debug, Clone)]
pub struct CategoryRow {
    pub category_id: String,
    pub val: CatVal,
}

#[derive(Debug, Clone)]
pub struct DirectoryRow {
    pub path: std::path::PathBuf,
    pub category_id: String,
    pub val: CatVal,
}

impl SnapshotMeta {
    /// Unix time from `created_at` (stored as UTC RFC 3339).
    pub fn unix_time(&self) -> i64 {
        crate::core::time::parse_datetime_prefix(&self.created_at).unwrap_or(0)
    }
}

/// A change event recorded by `diskdrift watch`.
#[derive(Debug, Clone)]
pub struct EventDraft {
    pub timestamp_unix: i64,
    /// "grow" | "shrink" | "removed"
    pub kind: &'static str,
    pub path: std::path::PathBuf,
    pub category_id: String,
    pub delta_bytes: i64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

#[derive(Debug, Clone)]
pub struct EventRow {
    pub id: i64,
    pub timestamp_unix: i64,
    pub timestamp_local: String,
    pub kind: String,
    pub path: std::path::PathBuf,
    pub category_id: String,
    pub delta_bytes: i64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}
