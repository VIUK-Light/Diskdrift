//! Human-readable terminal output.

use crate::core::categories;
use crate::core::diff::DiffResult;
use crate::core::doctor::DoctorReport;
use crate::core::explain::{ExplainOutput, display_path};
use crate::core::fs;
use crate::core::history::HistoryDay;
use crate::core::scan::ScanOutput;
use crate::core::size;
use crate::core::snapshot::{CatVal, SnapshotMeta};
use std::io::{self, Write};
use std::path::Path;

fn line<W: Write>(w: &mut W, label: &str, value: &str) -> io::Result<()> {
    writeln!(w, "{:<22}{:>12}", label, value)
}

pub fn render_scan<W: Write>(
    w: &mut W,
    out: &ScanOutput,
    home: &Path,
    verbose: bool,
    show_directories: bool,
) -> io::Result<()> {
    let rolled = fs::rollup_categories(&out.walk.categories);

    writeln!(w, "DiskDrift")?;
    writeln!(w)?;
    writeln!(w, "System-related storage")?;
    writeln!(w, "─────────────────────────────────")?;

    for id in ["developer", "ai", "applications", "system"] {
        let idx = match categories::index_of(id) {
            Some(i) => i,
            None => continue,
        };
        let acc = rolled[idx];
        if acc.allocated == 0 {
            continue;
        }
        line(
            w,
            categories::def_by_index(idx).name,
            &size::format_bytes(acc.allocated),
        )?;
        for child in categories::children_of(idx) {
            let c = rolled[child];
            if c.allocated == 0 {
                continue;
            }
            writeln!(
                w,
                "  {:<20}{:>12}",
                categories::def_by_index(child).name,
                size::format_bytes(c.allocated)
            )?;
        }
    }

    writeln!(w)?;
    line(
        w,
        "Detected total",
        &size::format_bytes(out.walk.totals.allocated),
    )?;
    writeln!(
        w,
        "Scanned {} files, {} directories in {:.1}s ({} threads)",
        size::format_count(out.walk.totals.files),
        size::format_count(out.walk.totals.dirs),
        out.duration.as_secs_f64(),
        out.threads
    )?;

    if out.walk.skipped_count > 0 {
        writeln!(w)?;
        writeln!(
            w,
            "Skipped {} protected or unreadable locations (permission denied or disappeared).",
            size::format_count(out.walk.skipped_count)
        )?;
        writeln!(w, "Run `diskdrift doctor` for details.")?;
        if verbose {
            for s in out.walk.skipped.iter().take(20) {
                writeln!(w, "  {}  ({})", display_path(&s.path, home), s.reason)?;
            }
        }
    }

    if out.walk.totals.hardlinks_deduped > 0 {
        let n = out.walk.totals.hardlinks_deduped;
        let word = if n == 1 { "file was" } else { "files were" };
        writeln!(
            w,
            "{} hard-linked {word} counted once (same inode).",
            size::format_count(n)
        )?;
    }

    if show_directories {
        let rows = directory_rows(out, 10);
        if !rows.is_empty() {
            writeln!(w)?;
            writeln!(w, "Largest directories")?;
            for (i, (path, bytes)) in rows.iter().enumerate() {
                writeln!(
                    w,
                    "  {:>2}. {:>12}  {}",
                    i + 1,
                    size::format_bytes(*bytes),
                    display_path(path, home)
                )?;
            }
        }
    }

    if out.walk.interrupted {
        writeln!(w)?;
        writeln!(w, "Interrupted — results may be incomplete.")?;
    }

    writeln!(w)?;
    writeln!(
        w,
        "Note: DiskDrift values can differ from macOS Storage \"System Data\""
    )?;
    writeln!(
        w,
        "because of APFS clones, APFS snapshots and purgeable space."
    )?;
    writeln!(w, "DiskDrift is read-only and never deletes files.")?;
    Ok(())
}

fn directory_rows(out: &ScanOutput, limit: usize) -> Vec<(&Path, u64)> {
    let mut rows: Vec<(&Path, u64)> = out
        .walk
        .directories
        .iter()
        .filter(|(_, d)| d.acc.allocated > 0)
        .map(|(p, d)| (p.as_path(), d.acc.allocated))
        .collect();
    rows.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    rows.truncate(limit.max(1));
    rows
}

pub fn render_top<W: Write>(
    w: &mut W,
    out: &ScanOutput,
    home: &Path,
    limit: usize,
) -> io::Result<()> {
    writeln!(w, "Largest directories")?;
    writeln!(w)?;
    writeln!(
        w,
        "Scanned {} in {:.1}s",
        size::format_bytes(out.walk.totals.allocated),
        out.duration.as_secs_f64()
    )?;
    writeln!(w)?;
    let rows = directory_rows(out, limit);
    if rows.is_empty() {
        writeln!(w, "No directories with data were found.")?;
        return Ok(());
    }
    for (i, (path, bytes)) in rows.iter().enumerate() {
        writeln!(
            w,
            "{:>4}. {:>12}  {}",
            i + 1,
            size::format_bytes(*bytes),
            display_path(path, home)
        )?;
    }
    Ok(())
}

