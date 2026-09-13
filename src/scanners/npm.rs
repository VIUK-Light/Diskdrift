use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![super::target(home.join(".npm"), "developer.npm", 1, "npm")]
}
