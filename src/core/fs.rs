//! Streaming filesystem walker.
//!
//! Design notes:
//! - Files are never kept in memory; only aggregates are accumulated.
//! - Symbolic links are never followed (cycles / double counting / escape).
//! - Hard links are de-duplicated by (device, inode) so the same bytes are
//!   counted once.
//! - Permission errors and disappearing files are warnings, never fatal.
//! - Ctrl+C stops the walk safely; partial results are returned.

use crate::core::categories::{self, CATEGORIES};
use crate::core::classify::Classifier;
use crate::core::interrupt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

const BLOCK_SIZE: u64 = 512;
const SKIP_EXAMPLES_PER_WORKER: usize = 200;
const INTERRUPT_CHECK_EVERY: u32 = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Accum {
    pub logical: u64,
    pub allocated: u64,
    pub files: u64,
    pub dirs: u64,
    pub symlinks: u64,
    pub hardlinks_deduped: u64,
}

impl Accum {
    pub fn merge(&mut self, other: &Accum) {
        self.logical = self.logical.saturating_add(other.logical);
        self.allocated = self.allocated.saturating_add(other.allocated);
        self.files = self.files.saturating_add(other.files);
        self.dirs = self.dirs.saturating_add(other.dirs);
        self.symlinks = self.symlinks.saturating_add(other.symlinks);
        self.hardlinks_deduped = self
            .hardlinks_deduped
            .saturating_add(other.hardlinks_deduped);
    }

    pub fn is_zero(&self) -> bool {
        self.logical == 0
            && self.allocated == 0
            && self.files == 0
            && self.dirs == 0
            && self.symlinks == 0
            && self.hardlinks_deduped == 0
    }
}

#[derive(Debug, Clone)]
pub struct DirEntryAgg {
    pub category: usize,
    pub acc: Accum,
}

#[derive(Debug, Clone)]
pub struct WalkTarget {
    pub path: PathBuf,
    pub tracked_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipKind {
    Permission,
    NotFound,
    Other,
}

#[derive(Debug, Clone)]
pub struct SkippedLocation {
    pub path: PathBuf,
    pub reason: String,
    pub kind: SkipKind,
}

impl SkippedLocation {
    pub fn kind_str(&self) -> &'static str {
        match self.kind {
            SkipKind::Permission => "permission",
            SkipKind::NotFound => "missing",
            SkipKind::Other => "error",
        }
    }
}

pub struct WalkResult {
    /// Index-aligned with `CATEGORIES`.
    pub categories: Vec<Accum>,
    pub directories: HashMap<PathBuf, DirEntryAgg>,
    /// Index-aligned with the `targets` passed to `walk`.
    pub targets: Vec<Accum>,
    pub totals: Accum,
    pub skipped: Vec<SkippedLocation>,
    pub skipped_count: u64,
    pub interrupted: bool,
    pub duration: Duration,
}

impl WalkResult {
    fn empty(duration: Duration) -> Self {
        WalkResult {
            categories: vec![Accum::default(); CATEGORIES.len()],
            directories: HashMap::new(),
            targets: Vec::new(),
            totals: Accum::default(),
            skipped: Vec::new(),
            skipped_count: 0,
            interrupted: false,
            duration,
        }
    }
}

/// Live counters shared with the progress renderer.
pub struct ProgressCounters {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub category_bytes: Vec<AtomicU64>,
    pub category_files: Vec<AtomicU64>,
}

impl ProgressCounters {
    pub fn new() -> Self {
        let n = CATEGORIES.len();
        let mut category_bytes = Vec::with_capacity(n);
        let mut category_files = Vec::with_capacity(n);
        for _ in 0..n {
            category_bytes.push(AtomicU64::new(0));
            category_files.push(AtomicU64::new(0));
        }
        ProgressCounters {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            category_bytes,
            category_files,
        }
    }

