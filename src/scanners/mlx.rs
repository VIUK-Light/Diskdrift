use crate::core::scan::ScanTarget;
use std::path::Path;

pub fn targets(home: &Path) -> Vec<ScanTarget> {
    // MLX models downloaded through Hugging Face live in the Hugging Face
    // cache; this target covers repositories that keep a separate directory.
    vec![super::target(home.join("mlx_models"), "ai.mlx", 1, "mlx")]
}
