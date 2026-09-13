//! `diskdrift explain` — focused live analysis of one category or path.

use crate::core::categories;
use crate::core::classify::Classifier;
use crate::core::error::{Error, Result};
use crate::core::fs::ProgressCounters;
use crate::core::scan::{self, ScanConfig, ScanTarget};
use crate::core::snapshot::CatVal;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use crate::core::paths::{display_path, expand_tilde};

pub enum ResolvedQuery {
    Category(usize),
    Path(PathBuf),
}

pub struct ExplainRow {
    pub name: String,
    pub category_id: Option<String>,
    pub path: Option<PathBuf>,
    pub val: CatVal,
}

pub struct ExplainOutput {
    pub query: String,
    pub resolved_kind: &'static str,
    pub resolved_id: Option<String>,
    pub resolved_path: Option<PathBuf>,
    pub title: String,
    pub total: CatVal,
    pub breakdown: Vec<ExplainRow>,
    pub purpose: Option<String>,
}

pub fn resolve_query(token: &str, home: &Path) -> Result<ResolvedQuery> {
    let token = token.trim();
    if token.is_empty() {
        return Err(Error::Message(
            "usage: diskdrift explain <category|path>  (e.g. `diskdrift explain xcode`)".into(),
        ));
    }
    if token.starts_with('~')
        || token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
    {
        return Ok(ResolvedQuery::Path(expand_tilde(token, home)));
    }
    if let Some(idx) = categories::resolve_token(token) {
        return Ok(ResolvedQuery::Category(idx));
    }
    let as_path = expand_tilde(token, home);
    if as_path.exists() {
        return Ok(ResolvedQuery::Path(as_path));
    }
    Err(Error::Message(format!(
        "unknown category or path '{token}'.\nTry `diskdrift explain xcode`, `diskdrift explain ollama`, or a path like `diskdrift explain ~/Library/Developer`"
    )))
}

fn targets_relevant_to_category(idx: usize, home: &Path, all: &[ScanTarget]) -> Vec<ScanTarget> {
    let classifier = Classifier::new(home);
    all.iter()
        .filter(|t| {
            let target_cat = classifier.classify(&t.path);
            if categories::is_descendant_or_self(target_cat, idx) {
                return true;
            }
            if let Some(hint) = categories::index_of(t.category_hint)
                && categories::is_descendant_or_self(hint, idx)
            {
                return true;
            }
            classifier.rules().any(|(prefix, cat)| {
                categories::is_descendant_or_self(cat, idx) && prefix.starts_with(&t.path)
            })
        })
        .cloned()
        .collect()
}

pub fn explain_category(
    idx: usize,
    home: &Path,
    all_targets: &[ScanTarget],
    threads: usize,
    progress: Option<&ProgressCounters>,
    exclusions: &[PathBuf],
) -> ExplainOutput {
    let def = categories::def_by_index(idx);
    let specs = targets_relevant_to_category(idx, home, all_targets);
    let out = scan::run(ScanConfig {
        home,
        targets: specs,
        threads,
        progress,
        exclusions: exclusions.to_vec(),
        depth_override: None,
    });
    let rolled = crate::core::fs::rollup_categories(&out.walk.categories);
    let total = rolled[idx];

    let mut breakdown = Vec::new();
    let mut children_sum = crate::core::fs::Accum::default();
    for child in categories::children_of(idx) {
        let acc = rolled[child];
        if acc.is_zero() {
            continue;
        }
        children_sum.merge(&acc);
        breakdown.push(ExplainRow {
            name: categories::def_by_index(child).name.to_string(),
            category_id: Some(categories::def_by_index(child).id.to_string()),
            path: None,
            val: CatVal {
                logical: acc.logical,
                allocated: acc.allocated,
                files: acc.files,
                dirs: acc.dirs,
            },
        });
    }
    let residual = CatVal {
        logical: total.logical.saturating_sub(children_sum.logical),
        allocated: total.allocated.saturating_sub(children_sum.allocated),
        files: total.files.saturating_sub(children_sum.files),
        dirs: total.dirs.saturating_sub(children_sum.dirs),
    };
    if !residual.is_zero() {
        breakdown.push(ExplainRow {
            name: "Other".to_string(),
            category_id: Some(def.id.to_string()),
            path: None,
            val: residual,
        });
    }

    // Leaf categories (e.g. Ollama) have no child categories, so a breakdown
    // by tracked directories is far more useful than a single "Other" row.
    if children_sum.is_zero() {
        breakdown.clear();
        let mut groups: BTreeMap<String, CatVal> = BTreeMap::new();
        for (dir_path, dir) in &out.walk.directories {
            if !categories::is_descendant_or_self(dir.category, idx) {
                continue;
            }
            let val = CatVal {
                logical: dir.acc.logical,
                allocated: dir.acc.allocated,
                files: dir.acc.files,
                dirs: dir.acc.dirs,
            };
            if val.is_zero() {
                continue;
            }
            groups
                .entry(relative_bucket_name(dir_path, &out.targets, home))
                .or_default()
                .add(&val);
        }
        for (name, val) in groups {
            breakdown.push(ExplainRow {
                name,
                category_id: None,
                path: None,
                val,
            });
        }
    }
    breakdown.retain(|r| !(r.val.allocated == 0 && r.val.logical == 0 && r.val.files == 0));
    breakdown.sort_by_key(|r| std::cmp::Reverse(r.val.allocated));

    ExplainOutput {
        query: def.name.to_string(),
        resolved_kind: "category",
        resolved_id: Some(def.id.to_string()),
        resolved_path: None,
        title: def.name.to_string(),
        total: CatVal {
            logical: total.logical,
            allocated: total.allocated,
            files: total.files,
            dirs: total.dirs,
        },
        breakdown,
        purpose: def.purpose.map(|p| p.to_string()),
    }
}

