use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(
            home.join("Library/Containers/com.docker.docker"),
            "developer.docker",
            1,
            "docker",
        ),
        super::target(
            home.join("Library/Group Containers/group.com.docker"),
            "developer.docker",
            1,
            "docker",
        ),
        super::target(home.join(".docker"), "developer.docker", 1, "docker"),
    ]
}
