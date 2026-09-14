//! Stable JSON output. Schema version is `version: 1`.
//!
//! Field order is declaration order (serde serializes structs in order) and
//! the schema is intended to be consumed by future GUIs and scripts.

use crate::core::categories;
use crate::core::diff::DiffResult;
use crate::core::doctor::DoctorReport;
use crate::core::explain::{ExplainOutput, display_path};
use crate::core::fs::{self, Accum};
use crate::core::history::HistoryDay;
use crate::core::scan::ScanOutput;
use crate::core::snapshot::{CatVal, EventDraft, EventRow, SnapshotMeta};
use crate::core::time;
use crate::core::what_happened::{Report, Window};
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

    let directories_out = directory_list(out);

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
pub struct SnapshotShowJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub snapshot: SnapshotMetaJson,
    pub categories: Vec<CategoryJson>,
}

pub fn snapshot_show(meta: &SnapshotMeta, rows: &[(usize, CatVal)]) -> SnapshotShowJson {
    SnapshotShowJson {
        version: SCHEMA_VERSION,
        command: "snapshot show",
        timestamp: time::format_local(time::now_unix()),
        snapshot: SnapshotMetaJson::from(meta),
        categories: rows
            .iter()
            .map(|(idx, val)| {
                let def = categories::def_by_index(*idx);
                CategoryJson {
                    id: def.id.to_string(),
                    name: def.name.to_string(),
                    parent_id: def.parent.map(|p| p.to_string()),
                    logical_bytes: val.logical,
                    allocated_bytes: val.allocated,
                    file_count: val.files,
                    directory_count: val.dirs,
                }
            })
            .collect(),
    }
}

#[derive(Serialize)]
pub struct SnapshotDeletedJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub deleted: SnapshotMetaJson,
}

pub fn snapshot_deleted(meta: &SnapshotMeta) -> SnapshotDeletedJson {
    SnapshotDeletedJson {
        version: SCHEMA_VERSION,
        command: "snapshot delete",
        timestamp: time::format_local(time::now_unix()),
        deleted: SnapshotMetaJson::from(meta),
    }
}

#[derive(Serialize)]
pub struct HistoryCategoryJson {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct HistoryDayJson {
    pub date: String,
    pub snapshot_id: i64,
    pub snapshot_count: usize,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub change_bytes: Option<i64>,
}

#[derive(Serialize)]
pub struct HistoryJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub category: Option<HistoryCategoryJson>,
    pub days: Vec<HistoryDayJson>,
}