pub fn render_snapshot_created<W: Write>(w: &mut W, meta: &SnapshotMeta) -> io::Result<()> {
    writeln!(w, "Snapshot created")?;
    writeln!(w)?;
    writeln!(w, "ID: {}", meta.id)?;
    writeln!(w, "Created: {}", display_local(&meta.created_at_local))?;
    writeln!(
        w,
        "Tracked: {} ({} files)",
        size::format_bytes(meta.total_allocated_bytes),
        size::format_count(meta.file_count)
    )?;
    if meta.skipped_count > 0 {
        writeln!(
            w,
            "Skipped: {} protected or unreadable locations",
            size::format_count(meta.skipped_count)
        )?;
    }
    Ok(())
}

pub fn render_diff<W: Write>(
    w: &mut W,
    diff: &DiffResult,
    home: &Path,
    top: usize,
) -> io::Result<()> {
    writeln!(w, "Storage changes")?;
    writeln!(w)?;
    writeln!(
        w,
        "{}",
        diff_range(&diff.old.created_at_local, &diff.new.created_at_local)
    )?;
    writeln!(w)?;

    if diff.display_rows.is_empty() {
        writeln!(w, "No storage changes detected.")?;
    } else {
        for row in diff.display_rows.iter().take(top) {
            let label = match &row.path {
                Some(p) => display_path(p, home),
                None => row.label.clone(),
            };
            let label = truncate(&label, 46);
            writeln!(w, "{:<48}{:>12}", label, size::format_delta(row.net))?;
        }
    }

    writeln!(w)?;
    line(
        w,
        "Total",
        &size::format_delta(diff.totals.net_change_bytes),
    )?;
    if diff.totals.added_bytes > 0 || diff.totals.removed_bytes > 0 {
        writeln!(
            w,
            "({} added, {} removed)",
            size::format_bytes(diff.totals.added_bytes),
            size::format_bytes(diff.totals.removed_bytes)
        )?;
    }
    Ok(())
}

pub fn render_explain<W: Write>(w: &mut W, e: &ExplainOutput) -> io::Result<()> {
    writeln!(w, "{}", e.title)?;
    writeln!(w, "────────────────────────────────")?;
    writeln!(w)?;
    writeln!(w, "Total: {}", size::format_bytes(e.total.allocated))?;
    writeln!(w)?;
    let shown: Vec<_> = e.breakdown.iter().filter(|r| r.val.allocated > 0).collect();
    if shown.is_empty() {
        writeln!(w, "(no data found in this location)")?;
    } else {
        const MAX_ROWS: usize = 20;
        for row in shown.iter().take(MAX_ROWS) {
            let label = truncate(&row.name, 30);
            writeln!(
                w,
                "{:<32}{:>12}",
                label,
                size::format_bytes(row.val.allocated)
            )?;
        }
        if shown.len() > MAX_ROWS {
            writeln!(
                w,
                "{:<32}{:>12}",
                format!("… {} smaller entries", shown.len() - MAX_ROWS),
                size::format_bytes(e.total.allocated)
            )?;
        }
    }
    if let Some(purpose) = &e.purpose {
        writeln!(w)?;
        writeln!(w, "Purpose:")?;
        for l in wrap(purpose, 64) {
            writeln!(w, "{l}")?;
        }
    }
    writeln!(w)?;
    writeln!(w, "DiskDrift does not delete this data.")?;
    Ok(())
}

pub fn render_doctor<W: Write>(w: &mut W, d: &DoctorReport, home: &Path) -> io::Result<()> {
    writeln!(w, "DiskDrift doctor")?;
    writeln!(w)?;

    writeln!(w, "Data directory")?;
    writeln!(w, "  {}", display_path(&d.data_dir, home))?;
    writeln!(
        w,
        "  database: {} ({}, {})",
        display_path(&d.db_path, home),
        if d.db_exists {
            "present"
        } else {
            "not created yet"
        },
        size::format_bytes(d.db_size_bytes)
    )?;
    writeln!(w, "  schema version: {}", d.schema_version)?;
    writeln!(w, "  snapshots: {}", d.snapshot_count)?;
    if !d.db_writable {
        writeln!(w, "  WARNING: database is not writable")?;
    }

    writeln!(w)?;
    writeln!(w, "Configuration")?;
    writeln!(
        w,
        "  {} ({})",
        display_path(&d.config_path, home),
        if let Some(err) = &d.config_error {
            format!("error: {err}")
        } else if d.config_exists {
            "loaded".to_string()
        } else {
            "not present".to_string()
        }
    )?;
    for path in &d.excluded {
        writeln!(w, "  exclude: {}", display_path(path, home))?;
    }

    if let Some(meta) = &d.latest {
        writeln!(w)?;
        writeln!(w, "Latest snapshot")?;
        writeln!(
            w,
            "  #{}  {}  ({} tracked)",
            meta.id,
            display_local(&meta.created_at_local),
            size::format_bytes(meta.total_allocated_bytes)
        )?;
    }

    writeln!(w)?;
    writeln!(w, "Scan locations")?;
    for c in &d.locations {
        writeln!(
            w,
            "  {:<8} {:<12} {}",
            c.status,
            c.label,
            display_path(&c.path, home)
        )?;
    }

    writeln!(w)?;
    writeln!(w, "Protected locations (Full Disk Access)")?;
    for c in &d.protected {
        writeln!(w, "  {:<8} {}", c.status, display_path(&c.path, home))?;
    }

    if !d.latest_skipped.is_empty() {
        writeln!(w)?;
        writeln!(
            w,
            "Skipped in latest snapshot ({})",
            size::format_count(d.latest_skipped.len() as u64)
        )?;
        for (path, reason, _kind) in d.latest_skipped.iter().take(15) {
            writeln!(w, "  {}  ({})", display_path(path, home), reason)?;
        }
    }

    if !d.warnings.is_empty() {
        writeln!(w)?;
        writeln!(w, "Warnings")?;
        for warning in &d.warnings {
            for l in wrap(warning, 72) {
                writeln!(w, "  {l}")?;
            }
        }
    }

    writeln!(w)?;
    writeln!(
        w,
        "DiskDrift never deletes files and never sends data over the network."
    )?;
    Ok(())
}

