//! Snapshot diffing at category and directory level.

use crate::core::categories::{self, CATEGORIES};
use crate::core::error::Result;
use crate::core::snapshot::{CatVal, SnapshotMeta};
use crate::core::store::Store;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct SideData {
    pub meta: SnapshotMeta,
    /// Leaf-level per-category totals, keyed by category id.
    pub categories: HashMap<String, CatVal>,
    /// Tracked directory totals.
    pub directories: HashMap<PathBuf, DirSide>,
}

pub struct DirSide {
    pub category_id: String,
    pub val: CatVal,
}

pub fn load_side(store: &Store, snapshot_id: i64) -> Result<SideData> {
    let meta = store.snapshot_by_id(snapshot_id)?.ok_or_else(|| {
        crate::core::error::Error::Message(format!("snapshot {snapshot_id} not found"))
    })?;
    let categories = store
        .load_categories(snapshot_id)?
        .into_iter()
        .map(|r| (r.category_id, r.val))
        .collect();
    let directories = store
        .load_directories(snapshot_id)?
        .into_iter()
        .map(|r| {
            (
                r.path,
                DirSide {
                    category_id: r.category_id,
                    val: r.val,
                },
            )
        })
        .collect();
    Ok(SideData {
        meta,
        categories,
        directories,
    })
}

