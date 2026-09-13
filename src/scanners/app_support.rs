use crate::core::scan::ScanTarget;
use std::path::{Path, PathBuf};

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(
            home.join("Library/Application Support"),
            "applications.app_support",
            2,
            "app_support",
        ),
        super::target(
            PathBuf::from("/Library/Application Support"),
            "applications.app_support",
            1,
            "app_support",
        ),
    ]
}