pub fn render_history<W: Write>(
    w: &mut W,
    days: &[HistoryDay],
    category: Option<&str>,
) -> io::Result<()> {
    match category {
        Some(name) => writeln!(w, "Storage History — {name}")?,
        None => writeln!(w, "Storage History")?,
    }
    writeln!(w)?;
    if days.is_empty() {
        writeln!(w, "No history yet. Run `diskdrift snapshot` regularly.")?;
        return Ok(());
    }
    writeln!(w, "{:<12} {:>12} {:>12}", "Date", "Tracked", "Change")?;
    for day in days {
        let change = day
            .change
            .map(size::format_delta)
            .unwrap_or_else(|| "-".to_string());
        writeln!(
            w,
            "{:<12} {:>12} {:>12}",
            day.date,
            size::format_bytes(day.allocated),
            change
        )?;
    }
    Ok(())
}

pub fn render_snapshot_show<W: Write>(
    w: &mut W,
    meta: &SnapshotMeta,
    rows: &[(usize, CatVal)],
) -> io::Result<()> {
    writeln!(w, "Snapshot #{}", meta.id)?;
    writeln!(w)?;
    writeln!(w, "Created:   {}", display_local(&meta.created_at_local))?;
    writeln!(
        w,
        "Tracked:   {} ({} files, {} directories)",
        size::format_bytes(meta.total_allocated_bytes),
        size::format_count(meta.file_count),
        size::format_count(meta.directory_count)
    )?;
    writeln!(
        w,
        "Skipped:   {} locations",
        size::format_count(meta.skipped_count)
    )?;
    writeln!(w, "Duration:  {:.1}s", meta.duration_ms as f64 / 1000.0)?;
    writeln!(w, "DiskDrift: {}", meta.app_version)?;

    if !rows.is_empty() {
        writeln!(w)?;
        writeln!(w, "Top categories")?;
        for (idx, val) in rows {
            let label = truncate(categories::def_by_index(*idx).name, 30);
            writeln!(w, "{:<32}{:>12}", label, size::format_bytes(val.allocated))?;
        }
    }
    Ok(())
}

pub fn render_snapshots<W: Write>(w: &mut W, list: &[SnapshotMeta], home: &Path) -> io::Result<()> {
    writeln!(w, "Snapshots")?;
    writeln!(w)?;
    if list.is_empty() {
        writeln!(w, "No snapshots yet. Run `diskdrift snapshot`.")?;
        return Ok(());
    }
    writeln!(w, "{:>4}  {:<21}{:>12}", "ID", "Created", "Tracked")?;
    for m in list {
        writeln!(
            w,
            "{:>4}  {:<21}{:>12}",
            m.id,
            display_local(&m.created_at_local),
            size::format_bytes(m.total_allocated_bytes)
        )?;
    }
    let _ = home;
    Ok(())
}

/// "2026-09-13T16:40:21+09:00" -> "2026-09-13 16:40:21"
fn display_local(local: &str) -> String {
    let date = local.get(0..10).unwrap_or(local);
    let time = local.get(11..19).unwrap_or("");
    if time.is_empty() {
        date.to_string()
    } else {
        format!("{date} {time}")
    }
}

/// Diff header: date only when the days differ, otherwise full timestamps.
fn diff_range(old_local: &str, new_local: &str) -> String {
    let old_date = old_local.get(0..10).unwrap_or(old_local);
    let new_date = new_local.get(0..10).unwrap_or(new_local);
    if old_date == new_date {
        format!(
            "{}  →  {}",
            display_local(old_local),
            display_local(new_local)
        )
    } else {
        format!("{old_date}  →  {new_date}")
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current.clear();
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello w…");
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("a b c", 10), vec!["a b c"]);
        assert_eq!(wrap("aaa bbb", 3), vec!["aaa", "bbb"]);
    }
}
