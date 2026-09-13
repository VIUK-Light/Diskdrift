use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(home.join("Library/pnpm"), "developer.pnpm", 1, "pnpm"),
        super::target(home.join(".pnpm-store"), "developer.pnpm", 1, "pnpm"),
        super::target(home.join(".local/share/pnpm"), "developer.pnpm", 1, "pnpm"),
    ]
}
