use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    vec![super::target(
        home.join(".cache/huggingface"),
        "ai.huggingface",
        2,
        "huggingface",
    )]
}
