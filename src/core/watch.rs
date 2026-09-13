//! Filesystem change monitoring pipeline.
//!
//! Filesystem events are only hints. Each changed directory is measured with
//! a focused scan, compared with the last known size and recorded as an
//! event when the change is large enough. A full rescan is never triggered
//! by an event.

use crate::core::categories;
use crate::core::classify::Classifier;
use crate::core::scan::{self, ScanConfig, ScanTarget};
use crate::core::snapshot::EventDraft;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WatchEntry {
    pub category_id: String,
    pub allocated: u64,
}

pub type WatchState = HashMap<PathBuf, WatchEntry>;

pub struct BatchOutcome {
    pub events: Vec<EventDraft>,
    pub updated: Vec<(PathBuf, String, u64)>,
    pub removed: Vec<PathBuf>,
    pub measured: usize,
}

/// Keep only the top-most paths: descendants of another dirty path are
/// dropped so a change is measured once per batch.
pub fn collapse_dirty(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths.dedup();
    let mut out: Vec<PathBuf> = Vec::new();
    for path in paths {
        if out
            .iter()
            .any(|kept| path == *kept || path.starts_with(kept))
        {
            continue;
        }
        out.push(path);
    }
    out
}

pub fn is_excluded(path: &Path, exclusions: &[PathBuf]) -> bool {
    exclusions
        .iter()
        .any(|e| path == e.as_path() || path.starts_with(e))
}

/// Scan the watched locations once so the first event has a baseline.
pub fn build_baseline(
    home: &Path,
    targets: &[ScanTarget],
    depth: usize,
    threads: usize,
    exclusions: &[PathBuf],
) -> (WatchState, scan::ScanOutput) {
    let out = scan::run(ScanConfig {
        home,
        targets: targets.to_vec(),
        threads,
        progress: None,
        exclusions: exclusions.to_vec(),
        depth_override: Some(depth),
    });
    let mut state = WatchState::new();
    for (path, dir) in &out.walk.directories {
        state.insert(
            path.clone(),
            WatchEntry {
                category_id: categories::def_by_index(dir.category).id.to_string(),
                allocated: dir.acc.allocated,
            },
        );
    }

    // Empty directories have no size rows, but they still need a baseline:
    // otherwise their first change would be swallowed as a silent baseline.
    let classifier = Classifier::new(home);
    for target in targets {
        seed_directories(&target.path, depth, &classifier, exclusions, &mut state, 0);
    }
    (state, out)
}

fn seed_directories(
    dir: &Path,
    max_depth: usize,
    classifier: &Classifier,
    exclusions: &[PathBuf],
    state: &mut WatchState,
    depth: usize,
) {
    state
        .entry(dir.to_path_buf())
        .or_insert_with(|| WatchEntry {
            category_id: categories::def_by_index(classifier.classify(dir))
                .id
                .to_string(),
            allocated: 0,
        });
    if depth >= max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_excluded(&path, exclusions) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            seed_directories(
                path.as_path(),
                max_depth,
                classifier,
                exclusions,
                state,
                depth + 1,
            );
        }
    }
}

pub fn state_rows(state: &WatchState, paths: &[PathBuf]) -> Vec<(PathBuf, String, u64)> {
    paths
        .iter()
        .filter_map(|path| {
            state
                .get(path)
                .map(|e| (path.clone(), e.category_id.clone(), e.allocated))
        })
        .collect()
}

/// Resolve a dirty path to something we have a baseline for: the path
/// itself when known, otherwise its nearest known ancestor. Empty
/// directories have no baseline row, so their parent is measured instead.
fn nearest_known(state: &WatchState, path: &Path) -> (PathBuf, Option<WatchEntry>) {
    if let Some(entry) = state.get(path) {
        return (path.to_path_buf(), Some(entry.clone()));
    }
    let mut ancestor = path.parent();
    while let Some(candidate) = ancestor {
        if let Some(entry) = state.get(candidate) {
            return (candidate.to_path_buf(), Some(entry.clone()));
        }
        ancestor = candidate.parent();
    }
    (path.to_path_buf(), None)
}

