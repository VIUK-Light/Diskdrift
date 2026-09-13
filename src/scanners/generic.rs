use crate::core::scan::ScanTarget;
use std::path::PathBuf;

/// Targets supplied with `--root <path>`: walked with classification rules,
/// tracked two levels deep.
pub fn target_for(path: PathBuf) -> ScanTarget {
    ScanTarget {
        path,
        category_hint: "system.other",
        tracked_depth: 2,
        scanner: "generic",
    }
}
