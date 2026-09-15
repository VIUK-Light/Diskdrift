//! Time Machine / APFS local snapshots (best effort, read-only).
//!
//! `tmutil listlocalsnapshots /` works without root and only reports
//! metadata. Snapshot blocks are shared with the volume, so sizes are not
//! reported here — they are macOS-managed.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct LocalSnapshot {
    pub name: String,
    /// "YYYY-MM-DD HH:MM:SS" parsed from the snapshot name, when possible.
    pub date: Option<String>,
}

/// Parse `tmutil listlocalsnapshots` output.
pub fn parse_tmutil_output(text: &str) -> Vec<LocalSnapshot> {
    let mut out = Vec::new();
    for line in text.lines() {
        let name = line.trim();
        if !name.starts_with("com.apple.TimeMachine.") {
            continue;
        }
        let date = parse_date(name);
        out.push(LocalSnapshot {
            name: name.to_string(),
            date,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn parse_date(name: &str) -> Option<String> {
    // com.apple.TimeMachine.2026-09-14-102233.local
    let rest = name.strip_prefix("com.apple.TimeMachine.")?;
    let stamp = rest.strip_suffix(".local")?;
    let parts: Vec<&str> = stamp.split('-').collect();
    if parts.len() != 4 {
        return None;
    }
    let (y, mo, d) = (parts[0], parts[1], parts[2]);
    let time = parts[3];
    if time.len() != 6 {
        return None;
    }
    Some(format!(
        "{y}-{mo}-{d} {}:{}:{}",
        &time[0..2],
        &time[2..4],
        &time[4..6]
    ))
}

/// List local snapshots; an empty list means "none or tmutil unavailable".
pub fn list() -> Vec<LocalSnapshot> {
    let output = std::process::Command::new("/usr/bin/tmutil")
        .args(["listlocalsnapshots", "/"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            parse_tmutil_output(&String::from_utf8_lossy(&output.stdout))
        }
        _ => Vec::new(),
    }
}

/// Convenience for the GUI/CLI: latest snapshot date, if any.
pub fn latest(snapshots: &[LocalSnapshot]) -> Option<&str> {
    snapshots.iter().rev().find_map(|s| s.date.as_deref())
}

#[allow(dead_code)]
pub fn list_for_volume(path: &Path) -> Vec<LocalSnapshot> {
    let _ = path;
    list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tmutil_output() {
        let text = "\
Snapshots for volume group containing disk /:
com.apple.TimeMachine.2026-09-13-221001.local
com.apple.TimeMachine.2026-09-14-102233.local
";
        let snapshots = parse_tmutil_output(text);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].date.as_deref(), Some("2026-09-13 22:10:01"));
        assert_eq!(snapshots[1].date.as_deref(), Some("2026-09-14 10:22:33"));
        assert_eq!(latest(&snapshots), Some("2026-09-14 10:22:33"));
    }

    #[test]
    fn ignores_other_lines() {
        let text = "Snapshots for volume group:\nNo snapshots found.\n";
        assert!(parse_tmutil_output(text).is_empty());
    }
}