/// Measure a debounced batch of dirty paths and produce change events.
#[allow(clippy::too_many_arguments)]
pub fn process_batch(
    dirty: &[PathBuf],
    home: &Path,
    classifier: &Classifier,
    state: &mut WatchState,
    min_change: u64,
    threads: usize,
    exclusions: &[PathBuf],
    timestamp: i64,
) -> BatchOutcome {
    let mut events = Vec::new();
    let mut updated = Vec::new();
    let mut removed = Vec::new();
    let mut measured = 0usize;

    // Map every dirty path to a path we have a baseline for, then collapse
    // so each measurement happens once per batch.
    let mut mapped: Vec<PathBuf> = Vec::new();
    let mut known: HashMap<PathBuf, Option<WatchEntry>> = HashMap::new();
    for path in dirty {
        if is_excluded(path, exclusions) {
            continue;
        }
        let (target, entry) = nearest_known(state, path);
        known.entry(target.clone()).or_insert(entry);
        mapped.push(target);
    }

    for path in collapse_dirty(mapped) {
        let previous_entry = known.get(&path).cloned().flatten();
        let category_id = previous_entry
            .as_ref()
            .map(|e| e.category_id.clone())
            .unwrap_or_else(|| {
                categories::def_by_index(classifier.classify(&path))
                    .id
                    .to_string()
            });

        if !path.is_dir() {
            // The directory vanished: everything under it is gone.
            if let Some(previous) = state.remove(&path) {
                removed.push(path.clone());
                if previous.allocated >= min_change && previous.allocated > 0 {
                    events.push(EventDraft {
                        timestamp_unix: timestamp,
                        kind: "removed",
                        path,
                        category_id: previous.category_id,
                        delta_bytes: -(previous.allocated as i64),
                        allocated_bytes: 0,
                        file_count: 0,
                        directory_count: 0,
                    });
                }
            }
            continue;
        }

        let out = scan::run(ScanConfig {
            home,
            targets: vec![ScanTarget {
                path: path.clone(),
                category_hint: "system.other",
                tracked_depth: 0,
                scanner: "watch",
            }],
            threads,
            progress: None,
            exclusions: exclusions.to_vec(),
            depth_override: Some(0),
        });
        measured += 1;

        let allocated = out.walk.totals.allocated;
        let previous = previous_entry.map(|e| e.allocated);
        state.insert(
            path.clone(),
            WatchEntry {
                category_id: category_id.clone(),
                allocated,
            },
        );
        updated.push((path.clone(), category_id.clone(), allocated));

        if let Some(previous) = previous {
            let delta = allocated as i64 - previous as i64;
            if delta != 0 && delta.unsigned_abs() >= min_change {
                events.push(EventDraft {
                    timestamp_unix: timestamp,
                    kind: if delta > 0 { "grow" } else { "shrink" },
                    path,
                    category_id,
                    delta_bytes: delta,
                    allocated_bytes: allocated,
                    file_count: out.walk.totals.files,
                    directory_count: out.walk.totals.dirs,
                });
            }
        }
    }

    BatchOutcome {
        events,
        updated,
        removed,
        measured,
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
                .join(format!("diskdrift-watch-{tag}-{}-{n}", std::process::id()));
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
    fn collapse_drops_descendants() {
        let paths = vec![
            PathBuf::from("/a/b/c"),
            PathBuf::from("/a/b"),
            PathBuf::from("/a/x"),
            PathBuf::from("/a/b"),
        ];
        assert_eq!(
            collapse_dirty(paths),
            vec![PathBuf::from("/a/b"), PathBuf::from("/a/x")]
        );
    }

    #[test]
    fn batch_measures_and_records_changes() {
        let tmp = Temp::new("batch");
        let dir = tmp.0.join("data");
        write(&dir.join("a.bin"), 5_000);
        let home = &tmp.0;
        let classifier = Classifier::new(home);
        let mut state = WatchState::new();
        let exclusions: Vec<PathBuf> = Vec::new();

        // First measurement only establishes a baseline.
        let first = process_batch(
            std::slice::from_ref(&dir),
            home,
            &classifier,
            &mut state,
            1,
            1,
            &exclusions,
            100,
        );
        assert!(first.events.is_empty());
        assert_eq!(first.measured, 1);
        let baseline = state[&dir].allocated;
        assert!(baseline > 0);

        // Growth is reported.
        write(&dir.join("b.bin"), 40_000);
        let grown = process_batch(
            std::slice::from_ref(&dir),
            home,
            &classifier,
            &mut state,
            1,
            1,
            &exclusions,
            200,
        );
        assert_eq!(grown.events.len(), 1);
        assert_eq!(grown.events[0].kind, "grow");
        assert!(grown.events[0].delta_bytes > 0);
        assert_eq!(grown.events[0].timestamp_unix, 200);

        // Shrinking is reported as a negative delta.
        std::fs::remove_file(dir.join("b.bin")).unwrap();
        let shrunk = process_batch(
            std::slice::from_ref(&dir),
            home,
            &classifier,
            &mut state,
            1,
            1,
            &exclusions,
            300,
        );
        assert_eq!(shrunk.events.len(), 1);
        assert!(shrunk.events[0].delta_bytes < 0);

        // Threshold filters small changes.
        std::fs::remove_file(dir.join("a.bin")).unwrap();
        let filtered = process_batch(
            std::slice::from_ref(&dir),
            home,
            &classifier,
            &mut state,
            u64::MAX,
            1,
            &exclusions,
            400,
        );
        assert!(filtered.events.is_empty());
    }

    #[test]
    fn paths_without_baseline_use_nearest_ancestor() {
        let tmp = Temp::new("ancestor");
        let parent = tmp.0.join("Library/Caches");
        std::fs::create_dir_all(&parent).unwrap();
        let home = &tmp.0;
        let classifier = Classifier::new(home);
        let mut state = WatchState::new();
        // Parent is known (empty), the new child directory is not.
        state.insert(
            parent.clone(),
            WatchEntry {
                category_id: "system.caches".to_string(),
                allocated: 0,
            },
        );
        let child = parent.join("Google");
        write(&child.join("c.bin"), 30_000);

        let out = process_batch(
            std::slice::from_ref(&child),
            home,
            &classifier,
            &mut state,
            1,
            1,
            &Vec::new(),
            100,
        );
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].kind, "grow");
        assert_eq!(out.events[0].path, parent);
        assert_eq!(out.events[0].category_id, "system.caches");
    }

    #[test]
    fn disappearing_directories_produce_removal_events() {
        let tmp = Temp::new("removed");
        let dir = tmp.0.join("gone");
        write(&dir.join("a.bin"), 20_000);
        let home = &tmp.0;
        let classifier = Classifier::new(home);
        let mut state = WatchState::new();
        let exclusions: Vec<PathBuf> = Vec::new();

        process_batch(
            std::slice::from_ref(&dir),
            home,
            &classifier,
            &mut state,
            1,
            1,
            &exclusions,
            100,
        );
        std::fs::remove_dir_all(&dir).unwrap();
        let out = process_batch(
            std::slice::from_ref(&dir),
            home,
            &classifier,
            &mut state,
            1,
            1,
            &exclusions,
            200,
        );
        assert_eq!(out.events.len(), 1);
        assert_eq!(out.events[0].kind, "removed");
        assert!(out.events[0].delta_bytes < 0);
        assert!(!state.contains_key(&dir));

        // Excluded paths are ignored.
        let excluded_dir = tmp.0.join("skip");
        write(&excluded_dir.join("x.bin"), 10_000);
        process_batch(
            std::slice::from_ref(&excluded_dir),
            home,
            &classifier,
            &mut state,
            1,
            1,
            std::slice::from_ref(&excluded_dir),
            300,
        );
        assert!(!state.contains_key(&excluded_dir));
    }
}