pub fn history(days: &[HistoryDay], category: Option<usize>) -> HistoryJson {
    HistoryJson {
        version: SCHEMA_VERSION,
        command: "history",
        timestamp: time::format_local(time::now_unix()),
        category: category.map(|idx| {
            let def = categories::def_by_index(idx);
            HistoryCategoryJson {
                id: def.id.to_string(),
                name: def.name.to_string(),
            }
        }),
        days: days
            .iter()
            .map(|d| HistoryDayJson {
                date: d.date.clone(),
                snapshot_id: d.snapshot_id,
                snapshot_count: d.snapshot_count,
                allocated_bytes: d.allocated,
                logical_bytes: d.logical,
                change_bytes: d.change,
            })
            .collect(),
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
pub struct DoctorConfigJson {
    pub path: String,
    pub exists: bool,
    pub error: Option<String>,
    pub exclude: Vec<String>,
}

#[derive(Serialize)]
pub struct DoctorJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub data_dir: String,
    pub database: DoctorDatabaseJson,
    pub config: DoctorConfigJson,
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
        config: DoctorConfigJson {
            path: display_path(&d.config_path, home),
            exists: d.config_exists,
            error: d.config_error.clone(),
            exclude: d.excluded.iter().map(|p| display_path(p, home)).collect(),
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

fn directory_list(out: &ScanOutput) -> Vec<DirectoryJson> {
    let mut directories: Vec<DirectoryJson> = out
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
    directories.sort_by_key(|d| std::cmp::Reverse(d.allocated_bytes));
    directories
}

#[derive(Serialize)]
pub struct TopJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub totals: TotalsJson,
    pub directories: Vec<DirectoryJson>,
}

pub fn top(out: &ScanOutput, limit: usize) -> TopJson {
    let mut directories = directory_list(out);
    directories.truncate(limit.max(1));
    TopJson {
        version: SCHEMA_VERSION,
        command: "top",
        timestamp: time::format_local(time::now_unix()),
        totals: TotalsJson::from(&out.walk.totals),
        directories,
    }
}

#[derive(Serialize)]
pub struct EventJson {
    pub id: i64,
    pub timestamp: String,
    pub timestamp_unix: i64,
    pub kind: String,
    pub path: String,
    pub category_id: String,
    pub delta_bytes: i64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

fn event_from_row(event: &EventRow, home: &Path) -> EventJson {
    EventJson {
        id: event.id,
        timestamp: event.timestamp_local.clone(),
        timestamp_unix: event.timestamp_unix,
        kind: event.kind.clone(),
        path: display_path(&event.path, home),
        category_id: event.category_id.clone(),
        delta_bytes: event.delta_bytes,
        allocated_bytes: event.allocated_bytes,
        file_count: event.file_count,
        directory_count: event.directory_count,
    }
}

#[derive(Serialize)]
pub struct EventsJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub since_unix: Option<i64>,
    pub events: Vec<EventJson>,
}

pub fn events(rows: &[EventRow], since_unix: Option<i64>, home: &Path) -> EventsJson {
    EventsJson {
        version: SCHEMA_VERSION,
        command: "events",
        timestamp: time::format_local(time::now_unix()),
        since_unix,
        events: rows.iter().map(|e| event_from_row(e, home)).collect(),
    }
}

#[derive(Serialize)]
pub struct WatchEventJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub event: WatchEventInnerJson,
}

#[derive(Serialize)]
pub struct WatchEventInnerJson {
    pub timestamp: String,
    pub timestamp_unix: i64,
    pub kind: &'static str,
    pub path: String,
    pub category_id: String,
    pub delta_bytes: i64,
    pub allocated_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

pub fn watch_event(event: &EventDraft, home: &Path) -> WatchEventJson {
    WatchEventJson {
        version: SCHEMA_VERSION,
        command: "watch",
        timestamp: time::format_local(time::now_unix()),
        event: WatchEventInnerJson {
            timestamp: time::format_local(event.timestamp_unix),
            timestamp_unix: event.timestamp_unix,
            kind: event.kind,
            path: display_path(&event.path, home),
            category_id: event.category_id.clone(),
            delta_bytes: event.delta_bytes,
            allocated_bytes: event.allocated_bytes,
            file_count: event.file_count,
            directory_count: event.directory_count,
        },
    }
}

#[derive(Serialize)]
pub struct IncidentJson {
    pub label: String,
    pub category_id: String,
    pub path: Option<String>,
    pub delta_bytes: i64,
    pub grow_bytes: u64,
    pub shrink_bytes: u64,
    pub first_event: String,
    pub last_event: String,
    pub event_count: usize,
}

#[derive(Serialize)]
pub struct WhatHappenedTotalJson {
    pub delta_bytes: i64,
    pub grow_bytes: u64,
    pub shrink_bytes: u64,
    pub event_count: usize,
}

#[derive(Serialize)]
pub struct WhatHappenedJson {
    pub version: u32,
    pub command: &'static str,
    pub timestamp: String,
    pub window: String,
    pub from: String,
    pub to: String,
    pub from_unix: i64,
    pub to_unix: i64,
    pub total: WhatHappenedTotalJson,
    pub incidents: Vec<IncidentJson>,
}

pub fn what_happened(report: &Report, window: &Window, home: &Path) -> WhatHappenedJson {
    WhatHappenedJson {
        version: SCHEMA_VERSION,
        command: "what-happened",
        timestamp: time::format_local(time::now_unix()),
        window: window.label.clone(),
        from: time::format_local(window.from_unix),
        to: time::format_local(window.to_unix),
        from_unix: window.from_unix,
        to_unix: window.to_unix,
        total: WhatHappenedTotalJson {
            delta_bytes: report.total_delta,
            grow_bytes: report.total_grow,
            shrink_bytes: report.total_shrink,
            event_count: report.event_count,
        },
        incidents: report
            .incidents
            .iter()
            .map(|incident| IncidentJson {
                label: incident.label.clone(),
                category_id: incident.category_id.clone(),
                path: incident.path.as_ref().map(|p| display_path(p, home)),
                delta_bytes: incident.delta_bytes,
                grow_bytes: incident.grow_bytes,
                shrink_bytes: incident.shrink_bytes,
                first_event: time::format_local(incident.first_unix),
                last_event: time::format_local(incident.last_unix),
                event_count: incident.event_count,
            })
            .collect(),
    }
}

pub fn to_json_line<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

pub fn to_pretty<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}
