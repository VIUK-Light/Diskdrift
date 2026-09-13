use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(
            home.join("Library/Containers"),
            "applications.containers",
            1,
            "containers",
        ),
        super::target(
            home.join("Library/Group Containers"),
            "applications.group_containers",
            1,
            "containers",
        ),
    ]
}