pub fn explain_path(
    path: &Path,
    home: &Path,
    threads: usize,
    progress: Option<&ProgressCounters>,
    exclusions: &[PathBuf],
) -> Result<ExplainOutput> {
    let display = display_path(path, home);
    let md = std::fs::symlink_metadata(path)
        .map_err(|e| Error::Message(format!("cannot access {display}: {e}")))?;
    if md.file_type().is_symlink() {
        return Err(Error::Message(format!(
            "{display} is a symbolic link; DiskDrift never follows links"
        )));
    }
    if !md.is_dir() {
        return Err(Error::Message(format!("{display} is not a directory")));
    }

    let classifier = Classifier::new(home);
    let target = ScanTarget {
        path: path.to_path_buf(),
        category_hint: "system.other",
        tracked_depth: 1,
        scanner: "explain",
    };
    let out = scan::run(ScanConfig {
        home,
        targets: vec![target],
        threads,
        progress,
        exclusions: exclusions.to_vec(),
        depth_override: None,
    });

    // Group tracked directories by their first component below the query path.
    let mut groups: BTreeMap<String, CatVal> = BTreeMap::new();
    for (dir_path, dir) in &out.walk.directories {
        let key = if dir_path == path {
            String::new() // files directly in this directory
        } else if let Ok(rel) = dir_path.strip_prefix(path) {
            match rel.components().next() {
                Some(c) => c.as_os_str().to_string_lossy().to_string(),
                None => String::new(),
            }
        } else {
            continue;
        };
        let val = CatVal {
            logical: dir.acc.logical,
            allocated: dir.acc.allocated,
            files: dir.acc.files,
            dirs: dir.acc.dirs,
        };
        groups.entry(key).or_default().add(&val);
    }

    let mut breakdown: Vec<ExplainRow> = groups
        .into_iter()
        .map(|(name, val)| ExplainRow {
            name: if name.is_empty() {
                "Other".to_string()
            } else {
                name
            },
            category_id: None,
            path: None,
            val,
        })
        .collect();
    breakdown.sort_by_key(|r| std::cmp::Reverse(r.val.allocated));

    let cat_idx = classifier.classify(path);
    let cat = categories::def_by_index(cat_idx);
    let totals = &out.walk.totals;
    Ok(ExplainOutput {
        query: display.clone(),
        resolved_kind: "path",
        resolved_id: Some(cat.id.to_string()),
        resolved_path: Some(path.to_path_buf()),
        title: display,
        total: CatVal {
            logical: totals.logical,
            allocated: totals.allocated,
            files: totals.files,
            dirs: totals.dirs,
        },
        breakdown,
        purpose: cat.purpose.map(|p| p.to_string()),
    })
}

fn relative_bucket_name(dir: &Path, targets: &[ScanTarget], home: &Path) -> String {
    // Prefer the path relative to the scan target it belongs to, so
    // `explain ollama` shows `models` instead of a long absolute path.
    for t in targets {
        if dir == t.path {
            return "Other".to_string();
        }
        if let Ok(rel) = dir.strip_prefix(&t.path) {
            if let Some(first) = rel.components().next() {
                return first.as_os_str().to_string_lossy().to_string();
            }
            return "Other".to_string();
        }
    }
    display_path(dir, home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion() {
        let home = PathBuf::from("/Users/test");
        assert_eq!(
            expand_tilde("~/Library", &home),
            PathBuf::from("/Users/test/Library")
        );
        assert_eq!(expand_tilde("~", &home), PathBuf::from("/Users/test"));
        assert_eq!(expand_tilde("/absolute", &home), PathBuf::from("/absolute"));
    }

    #[test]
    fn query_resolution() {
        let home = PathBuf::from("/Users/test");
        assert!(matches!(
            resolve_query("xcode", &home),
            Ok(ResolvedQuery::Category(_))
        ));
        assert!(matches!(
            resolve_query("~/Library/Developer", &home),
            Ok(ResolvedQuery::Path(_))
        ));
        assert!(resolve_query("definitely-not-a-category", &home).is_err());
    }
}
