//! Path helpers shared across the core.

use std::path::{Path, PathBuf};

/// Expand `~` and `~/...` against `home`.
pub fn expand_tilde(token: &str, home: &Path) -> PathBuf {
    if token == "~" {
        home.to_path_buf()
    } else if let Some(rest) = token.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(token)
    }
}

/// Expand `~`, and resolve relative paths against `home`.
pub fn expand_user_path(token: &str, home: &Path) -> PathBuf {
    let expanded = expand_tilde(token, home);
    if expanded.is_absolute() {
        expanded
    } else {
        home.join(expanded)
    }
}

/// Render a path with `~` when it is inside the home directory.
pub fn display_path(path: &Path, home: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(home) {
        if rel.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", rel.to_string_lossy())
        }
    } else {
        path.to_string_lossy().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion() {
        let home = PathBuf::from("/Users/test");
        assert_eq!(
            expand_tilde("~/Library", &home),
            PathBuf::from("/Users/test/Library")
        );
        assert_eq!(expand_tilde("~", &home), PathBuf::from("/Users/test"));
        assert_eq!(expand_tilde("/absolute", &home), PathBuf::from("/absolute"));
        assert_eq!(
            expand_user_path("relative", &home),
            PathBuf::from("/Users/test/relative")
        );
    }

    #[test]
    fn display() {
        let home = PathBuf::from("/Users/test");
        assert_eq!(display_path(&home, &home), "~");
        assert_eq!(display_path(&home.join("Library/x"), &home), "~/Library/x");
        assert_eq!(
            display_path(Path::new("/opt/homebrew"), &home),
            "/opt/homebrew"
        );
    }
}
