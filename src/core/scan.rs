//! Scan orchestration: target preparation and walking.

use crate::core::classify::Classifier;
use crate::core::fs::{self, ProgressCounters, WalkResult};
use crate::core::time;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A location DiskDrift knows how to scan and why.
#[derive(Debug, Clone)]
pub struct ScanTarget {
    pub path: PathBuf,
    pub category_hint: &'static str,
    pub tracked_depth: usize,
    pub scanner: &'static str,
}

pub struct ScanConfig<'a> {
    pub home: &'a Path,
    pub targets: Vec<ScanTarget>,
    pub threads: usize,
    pub progress: Option<&'a ProgressCounters>,
    /// Paths that must never be scanned (e.g. DiskDrift's own database).
    pub exclusions: Vec<PathBuf>,
}

pub struct ScanOutput {
    pub started_at: i64,
    pub duration: Duration,
    pub threads: usize,
    pub targets: Vec<ScanTarget>,
    pub missing_targets: Vec<PathBuf>,
    pub walk: WalkResult,
}

/// Default parallelism: bounded so we never thrash the disk.
pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
}

/// De-duplicate nested targets, then keep only locations that exist.
///
/// Overlap handling matters: `~/Library/Caches` already covers
/// `~/Library/Caches/Homebrew`, and walking both would double count.
pub fn prepare_targets(mut specs: Vec<ScanTarget>) -> (Vec<ScanTarget>, Vec<PathBuf>) {
    specs.sort_by(|a, b| a.path.cmp(&b.path));
    let mut kept: Vec<ScanTarget> = Vec::with_capacity(specs.len());
    for t in specs {
        if kept
            .iter()
            .any(|k| t.path == k.path || t.path.starts_with(&k.path))
        {
            continue;
        }
        kept.push(t);
    }

    let mut existing = Vec::new();
    let mut missing = Vec::new();
    for t in kept {
        match std::fs::metadata(&t.path) {
            Ok(md) if md.is_dir() => existing.push(t),
            _ => missing.push(t.path),
        }
    }
    (existing, missing)
}

pub fn run(config: ScanConfig<'_>) -> ScanOutput {
    let started_at = time::now_unix();
    let classifier = Classifier::new(config.home);
    let (targets, missing_targets) = prepare_targets(config.targets);
    let walk_targets: Vec<fs::WalkTarget> = targets
        .iter()
        .map(|t| fs::WalkTarget {
            path: t.path.clone(),
            tracked_depth: t.tracked_depth,
        })
        .collect();
    let walk = fs::walk(
        &walk_targets,
        &classifier,
        config.threads,
        config.progress,
        &config.exclusions,
    );
    ScanOutput {
        started_at,
        duration: walk.duration,
        threads: config.threads,
        targets,
        missing_targets,
        walk,
    }
}

/// Categories whose rolled-up totals are non-zero, as (index, value).
pub fn nonempty_categories(rolled: &[fs::Accum]) -> Vec<(usize, fs::Accum)> {
    rolled
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.is_zero())
        .map(|(i, a)| (i, *a))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(path: &str, scanner: &'static str) -> ScanTarget {
        ScanTarget {
            path: PathBuf::from(path),
            category_hint: "system.other",
            tracked_depth: 1,
            scanner,
        }
    }

    #[test]
    fn nested_targets_are_dropped() {
        let specs = vec![
            t("/tmp", "generic"),
            t("/tmp/nested", "generic"),
            t("/other", "generic"),
        ];
        let (kept, missing) = prepare_targets(specs);
        let paths: Vec<_> = kept.iter().map(|t| t.path.clone()).collect();
        assert_eq!(paths, vec![PathBuf::from("/tmp")]);
        assert_eq!(missing, vec![PathBuf::from("/other")]);
    }
}
