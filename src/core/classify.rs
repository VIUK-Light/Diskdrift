//! Path -> category classification.
//!
//! Rules are plain path prefixes. The category tree lives in `categories.rs`,
//! so adding a new tool means adding a rule and (optionally) a category, not
//! changing the scanner.

use crate::core::categories;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct CompiledRule {
    prefix: PathBuf,
    category: usize,
}

pub struct Classifier {
    rules: Vec<CompiledRule>,
    default_category: usize,
}

impl Classifier {
    pub fn new(home: &Path) -> Self {
        Self::with_extra(home, &[])
    }

    /// Build the default rule set, expanded against `home`, plus optional
    /// caller-provided rules (used by tests and future config files).
    pub fn with_extra(home: &Path, extra: &[(PathBuf, &'static str)]) -> Self {
        let mut raw: Vec<(PathBuf, &'static str)> = Vec::new();
        let h = home.to_path_buf();
        let push = |rel: &str, cat: &'static str, raw: &mut Vec<(PathBuf, &'static str)>| {
            raw.push((
                if rel.starts_with('/') {
                    PathBuf::from(rel)
                } else {
                    h.join(rel)
                },
                cat,
            ));
        };

        // Most specific rules first; ordering is enforced after collection.
        push(".cache/huggingface", "ai.huggingface", &mut raw);
        push(".cache/lm-studio", "ai.lmstudio", &mut raw);
        push(".lmstudio", "ai.lmstudio", &mut raw);
        push(
            "Library/Application Support/LM Studio",
            "ai.lmstudio",
            &mut raw,
        );
        push(".ollama", "ai.ollama", &mut raw);

        push(
            "Library/Developer/Xcode/DerivedData",
            "developer.xcode.derived_data",
            &mut raw,
        );
        push(
            "Library/Developer/Xcode/Archives",
            "developer.xcode.archives",
            &mut raw,
        );
        push(
            "Library/Developer/Xcode/iOS DeviceSupport",
            "developer.xcode.device_support",
            &mut raw,
        );
        push(
            "Library/Developer/Xcode/watchOS DeviceSupport",
            "developer.xcode.device_support",
            &mut raw,
        );
        push(
            "Library/Developer/Xcode/tvOS DeviceSupport",
            "developer.xcode.device_support",
            &mut raw,
        );
        push(
            "Library/Developer/Xcode/visionOS DeviceSupport",
            "developer.xcode.device_support",
            &mut raw,
        );
        push(
            "Library/Developer/Xcode/macOS DeviceSupport",
            "developer.xcode.device_support",
            &mut raw,
        );
        push(
            "Library/Developer/CoreSimulator",
            "developer.xcode.core_simulator",
            &mut raw,
        );
        push("Library/Developer/Xcode", "developer.xcode", &mut raw);
        push("Library/Developer", "developer.other", &mut raw);

        push("Library/Caches/Homebrew", "developer.homebrew", &mut raw);
        push("/opt/homebrew", "developer.homebrew", &mut raw);
        push("/usr/local/Cellar", "developer.homebrew", &mut raw);
        push("/usr/local/Caskroom", "developer.homebrew", &mut raw);
        push("/usr/local/Homebrew", "developer.homebrew", &mut raw);

        push(
            "Library/Containers/com.docker.docker",
            "developer.docker",
            &mut raw,
        );
        push(
            "Library/Group Containers/group.com.docker",
            "developer.docker",
            &mut raw,
        );
        push(".docker", "developer.docker", &mut raw);

        push(".npm", "developer.npm", &mut raw);
        push("Library/pnpm", "developer.pnpm", &mut raw);
        push(".pnpm-store", "developer.pnpm", &mut raw);
        push(".local/share/pnpm", "developer.pnpm", &mut raw);

        push(".cache", "system.caches", &mut raw);

        push("Library/Caches", "system.caches", &mut raw);
        push("/Library/Caches", "system.caches", &mut raw);
        push("Library/Logs", "system.logs", &mut raw);
        push("/Library/Logs", "system.logs", &mut raw);
        push("/private/var/log", "system.logs", &mut raw);
        push(
            "Library/Application Support",
            "applications.app_support",
            &mut raw,
        );
        push(
            "/Library/Application Support",
            "applications.app_support",
            &mut raw,
        );
        push("Library/Containers", "applications.containers", &mut raw);
        push(
            "Library/Group Containers",
            "applications.group_containers",
            &mut raw,
        );
        push("/private/tmp", "system.temporary", &mut raw);
        push("/private/var/tmp", "system.temporary", &mut raw);

        for (p, c) in extra {
            raw.push((p.clone(), c));
        }

        let mut rules: Vec<CompiledRule> = raw
            .into_iter()
            .filter_map(|(p, cat)| {
                categories::index_of(cat).map(|idx| CompiledRule {
                    prefix: normalize(&p),
                    category: idx,
                })
            })
            .collect();

        // Longest (deepest) prefixes win.
        rules.sort_by(|a, b| {
            let alen = a.prefix.components().count();
            let blen = b.prefix.components().count();
            blen.cmp(&alen)
                .then_with(|| b.prefix.as_os_str().len().cmp(&a.prefix.as_os_str().len()))
        });
        rules.dedup_by(|a, b| a.prefix == b.prefix);

        Classifier {
            rules,
            default_category: categories::index_of("system.other").unwrap(),
        }
    }

    pub fn classify(&self, path: &Path) -> usize {
        let p = normalize(path);
        for rule in &self.rules {
            if p == rule.prefix || p.starts_with(&rule.prefix) {
                return rule.category;
            }
        }
        self.default_category
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Iterate over (prefix, category index) rules. Used by `explain`.
    pub fn rules(&self) -> impl Iterator<Item = (&Path, usize)> {
        self.rules.iter().map(|r| (r.prefix.as_path(), r.category))
    }
}

/// Remove a trailing slash (but keep `/`) and redundant components, so
/// `Path::starts_with` behaves predictably.
fn normalize(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let trimmed = if s.len() > 1 {
        s.trim_end_matches('/')
    } else {
        &s
    };
    PathBuf::from(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn c() -> Classifier {
        Classifier::new(&PathBuf::from("/Users/test"))
    }

    fn cat(path: &str) -> &'static str {
        let idx = c().classify(&PathBuf::from(path));
        categories::def_by_index(idx).id
    }

    #[test]
    fn xcode_paths() {
        assert_eq!(
            cat("/Users/test/Library/Developer/Xcode/DerivedData"),
            "developer.xcode.derived_data"
        );
        assert_eq!(
            cat("/Users/test/Library/Developer/Xcode/DerivedData/Foo/Bar"),
            "developer.xcode.derived_data"
        );
        assert_eq!(
            cat("/Users/test/Library/Developer/CoreSimulator/Devices"),
            "developer.xcode.core_simulator"
        );
        assert_eq!(
            cat("/Users/test/Library/Developer/Xcode/Archives/2026"),
            "developer.xcode.archives"
        );
        assert_eq!(
            cat("/Users/test/Library/Developer/Xcode/iOS DeviceSupport/18.0"),
            "developer.xcode.device_support"
        );
        assert_eq!(
            cat("/Users/test/Library/Developer/Xcode/UserData"),
            "developer.xcode"
        );
        assert_eq!(
            cat("/Users/test/Library/Developer/SomethingElse"),
            "developer.other"
        );
    }

    #[test]
    fn tool_paths() {
        assert_eq!(cat("/Users/test/.ollama/models"), "ai.ollama");
        assert_eq!(cat("/Users/test/.cache/huggingface/hub"), "ai.huggingface");
        assert_eq!(cat("/Users/test/.cache/lm-studio/models"), "ai.lmstudio");
        assert_eq!(cat("/Users/test/.cache/pip"), "system.caches");
        assert_eq!(cat("/opt/homebrew/Cellar/foo/1.0"), "developer.homebrew");
        assert_eq!(
            cat("/Users/test/Library/Caches/Homebrew/downloads"),
            "developer.homebrew"
        );
        assert_eq!(
            cat("/Users/test/Library/Containers/com.docker.docker/Data"),
            "developer.docker"
        );
        assert_eq!(
            cat("/Users/test/Library/Containers/com.example.app"),
            "applications.containers"
        );
        assert_eq!(cat("/Users/test/.npm/_cacache"), "developer.npm");
        assert_eq!(cat("/Users/test/Library/pnpm/store"), "developer.pnpm");
    }

    #[test]
    fn generic_paths() {
        assert_eq!(cat("/Library/Caches/com.apple.foo"), "system.caches");
        assert_eq!(
            cat("/Users/test/Library/Application Support/Google/Chrome"),
            "applications.app_support"
        );
        assert_eq!(cat("/Users/test/Library/Logs/Foo"), "system.logs");
        assert_eq!(cat("/Users/test/Documents/report.pdf"), "system.other");
        assert_eq!(cat("/"), "system.other");
    }

    #[test]
    fn specific_rule_wins() {
        // LM Studio under Application Support must beat the generic rule.
        assert_eq!(
            cat("/Users/test/Library/Application Support/LM Studio/models"),
            "ai.lmstudio"
        );
    }
}
