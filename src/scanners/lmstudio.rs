use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![
        super::target(home.join(".lmstudio"), "ai.lmstudio", 1, "lmstudio"),
        super::target(home.join(".cache/lm-studio"), "ai.lmstudio", 1, "lmstudio"),
        super::target(
            home.join("Library/Application Support/LM Studio"),
            "ai.lmstudio",
            2,
            "lmstudio",
        ),
    ]
}