    fn add_file(&self, category: usize, bytes: u64) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.category_bytes[category].fetch_add(bytes, Ordering::Relaxed);
        self.category_files[category].fetch_add(1, Ordering::Relaxed);
    }

    fn add_dir(&self) {
        self.dirs.fetch_add(1, Ordering::Relaxed);
    }

    /// Top categories by allocated bytes: (category index, bytes, files).
    pub fn top_categories(&self, limit: usize) -> Vec<(usize, u64, u64)> {
        let mut v: Vec<(usize, u64, u64)> = (0..CATEGORIES.len())
            .map(|i| {
                (
                    i,
                    self.category_bytes[i].load(Ordering::Relaxed),
                    self.category_files[i].load(Ordering::Relaxed),
                )
            })
            .filter(|(_, b, _)| *b > 0)
            .collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.1));
        v.truncate(limit);
        v
    }
}

impl Default for ProgressCounters {
    fn default() -> Self {
        Self::new()
    }
}

pub fn walk(
    targets: &[WalkTarget],
    classifier: &Classifier,
    threads: usize,
    progress: Option<&ProgressCounters>,
    exclusions: &[PathBuf],
) -> WalkResult {
    let started = Instant::now();
    if targets.is_empty() {
        return WalkResult::empty(started.elapsed());
    }

    let mut seeds = Vec::with_capacity(targets.len());
    for (ti, t) in targets.iter().enumerate() {
        let category = classifier.classify(&t.path);
        seeds.push(WorkItem {
            path: t.path.clone(),
            target: ti,
            depth: 0,
            category,
        });
    }

    let queue = Queue::new(seeds);
    let hardlinks: Mutex<HashSet<(u64, u64)>> = Mutex::new(HashSet::new());
    let n_threads = threads.max(1);

    let locals: Vec<LocalAgg> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(n_threads);
        for _ in 0..n_threads {
            let queue = &queue;
            let hardlinks = &hardlinks;
            handles.push(scope.spawn(move || {
                worker(queue, targets, classifier, hardlinks, progress, exclusions)
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });

    let duration = started.elapsed();
    merge(locals, targets.len(), duration)
}

fn merge(locals: Vec<LocalAgg>, target_count: usize, duration: Duration) -> WalkResult {
    let mut result = WalkResult::empty(duration);
    result.targets = vec![Accum::default(); target_count];
    for local in locals {
        for (i, acc) in local.categories.iter().enumerate() {
            result.categories[i].merge(acc);
        }
        for (i, acc) in local.targets.iter().enumerate() {
            result.targets[i].merge(acc);
        }
        result.totals.merge(&local.totals);
        result.skipped_count += local.skipped_count;
        result.skipped.extend(local.skipped);
        result.interrupted |= local.interrupted;
        for (path, dir) in local.dirs {
            let entry = result
                .directories
                .entry(path)
                .or_insert_with(|| DirEntryAgg {
                    category: dir.category,
                    acc: Accum::default(),
                });
            entry.acc.merge(&dir.acc);
        }
    }
    result.skipped.sort_by(|a, b| a.path.cmp(&b.path));
    result.skipped.truncate(1000);
    result
}

#[derive(Default)]
struct LocalAgg {
    categories: Vec<Accum>,
    dirs: HashMap<PathBuf, DirEntryAgg>,
    targets: Vec<Accum>,
    totals: Accum,
    skipped: Vec<SkippedLocation>,
    skipped_count: u64,
    interrupted: bool,
}

impl LocalAgg {
    fn new(target_count: usize) -> Self {
        LocalAgg {
            categories: vec![Accum::default(); CATEGORIES.len()],
            dirs: HashMap::new(),
            targets: vec![Accum::default(); target_count],
            totals: Accum::default(),
            skipped: Vec::new(),
            skipped_count: 0,
            interrupted: false,
        }
    }

    fn record_io_skip(&mut self, path: &Path, e: &io::Error) {
        let kind = match e.kind() {
            io::ErrorKind::PermissionDenied => SkipKind::Permission,
            io::ErrorKind::NotFound => SkipKind::NotFound,
            _ => SkipKind::Other,
        };
        self.record_skip(path, io_reason(e), kind);
    }

    fn record_skip(&mut self, path: &Path, reason: String, kind: SkipKind) {
        self.skipped_count += 1;
        if self.skipped.len() < SKIP_EXAMPLES_PER_WORKER {
            self.skipped.push(SkippedLocation {
                path: path.to_path_buf(),
                reason,
                kind,
            });
        }
    }

    fn dir_bucket(&mut self, key: &Path, category: usize) -> &mut DirEntryAgg {
        self.dirs
            .entry(key.to_path_buf())
            .or_insert_with(|| DirEntryAgg {
                category,
                acc: Accum::default(),
            })
    }
}

fn is_excluded(path: &Path, exclusions: &[PathBuf]) -> bool {
    exclusions
        .iter()
        .any(|e| path == e.as_path() || path.starts_with(e))
}

fn io_reason(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        io::ErrorKind::NotFound => "no longer exists".to_string(),
        _ => e.to_string(),
    }
}

#[derive(Debug, Clone)]
struct WorkItem {
    path: PathBuf,
    target: usize,
    depth: usize,
    category: usize,
}

struct QueueState {
    items: VecDeque<WorkItem>,
    queued: u64,
    in_flight: u64,
    done: bool,
}

struct Queue {
    state: Mutex<QueueState>,
    cv: Condvar,
}

impl Queue {
    fn new(seeds: Vec<WorkItem>) -> Self {
        let queued = seeds.len() as u64;
        Queue {
            state: Mutex::new(QueueState {
                items: seeds.into_iter().collect(),
                queued,
                in_flight: 0,
                done: false,
            }),
            cv: Condvar::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, item: WorkItem) {
        let mut s = self.lock();
        if s.done {
            return;
        }
        s.items.push_back(item);
        s.queued += 1;
        self.cv.notify_one();
    }

    fn pop(&self) -> Option<WorkItem> {
        let mut s = self.lock();
        loop {
            if let Some(item) = s.items.pop_front() {
                s.queued -= 1;
                s.in_flight += 1;
                return Some(item);
            }
            if s.in_flight == 0 {
                s.done = true;
                self.cv.notify_all();
                return None;
            }
            s = self.cv.wait(s).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn finish(&self) {
        let mut s = self.lock();
        s.in_flight = s.in_flight.saturating_sub(1);
        if s.in_flight == 0 && s.queued == 0 {
            s.done = true;
            self.cv.notify_all();
        }
    }

    fn abort(&self) {
        let mut s = self.lock();
        s.items.clear();
        s.queued = 0;
        s.done = true;
        self.cv.notify_all();
    }
}

fn worker(
    queue: &Queue,
    targets: &[WalkTarget],
    classifier: &Classifier,
    hardlinks: &Mutex<HashSet<(u64, u64)>>,
    progress: Option<&ProgressCounters>,
    exclusions: &[PathBuf],
) -> LocalAgg {
    let mut local = LocalAgg::new(targets.len());
    while let Some(item) = queue.pop() {
        if interrupt::interrupted() {
            local.interrupted = true;
            queue.abort();
            // The popped item still counts as in-flight; release it before
            // leaving, otherwise other workers wait forever.
            queue.finish();
            break;
        }
        let target = &targets[item.target];
        let track_key = tracked_key(&target.path, &item.path, item.depth, target.tracked_depth);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process_dir(
                &item, &track_key, classifier, hardlinks, progress, queue, exclusions, &mut local,
            );
        }));
        if outcome.is_err() {
            local.record_skip(
                &item.path,
                "internal error while scanning".to_string(),
                SkipKind::Other,
            );
        }
        queue.finish();
    }
    local
}

#[allow(clippy::too_many_arguments)]
fn process_dir(
    item: &WorkItem,
    track_key: &Path,
    classifier: &Classifier,
    hardlinks: &Mutex<HashSet<(u64, u64)>>,
    progress: Option<&ProgressCounters>,
    queue: &Queue,
    exclusions: &[PathBuf],
    local: &mut LocalAgg,
) {
    let read = match std::fs::read_dir(&item.path) {
        Ok(r) => r,
        Err(e) => {
            local.record_io_skip(&item.path, &e);
            return;
        }
    };

    let mut checks: u32 = 0;
    for entry in read {
        checks += 1;
        if checks >= INTERRUPT_CHECK_EVERY {
            checks = 0;
            if interrupt::interrupted() {
                local.interrupted = true;
                queue.abort();
                return;
            }
        }

        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                local.record_io_skip(&item.path, &e);
                continue;
            }
        };
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                local.record_io_skip(&path, &e);
                continue;
            }
        };

        if ft.is_symlink() {
            // Never follows links. Counted for transparency only.
            local.totals.symlinks += 1;
            local.categories[item.category].symlinks += 1;
            local.dir_bucket(track_key, item.category).acc.symlinks += 1;
            local.targets[item.target].symlinks += 1;
            continue;
        }

        if ft.is_dir() {
            if is_excluded(&path, exclusions) {
                // Intentional exclusion (e.g. DiskDrift's own database):
                // silent, not a warning.
                continue;
            }
            let cat = classifier.classify(&path);
            local.totals.dirs += 1;
            local.categories[cat].dirs += 1;
            local.targets[item.target].dirs += 1;
            local.dir_bucket(track_key, item.category).acc.dirs += 1;
            if let Some(c) = progress {
                c.add_dir();
            }
            queue.push(WorkItem {
                path,
                target: item.target,
                depth: item.depth + 1,
                category: cat,
            });
            continue;
        }

        if !ft.is_file() {
            // Sockets, FIFOs and devices: counted but have no meaningful size.
            local.totals.files += 1;
            local.categories[item.category].files += 1;
            local.targets[item.target].files += 1;
            local.dir_bucket(track_key, item.category).acc.files += 1;
            continue;
        }

        let md = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                local.record_io_skip(&path, &e);
                continue;
            }
        };
        let logical = md.size();
        let allocated = md.blocks().saturating_mul(BLOCK_SIZE);

        // Hard link de-duplication: count bytes once per (dev, inode).
        let mut deduped = false;
        if md.nlink() > 1 {
            let key = (md.dev(), md.ino());
            let mut set = hardlinks.lock().unwrap_or_else(|e| e.into_inner());
            if !set.insert(key) {
                deduped = true;
            }
        }

        local.totals.files += 1;
        local.categories[item.category].files += 1;
        local.targets[item.target].files += 1;
        local.dir_bucket(track_key, item.category).acc.files += 1;

        if deduped {
            local.totals.hardlinks_deduped += 1;
            local.categories[item.category].hardlinks_deduped += 1;
            local
                .dir_bucket(track_key, item.category)
                .acc
                .hardlinks_deduped += 1;
            local.targets[item.target].hardlinks_deduped += 1;
            if let Some(c) = progress {
                c.add_file(item.category, 0);
            }
        } else {
            local.totals.logical = local.totals.logical.saturating_add(logical);
            local.totals.allocated = local.totals.allocated.saturating_add(allocated);
            local.categories[item.category].logical = local.categories[item.category]
                .logical
                .saturating_add(logical);
            local.categories[item.category].allocated = local.categories[item.category]
                .allocated
                .saturating_add(allocated);
            local.targets[item.target].logical =
                local.targets[item.target].logical.saturating_add(logical);
            local.targets[item.target].allocated = local.targets[item.target]
                .allocated
                .saturating_add(allocated);
            let bucket = local.dir_bucket(track_key, item.category);
            bucket.acc.logical = bucket.acc.logical.saturating_add(logical);
            bucket.acc.allocated = bucket.acc.allocated.saturating_add(allocated);
            if let Some(c) = progress {
                c.add_file(item.category, allocated);
            }
        }
    }
}

/// The aggregate bucket a file belongs to: at most `tracked_depth` path
/// components below the scan root.
fn tracked_key(root: &Path, dir: &Path, depth: usize, tracked_depth: usize) -> PathBuf {
    if depth <= tracked_depth {
        return dir.to_path_buf();
    }
    let rel = dir.strip_prefix(root).unwrap_or(Path::new(""));
    let mut out = root.to_path_buf();
    for comp in rel.components().take(tracked_depth) {
        out.push(comp.as_os_str());
    }
    out
}

/// Rolled-up category totals: every category gets the sum of its whole subtree.
pub fn rollup_categories(leaf: &[Accum]) -> Vec<Accum> {
    let mut out = vec![Accum::default(); CATEGORIES.len()];
    for (idx, _) in CATEGORIES.iter().enumerate() {
        let mut acc = Accum::default();
        for (leaf_idx, value) in leaf.iter().enumerate() {
            if categories::is_descendant_or_self(leaf_idx, idx) {
                acc.merge(value);
            }
        }
        out[idx] = acc;
    }
    out
}
