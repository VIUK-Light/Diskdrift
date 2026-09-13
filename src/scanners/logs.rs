use crate::core::scan::ScanTarget;
use std::path::{Path, PathBuf};

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(home.join("Library/Logs"), "system.logs", 1, "logs"),
        super::target(PathBuf::from("/Library/Logs"), "system.logs", 1, "logs"),
    ]
}
