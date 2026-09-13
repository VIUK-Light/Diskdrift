//! Stable JSON output. Schema version is `version: 1`.
//!
//! Field order is declaration order (serde serializes structs in order) and
//! the schema is intended to be consumed by future GUIs and scripts.

use crate::core::categories;
use crate::core::diff::DiffResult;
use crate::core::doctor::DoctorReport;
use crate::core::explain::{ExplainOutput, display_path};
use crate::core::fs::{self, Accum};
use crate::core::scan::ScanOutput;
use crate::core::snapshot::SnapshotMeta;
use crate::core::time;
use serde::Serialize;
use std::path::Path;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct TotalsJson {
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

impl From<&Accum> for TotalsJson {
    fn from(a: &Accum) -> Self {
        TotalsJson {
            logical_bytes: a.logical,
            allocated_bytes: a.allocated,
            file_count: a.files,
            directory_count: a.dirs,
        }
    }
}

#[derive(Serialize)]
pub struct CategoryJson {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

#[derive(Serialize)]
pub struct DirectoryJson {
    pub path: String,
    pub category_id: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

#[derive(Serialize)]
pub struct RootJson {
    pub path: String,
    pub scanner: String,
    pub category_hint: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

#[derive(Serialize)]
pub struct SkippedJson {
    pub path: String,
    pub reason: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct ScanJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub duration_ms: u64,
    pub threads: usize,
    pub totals: TotalsJson,
    pub categories: Vec<CategoryJson>,
    pub directories: Vec<DirectoryJson>,
    pub roots: Vec<RootJson>,
    pub missing_roots: Vec<String>,
    pub skipped: Vec<SkippedJson>,
    pub skipped_count: u64,
    pub symlink_count: u64,
    pub hardlink_deduplicated_count: u64,
    pub interrupted: bool,
}

pub fn scan(out: &ScanOutput) -> ScanJson {
    let rolled = fs::rollup_categories(&out.walk.categories);
    let mut categories_out = Vec::new();
    for (idx, def) in categories::CATEGORIES.iter().enumerate() {
        let a = rolled[idx];
        if a.is_zero() {
            continue;
        }
        categories_out.push(CategoryJson {
            id: def.id.to_string(),
            name: def.name.to_string(),
            parent_id: def.parent.map(|p| p.to_string()),
            logical_bytes: a.logical,
            allocated_bytes: a.allocated,
            file_count: a.files,
            directory_count: a.dirs,
        });
    }
    categories_out.sort_by_key(|c| std::cmp::Reverse(c.allocated_bytes));

    let mut directories_out: Vec<DirectoryJson> = out
        .walk
        .directories
        .iter()
        .filter(|(_, d)| !d.acc.is_zero())
        .map(|(path, d)| DirectoryJson {
            path: path.to_string_lossy().to_string(),
            category_id: categories::def_by_index(d.category).id.to_string(),
            logical_bytes: d.acc.logical,
            allocated_bytes: d.acc.allocated,
            file_count: d.acc.files,
            directory_count: d.acc.dirs,
        })
        .collect();
    directories_out.sort_by_key(|d| std::cmp::Reverse(d.allocated_bytes));

    let roots = out
        .targets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let a = out.walk.targets.get(i).copied().unwrap_or_default();
            RootJson {
                path: t.path.to_string_lossy().to_string(),
                scanner: t.scanner.to_string(),
                category_hint: t.category_hint.to_string(),
                logical_bytes: a.logical,
                allocated_bytes: a.allocated,
                file_count: a.files,
                directory_count: a.dirs,
            }
        })
        .collect();

    ScanJson {
        version: SCHEMA_VERSION,
        command: "scan",
        timestamp: time::format_local(time::now_unix()),
        duration_ms: out.duration.as_millis() as u64,
        threads: out.threads,
        totals: TotalsJson::from(&out.walk.totals),
        categories: categories_out,
        directories: directories_out,
        roots,
        missing_roots: out
            .missing_targets
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        skipped: out
            .walk
            .skipped
            .iter()
            .map(|s| SkippedJson {
                path: s.path.to_string_lossy().to_string(),
                reason: s.reason.clone(),
                kind: s.kind_str().to_string(),
            })
            .collect(),
        skipped_count: out.walk.skipped_count,
        symlink_count: out.walk.totals.symlinks,
        hardlink_deduplicated_count: out.walk.totals.hardlinks_deduped,
        interrupted: out.walk.interrupted,
    }
}

#[derive(Serialize)]
pub struct SnapshotMetaJson {
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

impl From<&SnapshotMeta> for SnapshotMetaJson {
    fn from(m: &SnapshotMeta) -> Self {
        SnapshotMetaJson {
            id: m.id,
            created_at: m.created_at.clone(),
            created_at_local: m.created_at_local.clone(),
            total_logical_bytes: m.total_logical_bytes,
            total_allocated_bytes: m.total_allocated_bytes,
            file_count: m.file_count,
            directory_count: m.directory_count,
            symlink_count: m.symlink_count,
            skipped_count: m.skipped_count,
            duration_ms: m.duration_ms,
            app_version: m.app_version.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct SnapshotJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub snapshot: SnapshotMetaJson,
}

pub fn snapshot(meta: &SnapshotMeta) -> SnapshotJson {
    SnapshotJson {
        version: SCHEMA_VERSION,
        command: "snapshot",
        timestamp: time::format_local(time::now_unix()),
        snapshot: SnapshotMetaJson::from(meta),
    }
}

#[derive(Serialize)]
pub struct SnapshotsJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub snapshots: Vec<SnapshotMetaJson>,
}

pub fn snapshots(list: &[SnapshotMeta]) -> SnapshotsJson {
    SnapshotsJson {
        version: SCHEMA_VERSION,
        command: "snapshots",
        timestamp: time::format_local(time::now_unix()),
        snapshots: list.iter().map(SnapshotMetaJson::from).collect(),
    }
}

#[derive(Serialize)]
pub struct DiffTotalsJson {
    pub old_logical_bytes: u64,
    pub new_logical_bytes: u64,
    pub old_allocated_bytes: u64,
    pub new_allocated_bytes: u64,
    pub added_bytes: u64,
    pub removed_bytes: u64,
    pub net_change_bytes: i64,
}

#[derive(Serialize)]
pub struct CategoryDeltaJson {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub old_allocated_bytes: u64,
    pub new_allocated_bytes: u64,
    pub added_bytes: u64,
    pub removed_bytes: u64,
    pub net_change_bytes: i64,
}

#[derive(Serialize)]
pub struct DirectoryDeltaJson {
    pub path: String,
    pub category_id: String,
    pub old_allocated_bytes: u64,
    pub new_allocated_bytes: u64,
    pub added_bytes: u64,
    pub removed_bytes: u64,
    pub net_change_bytes: i64,
}

#[derive(Serialize)]
pub struct DiffJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub old: SnapshotMetaJson,
    pub new: SnapshotMetaJson,
    pub total: DiffTotalsJson,
    pub categories: Vec<CategoryDeltaJson>,
    pub directories: Vec<DirectoryDeltaJson>,
}

pub fn diff(d: &DiffResult) -> DiffJson {
    DiffJson {
        version: SCHEMA_VERSION,
        command: "diff",
        timestamp: time::format_local(time::now_unix()),
        old: SnapshotMetaJson::from(&d.old),
        new: SnapshotMetaJson::from(&d.new),
        total: DiffTotalsJson {
            old_logical_bytes: d.totals.old_logical_bytes,
            new_logical_bytes: d.totals.new_logical_bytes,
            old_allocated_bytes: d.totals.old_allocated_bytes,
            new_allocated_bytes: d.totals.new_allocated_bytes,
            added_bytes: d.totals.added_bytes,
            removed_bytes: d.totals.removed_bytes,
            net_change_bytes: d.totals.net_change_bytes,
        },
        categories: d
            .categories
            .iter()
            .map(|c| CategoryDeltaJson {
                id: c.id.clone(),
                name: c.name.clone(),
                parent_id: c.parent_id.clone(),
                old_allocated_bytes: c.old.allocated,
                new_allocated_bytes: c.new.allocated,
                added_bytes: c.added,
                removed_bytes: c.removed,
                net_change_bytes: c.net,
            })
            .collect(),
        directories: d
            .directories
            .iter()
            .map(|c| DirectoryDeltaJson {
                path: c.path.to_string_lossy().to_string(),
                category_id: c.category_id.clone(),
                old_allocated_bytes: c.old.allocated,
                new_allocated_bytes: c.new.allocated,
                added_bytes: c.added,
                removed_bytes: c.removed,
                net_change_bytes: c.net,
            })
            .collect(),
    }
}

#[derive(Serialize)]
pub struct ResolvedJson {
    pub kind: &'static str,
    pub id: Option<String>,
    pub name: String,
    pub path: Option<String>,
}

#[derive(Serialize)]
pub struct BreakdownJson {
    pub name: String,
    pub category_id: Option<String>,
    pub path: Option<String>,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

#[derive(Serialize)]
pub struct ExplainJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub query: String,
    pub resolved: ResolvedJson,
    pub title: String,
    pub total: TotalsJson,
    pub breakdown: Vec<BreakdownJson>,
    pub purpose: Option<String>,
    pub deletes_data: bool,
}

pub fn explain(e: &ExplainOutput, home: &Path) -> ExplainJson {
    ExplainJson {
        version: SCHEMA_VERSION,
        command: "explain",
        timestamp: time::format_local(time::now_unix()),
        query: e.query.clone(),
        resolved: ResolvedJson {
            kind: e.resolved_kind,
            id: e.resolved_id.clone(),
            name: e.title.clone(),
            path: e.resolved_path.as_ref().map(|p| display_path(p, home)),
        },
        title: e.title.clone(),
        total: TotalsJson {
            logical_bytes: e.total.logical,
            allocated_bytes: e.total.allocated,
            file_count: e.total.files,
            directory_count: e.total.dirs,
        },
        breakdown: e
            .breakdown
            .iter()
            .map(|r| BreakdownJson {
                name: r.name.clone(),
                category_id: r.category_id.clone(),
                path: r.path.as_ref().map(|p| display_path(p, home)),
                logical_bytes: r.val.logical,
                allocated_bytes: r.val.allocated,
                file_count: r.val.files,
                directory_count: r.val.dirs,
            })
            .collect(),
        purpose: e.purpose.clone(),
        deletes_data: false,
    }
}

#[derive(Serialize)]
pub struct DoctorDatabaseJson {
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
    pub writable: bool,
    pub schema_version: i64,
}

#[derive(Serialize)]
pub struct DoctorLocationJson {
    pub path: String,
    pub label: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct DoctorJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub data_dir: String,
    pub database: DoctorDatabaseJson,
    pub snapshot_count: i64,
    pub latest_snapshot: Option<SnapshotMetaJson>,
    pub locations: Vec<DoctorLocationJson>,
    pub protected_locations: Vec<DoctorLocationJson>,
    pub last_skipped: Vec<SkippedJson>,
    pub warnings: Vec<String>,
}

pub fn doctor(d: &DoctorReport, home: &Path) -> DoctorJson {
    let loc = |c: &crate::core::doctor::LocationCheck| DoctorLocationJson {
        path: display_path(&c.path, home),
        label: c.label.to_string(),
        status: c.status.to_string(),
        detail: c.detail.clone(),
    };
    DoctorJson {
        version: SCHEMA_VERSION,
        command: "doctor",
        timestamp: time::format_local(time::now_unix()),
        data_dir: display_path(&d.data_dir, home),
        database: DoctorDatabaseJson {
            path: display_path(&d.db_path, home),
            exists: d.db_exists,
            size_bytes: d.db_size_bytes,
            writable: d.db_writable,
            schema_version: d.schema_version,
        },
        snapshot_count: d.snapshot_count,
        latest_snapshot: d.latest.as_ref().map(SnapshotMetaJson::from),
        locations: d.locations.iter().map(loc).collect(),
        protected_locations: d.protected.iter().map(loc).collect(),
        last_skipped: d
            .latest_skipped
            .iter()
            .map(|(path, reason, kind)| SkippedJson {
                path: display_path(path, home),
                reason: reason.clone(),
                kind: kind.clone(),
            })
            .collect(),
        warnings: d.warnings.clone(),
    }
}

pub fn to_pretty<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}
