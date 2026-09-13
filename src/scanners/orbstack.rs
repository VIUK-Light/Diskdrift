use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(home.join(".orbstack"), "developer.orbstack", 2, "orbstack"),
        super::target(
            home.join("Library/Group Containers/HUAQ24HBR6.dev.orbstack"),
            "developer.orbstack",
            1,
            "orbstack",
        ),
        super::target(
            home.join("Library/Application Support/OrbStack"),
            "developer.orbstack",
            1,
            "orbstack",
        ),
    ]
}
