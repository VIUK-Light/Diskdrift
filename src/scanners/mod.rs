//! Scanner modules.
//!
//! Each module only declares *where* its data lives (and a category hint).
//! Classification rules live in `core::classify`, so adding a new tool is a
//! local change here and never touches the scanner core.

use crate::core::scan::ScanTarget;
use std::path::{Path, PathBuf};

pub mod app_support;
pub mod caches;
pub mod containers;
pub mod docker;
pub mod generic;
pub mod homebrew;
pub mod huggingface;
pub mod lmstudio;
pub mod logs;
pub mod npm;
pub mod ollama;
pub mod pnpm;
pub mod xcode;

fn target(
    path: PathBuf,
    category_hint: &'static str,
    tracked_depth: usize,
    scanner: &'static str,
) -> ScanTarget {
    ScanTarget {
        path,
        category_hint,
        tracked_depth,
        scanner,
    }
}

/// All default scan targets. Overlapping paths are resolved later by
/// `core::scan::prepare_targets`.
pub fn default_targets(home: &Path) -> Vec<ScanTarget> {
    let mut all = Vec::new();
    all.extend(xcode::targets(home));
    all.extend(homebrew::targets(home));
    all.extend(docker::targets(home));
    all.extend(ollama::targets(home));
    all.extend(huggingface::targets(home));
    all.extend(lmstudio::targets(home));
    all.extend(npm::targets(home));
    all.extend(pnpm::targets(home));
    all.extend(caches::targets(home));
    all.extend(app_support::targets(home));
    all.extend(containers::targets(home));
    all.extend(logs::targets(home));
    all
}
