use crate::core::scan::ScanTarget;
use std::path::{Path, PathBuf};

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(home.join("Library/Caches"), "system.caches", 2, "caches"),
        super::target(home.join(".cache"), "system.caches", 2, "caches"),
        super::target(
            PathBuf::from("/Library/Caches"),
            "system.caches",
            1,
            "caches",
        ),
    ]
}
