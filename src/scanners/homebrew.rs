use crate::core::scan::ScanTarget;
use std::path::{Path, PathBuf};

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(
            home.join("Library/Caches/Homebrew"),
            "developer.homebrew",
            0,
            "homebrew",
        ),
        super::target(
            PathBuf::from("/opt/homebrew"),
            "developer.homebrew",
            2,
            "homebrew",
        ),
        super::target(
            PathBuf::from("/usr/local/Cellar"),
            "developer.homebrew",
            2,
            "homebrew",
        ),
        super::target(
            PathBuf::from("/usr/local/Caskroom"),
            "developer.homebrew",
            1,
            "homebrew",
        ),
        super::target(
            PathBuf::from("/usr/local/Homebrew"),
            "developer.homebrew",
            1,
            "homebrew",
        ),
    ]
}