/// Roll leaf totals up so every category contains its whole subtree.
pub fn rollup(leaf: &HashMap<String, CatVal>) -> HashMap<String, CatVal> {
    let mut out = HashMap::new();
    for (idx, def) in CATEGORIES.iter().enumerate() {
        let mut val = CatVal::default();
        for (id, v) in leaf {
            if let Some(j) = categories::index_of(id)
                && categories::is_descendant_or_self(j, idx)
            {
                val.add(v);
            }
        }
        if !val.is_zero() {
            out.insert(def.id.to_string(), val);
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct CatDelta {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub old: CatVal,
    pub new: CatVal,
    pub added: u64,
    pub removed: u64,
    pub net: i64,
}

#[derive(Debug, Clone)]
pub struct DirDelta {
    pub path: PathBuf,
    pub category_id: String,
    pub old: CatVal,
    pub new: CatVal,
    pub added: u64,
    pub removed: u64,
    pub net: i64,
}

#[derive(Debug, Clone)]
pub struct DiffTotals {
    pub old_logical_bytes: u64,
    pub new_logical_bytes: u64,
    pub old_allocated_bytes: u64,
    pub new_allocated_bytes: u64,
    pub added_bytes: u64,
    pub removed_bytes: u64,
    pub net_change_bytes: i64,
}

/// One line for terminal output.
#[derive(Debug, Clone)]
pub struct DisplayRow {
    pub label: String,
    pub path: Option<PathBuf>,
    pub net: i64,
}

pub struct DiffResult {
    pub old: SnapshotMeta,
    pub new: SnapshotMeta,
    pub totals: DiffTotals,
    pub categories: Vec<CatDelta>,
    pub directories: Vec<DirDelta>,
    pub display_rows: Vec<DisplayRow>,
}

pub fn make_delta(old: &CatVal, new: &CatVal) -> (i64, u64, u64) {
    let net = new.allocated as i64 - old.allocated as i64;
    let added = if net > 0 { net as u64 } else { 0 };
    let removed = if net < 0 { net.unsigned_abs() } else { 0 };
    (net, added, removed)
}

pub fn compute(old: SideData, new: SideData, top: usize) -> DiffResult {
    let old_rolled = rollup(&old.categories);
    let new_rolled = rollup(&new.categories);

    // --- category deltas (rolled up) ---
    let mut cat_ids: HashSet<&String> = HashSet::new();
    cat_ids.extend(old_rolled.keys());
    cat_ids.extend(new_rolled.keys());
    let zero = CatVal::default();
    let mut categories: Vec<CatDelta> = cat_ids
        .into_iter()
        .filter_map(|id| {
            let idx = categories::index_of(id)?;
            let def = categories::def_by_index(idx);
            let o = old_rolled.get(id).unwrap_or(&zero);
            let n = new_rolled.get(id).unwrap_or(&zero);
            let (net, added, removed) = make_delta(o, n);
            if net == 0 && added == 0 && removed == 0 {
                return None;
            }
            Some(CatDelta {
                id: id.clone(),
                name: def.name.to_string(),
                parent_id: def.parent.map(|p| p.to_string()),
                old: *o,
                new: *n,
                added,
                removed,
                net,
            })
        })
        .collect();
    categories.sort_by(|a, b| b.net.abs().cmp(&a.net.abs()).then_with(|| a.id.cmp(&b.id)));

    // --- directory deltas ---
    let mut paths: HashSet<&PathBuf> = HashSet::new();
    paths.extend(old.directories.keys());
    paths.extend(new.directories.keys());
    let mut directories: Vec<DirDelta> = paths
        .into_iter()
        .filter_map(|path| {
            let o_side = old.directories.get(path);
            let n_side = new.directories.get(path);
            let o = o_side.map(|s| s.val).unwrap_or_default();
            let n = n_side.map(|s| s.val).unwrap_or_default();
            let (net, added, removed) = make_delta(&o, &n);
            if net == 0 {
                return None;
            }
            let category_id = n_side
                .map(|s| s.category_id.clone())
                .or_else(|| o_side.map(|s| s.category_id.clone()))
                .unwrap_or_else(|| "system.other".to_string());
            Some(DirDelta {
                path: path.clone(),
                category_id,
                old: o,
                new: n,
                added,
                removed,
                net,
            })
        })
        .collect();
    directories.sort_by(|a, b| {
        b.net
            .abs()
            .cmp(&a.net.abs())
            .then_with(|| a.path.cmp(&b.path))
    });

    // --- totals: directory buckets partition all scanned bytes ---
    let added_bytes: u64 = directories.iter().map(|d| d.added).sum();
    let removed_bytes: u64 = directories.iter().map(|d| d.removed).sum();
    let totals = DiffTotals {
        old_logical_bytes: old.meta.total_logical_bytes,
        new_logical_bytes: new.meta.total_logical_bytes,
        old_allocated_bytes: old.meta.total_allocated_bytes,
        new_allocated_bytes: new.meta.total_allocated_bytes,
        added_bytes,
        removed_bytes,
        net_change_bytes: new.meta.total_allocated_bytes as i64
            - old.meta.total_allocated_bytes as i64,
    };

    let display_rows = build_display_rows(&old_rolled, &new_rolled, &directories, top);

    DiffResult {
        old: old.meta,
        new: new.meta,
        totals,
        categories,
        directories,
        display_rows,
    }
}

/// Terminal display strategy:
/// - Developer / AI changes are shown as categories (rolled to the most
///   specific category), so `CoreSimulator` is not repeated as a directory.
/// - Everything else is shown as tracked directories.
fn build_display_rows(
    old_rolled: &HashMap<String, CatVal>,
    new_rolled: &HashMap<String, CatVal>,
    directories: &[DirDelta],
    top: usize,
) -> Vec<DisplayRow> {
    let zero = CatVal::default();
    let dev = categories::index_of("developer").unwrap();
    let ai = categories::index_of("ai").unwrap();
    let mut rows: Vec<DisplayRow> = Vec::new();

    for (idx, def) in CATEGORIES.iter().enumerate() {
        if !(categories::is_descendant_or_self(idx, dev)
            || categories::is_descendant_or_self(idx, ai))
        {
            continue;
        }
        let old_parent = old_rolled.get(def.id).copied().unwrap_or(zero);
        let new_parent = new_rolled.get(def.id).copied().unwrap_or(zero);
        let mut old_children = CatVal::default();
        let mut new_children = CatVal::default();
        for child in categories::children_of(idx) {
            let cid = CATEGORIES[child].id;
            old_children.add(old_rolled.get(cid).unwrap_or(&zero));
            new_children.add(new_rolled.get(cid).unwrap_or(&zero));
        }
        let old_own = CatVal {
            allocated: old_parent.allocated.saturating_sub(old_children.allocated),
            logical: old_parent.logical.saturating_sub(old_children.logical),
            files: old_parent.files.saturating_sub(old_children.files),
            dirs: old_parent.dirs.saturating_sub(old_children.dirs),
        };
        let new_own = CatVal {
            allocated: new_parent.allocated.saturating_sub(new_children.allocated),
            logical: new_parent.logical.saturating_sub(new_children.logical),
            files: new_parent.files.saturating_sub(new_children.files),
            dirs: new_parent.dirs.saturating_sub(new_children.dirs),
        };
        let net = new_own.allocated as i64 - old_own.allocated as i64;
        if net != 0 {
            rows.push(DisplayRow {
                label: categories::display_name(idx),
                path: None,
                net,
            });
        }
    }

    for d in directories {
        let cat_idx = categories::index_of(&d.category_id);
        let under_dev_or_ai = cat_idx
            .map(|i| {
                categories::is_descendant_or_self(i, dev)
                    || categories::is_descendant_or_self(i, ai)
            })
            .unwrap_or(false);
        if under_dev_or_ai {
            continue;
        }
        rows.push(DisplayRow {
            label: String::new(),
            path: Some(d.path.clone()),
            net: d.net,
        });
    }

    rows.sort_by_key(|r| std::cmp::Reverse(r.net.abs()));
    rows.truncate(top.max(1));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollup_sums_subtrees() {
        let mut leaf = HashMap::new();
        leaf.insert(
            "developer.xcode.core_simulator".to_string(),
            CatVal {
                allocated: 100,
                ..Default::default()
            },
        );
        leaf.insert(
            "developer.homebrew".to_string(),
            CatVal {
                allocated: 30,
                ..Default::default()
            },
        );
        let rolled = rollup(&leaf);
        assert_eq!(rolled["developer.xcode"].allocated, 100);
        assert_eq!(rolled["developer"].allocated, 130);
    }
}
