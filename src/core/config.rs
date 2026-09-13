//! Configuration file support.
//!
//! Default location: `~/.config/diskdrift/config.toml` (or
//! `$XDG_CONFIG_HOME/diskdrift/config.toml`). `--config <path>` and the
//! `DISKDRIFT_CONFIG` environment variable override it.
//!
//! ```toml
//! exclude = ["~/Downloads", "/Volumes/External"]
//! threads = 4
//! depth = 2
//! ```
//!
//! `exclude` paths are never entered by the scanner (in addition to
//! DiskDrift's own database directory). `threads` and `depth` are defaults;
//! command-line flags always win.

use crate::core::error::{Error, Result};
use crate::core::paths::expand_user_path;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const CONFIG_ENV: &str = "DISKDRIFT_CONFIG";
pub const MAX_DEPTH: usize = 8;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    exclude: Vec<String>,
    threads: Option<usize>,
    depth: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub path: PathBuf,
    pub exists: bool,
    pub exclude: Vec<PathBuf>,
    pub threads: Option<usize>,
    pub depth: Option<usize>,
}

impl Config {
    pub fn disabled() -> Self {
        Config {
            path: PathBuf::new(),
            exists: false,
            exclude: Vec::new(),
            threads: None,
            depth: None,
        }
    }

    pub fn default_path(home: &Path) -> PathBuf {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
            if !dir.is_empty() {
                return PathBuf::from(dir).join("diskdrift/config.toml");
            }
        }
        home.join(".config/diskdrift/config.toml")
    }

    /// Load the configuration. A missing default file is fine; a missing file
    /// that was explicitly requested (`--config` or env var) is an error.
    pub fn load(home: &Path, explicit: Option<&Path>) -> Result<Config> {
        let (path, required) = match explicit {
            Some(p) => (p.to_path_buf(), true),
            None => match std::env::var_os(CONFIG_ENV) {
                Some(v) if !v.is_empty() => (PathBuf::from(v), true),
                _ => (Self::default_path(home), false),
            },
        };
        if !path.exists() {
            if required {
                return Err(Error::Message(format!(
                    "config file not found: {}",
                    path.display()
                )));
            }
            return Ok(Config {
                path,
                ..Config::disabled()
            });
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::Message(format!("cannot read config {}: {e}", path.display())))?;
        Self::parse(&text, &path, home)
    }

    pub fn parse(text: &str, path: &Path, home: &Path) -> Result<Config> {
        let file: ConfigFile = toml::from_str(text)
            .map_err(|e| Error::Message(format!("invalid config {}: {e}", path.display())))?;
        if let Some(threads) = file.threads {
            if threads == 0 {
                return Err(Error::Message(format!(
                    "invalid config {}: threads must be at least 1",
                    path.display()
                )));
            }
        }
        if let Some(depth) = file.depth {
            if depth == 0 || depth > MAX_DEPTH {
                return Err(Error::Message(format!(
                    "invalid config {}: depth must be between 1 and {MAX_DEPTH}",
                    path.display()
                )));
            }
        }
        let exclude = file
            .exclude
            .iter()
            .map(|p| expand_user_path(p, home))
            .collect();
        Ok(Config {
            path: path.to_path_buf(),
            exists: true,
            exclude,
            threads: file.threads,
            depth: file.depth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config> {
        Config::parse(
            text,
            Path::new("/tmp/config.toml"),
            Path::new("/Users/test"),
        )
    }

    #[test]
    fn parses_exclusions_and_defaults() {
        let config = parse(
            r#"
exclude = ["~/Downloads", "relative/dir", "/Volumes/External"]
threads = 4
depth = 3
"#,
        )
        .unwrap();
        assert_eq!(
            config.exclude,
            vec![
                PathBuf::from("/Users/test/Downloads"),
                PathBuf::from("/Users/test/relative/dir"),
                PathBuf::from("/Volumes/External"),
            ]
        );
        assert_eq!(config.threads, Some(4));
        assert_eq!(config.depth, Some(3));
    }

    #[test]
    fn empty_config_is_valid() {
        let config = parse("").unwrap();
        assert!(config.exclude.is_empty());
        assert_eq!(config.threads, None);
        assert_eq!(config.depth, None);
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = parse("threds = 4").unwrap_err().to_string();
        assert!(err.contains("invalid config"), "{err}");
    }

    #[test]
    fn rejects_bad_values() {
        assert!(parse("threads = 0").is_err());
        assert!(parse("depth = 0").is_err());
        assert!(parse("depth = 99").is_err());
    }
}
