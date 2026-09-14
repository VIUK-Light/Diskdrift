//! Turn watch events into an explanation of "what happened".
//!
//! Events are grouped into incidents: developer/AI categories are reported at
//! their most specific category, while everything else is grouped by
//! category (or by directory when the category is a fallback "Other").

use crate::core::categories;
use crate::core::error::{Error, Result};
use crate::core::paths::display_path;
use crate::core::snapshot::EventRow;
use crate::core::time;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Incident {
    pub key: String,
    pub label: String,
    pub category_id: String,
    pub path: Option<PathBuf>,
    pub delta_bytes: i64,
    pub grow_bytes: u64,
    pub shrink_bytes: u64,
    pub first_unix: i64,
    pub last_unix: i64,
    pub event_count: usize,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub from_unix: i64,
    pub to_unix: i64,
    pub total_delta: i64,
    pub total_grow: u64,
    pub total_shrink: u64,
    pub event_count: usize,
    pub incidents: Vec<Incident>,
}

fn is_fallback_category(idx: usize) -> bool {
    categories::def_by_index(idx).id.ends_with(".other")
}

fn bucket_for(event: &EventRow, home: &Path) -> (String, String, String, Option<PathBuf>) {
    match categories::index_of(&event.category_id) {
        Some(idx) if !is_fallback_category(idx) => (
            event.category_id.clone(),
            categories::display_name(idx),
            event.category_id.clone(),
            None,
        ),
        _ => {
            let label = display_path(&event.path, home);
            (
                format!("path:{}", event.path.display()),
                label,
                event.category_id.clone(),
                Some(event.path.clone()),
            )
        }
    }
}

pub fn build_report(events: &[EventRow], from: i64, to: i64, home: &Path, limit: usize) -> Report {
    let mut incidents: HashMap<String, Incident> = HashMap::new();
    let mut total_delta: i64 = 0;
    let mut total_grow: u64 = 0;
    let mut total_shrink: u64 = 0;

    for event in events {
        total_delta = total_delta.saturating_add(event.delta_bytes);
        if event.delta_bytes > 0 {
            total_grow = total_grow.saturating_add(event.delta_bytes as u64);
        } else if event.delta_bytes < 0 {
            total_shrink = total_shrink.saturating_add(event.delta_bytes.unsigned_abs());
        }

        let (key, label, category_id, path) = bucket_for(event, home);
        let incident = incidents.entry(key.clone()).or_insert_with(|| Incident {
            key,
            label,
            category_id,
            path,
            delta_bytes: 0,
            grow_bytes: 0,
            shrink_bytes: 0,
            first_unix: event.timestamp_unix,
            last_unix: event.timestamp_unix,
            event_count: 0,
        });
        incident.delta_bytes = incident.delta_bytes.saturating_add(event.delta_bytes);
        if event.delta_bytes > 0 {
            incident.grow_bytes = incident.grow_bytes.saturating_add(event.delta_bytes as u64);
        } else if event.delta_bytes < 0 {
            incident.shrink_bytes = incident
                .shrink_bytes
                .saturating_add(event.delta_bytes.unsigned_abs());
        }
        incident.first_unix = incident.first_unix.min(event.timestamp_unix);
        incident.last_unix = incident.last_unix.max(event.timestamp_unix);
        incident.event_count += 1;
    }

    let mut incidents: Vec<Incident> = incidents.into_values().collect();
    incidents.sort_by(|a, b| {
        b.delta_bytes
            .abs()
            .cmp(&a.delta_bytes.abs())
            .then_with(|| a.label.cmp(&b.label))
    });
    incidents.truncate(limit.max(1));

    Report {
        from_unix: from,
        to_unix: to,
        total_delta,
        total_grow,
        total_shrink,
        event_count: events.len(),
        incidents,
    }
}

#[derive(Debug, Clone)]
pub struct Window {
    pub from_unix: i64,
    pub to_unix: i64,
    pub label: String,
}

/// Resolve `--since` / `--from` / `--to` into a time window.
pub fn resolve_window(
    since: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    now: i64,
) -> Result<Window> {
    if since.is_some() && (from.is_some() || to.is_some()) {
        return Err(Error::Message(
            "--since cannot be combined with --from/--to".into(),
        ));
    }
    if let Some(since) = since {
        let seconds = time::parse_duration(since)
            .ok_or_else(|| Error::Message(format!("invalid duration '{since}'")))?;
        return Ok(Window {
            from_unix: now - seconds,
            to_unix: now,
            label: format!("the last {since}"),
        });
    }
    if from.is_none() && to.is_none() {
        return Ok(Window {
            from_unix: now - 86_400,
            to_unix: now,
            label: "the last 24 hours".to_string(),
        });
    }

    let (year, month, day, ..) = time::local_parts(now);
    let parse_time = |value: &str| -> Result<i64> {
        let (hour, minute, second) = time::parse_hhmm(value).ok_or_else(|| {
            Error::Message(format!(
                "expected a time like 14:00 or 14:00:30, got '{value}'"
            ))
        })?;
        time::local_datetime_unix(year, month, day, hour, minute, second)
            .ok_or_else(|| Error::Message(format!("cannot resolve time '{value}'")))
    };

    let from_unix = match from {
        Some(value) => Some(parse_time(value)?),
        None => None,
    };
    let to_unix = match to {
        Some(value) => Some(parse_time(value)?),
        None => None,
    };
    let (from_unix, to_unix) = match (from_unix, to_unix) {
        (Some(from), Some(mut to)) => {
            if to <= from {
                // The window crosses midnight (e.g. 22:00 -> 02:00).
                to += 86_400;
            }
            (from, to)
        }
        (Some(from), None) => (from, now),
        (None, Some(to)) => (to - 86_400, to),
        (None, None) => unreachable!(),
    };

    let short = |value: i64| -> String {
        let local = time::format_local(value);
        let date = local.get(0..10).unwrap_or("");
        let today = time::format_local(now);
        let time_part = local.get(11..16).unwrap_or("--:--");
        if Some(date) == today.get(0..10) {
            time_part.to_string()
        } else {
            format!("{} {time_part}", local.get(5..10).unwrap_or(date))
        }
    };
    Ok(Window {
        from_unix,
        to_unix,
        label: format!("between {} and {}", short(from_unix), short(to_unix)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: i64, ts: i64, category: &str, path: &str, delta: i64) -> EventRow {
        EventRow {
            id,
            timestamp_unix: ts,
            timestamp_local: time::format_local(ts),
            kind: if delta >= 0 { "grow" } else { "shrink" }.to_string(),
            path: PathBuf::from(path),
            category_id: category.to_string(),
            delta_bytes: delta,
            allocated_bytes: 0,
            file_count: 0,
            directory_count: 0,
        }
    }

    #[test]
    fn groups_categories_and_paths() {
        let home = Path::new("/Users/test");
        let events = vec![
            event(
                1,
                100,
                "developer.xcode.core_simulator",
                "/Users/test/Library/Developer/CoreSimulator",
                100,
            ),
            event(
                2,
                200,
                "developer.xcode.core_simulator",
                "/Users/test/Library/Developer/CoreSimulator",
                300,
            ),
            event(3, 150, "system.caches", "/Users/test/Library/Caches/a", 50),
            event(4, 160, "system.caches", "/Users/test/.cache/b", 25),
            event(5, 170, "system.other", "/Users/test/somewhere/unknown", 10),
        ];
        let report = build_report(&events, 0, 1000, home, 10);
        assert_eq!(report.event_count, 5);
        assert_eq!(report.total_delta, 485);
        assert_eq!(report.total_grow, 485);
        assert_eq!(report.total_shrink, 0);

        let sim = report
            .incidents
            .iter()
            .find(|i| i.category_id == "developer.xcode.core_simulator")
            .expect("simulator incident");
        assert_eq!(sim.label, "Xcode / CoreSimulator");
        assert_eq!(sim.delta_bytes, 400);
        assert_eq!(sim.first_unix, 100);
        assert_eq!(sim.last_unix, 200);
        assert_eq!(sim.event_count, 2);

        let caches = report
            .incidents
            .iter()
            .find(|i| i.category_id == "system.caches")
            .expect("caches incident");
        assert_eq!(caches.delta_bytes, 75);
        assert_eq!(caches.event_count, 2);

        let other = report
            .incidents
            .iter()
            .find(|i| i.category_id == "system.other")
            .expect("fallback incident");
        assert_eq!(other.label, "~/somewhere/unknown");
        assert!(other.path.is_some());
    }

    #[test]
    fn negative_changes_are_separated() {
        let home = Path::new("/Users/test");
        let events = vec![
            event(1, 100, "ai.ollama", "/Users/test/.ollama", 500),
            event(2, 200, "ai.ollama", "/Users/test/.ollama", -200),
        ];
        let report = build_report(&events, 0, 1000, home, 10);
        assert_eq!(report.total_delta, 300);
        assert_eq!(report.total_grow, 500);
        assert_eq!(report.total_shrink, 200);
        let ollama = &report.incidents[0];
        assert_eq!(ollama.delta_bytes, 300);
        assert_eq!(ollama.grow_bytes, 500);
        assert_eq!(ollama.shrink_bytes, 200);
    }

    #[test]
    fn report_respects_limit() {
        let home = Path::new("/Users/test");
        let events = vec![
            event(1, 100, "ai.ollama", "/Users/test/.ollama", 500),
            event(2, 100, "system.caches", "/Users/test/Library/Caches/a", 300),
            event(3, 100, "developer.homebrew", "/opt/homebrew", 100),
        ];
        let report = build_report(&events, 0, 1000, home, 2);
        assert_eq!(report.incidents.len(), 2);
        assert_eq!(report.incidents[0].category_id, "ai.ollama");
        assert_eq!(report.total_delta, 900);
    }

    #[test]
    fn window_defaults_to_24_hours() {
        let now = 1_000_000;
        let window = resolve_window(None, None, None, now).unwrap();
        assert_eq!(window.from_unix, now - 86_400);
        assert_eq!(window.to_unix, now);
        assert_eq!(window.label, "the last 24 hours");

        let window = resolve_window(Some("1h"), None, None, now).unwrap();
        assert_eq!(window.from_unix, now - 3_600);
        assert_eq!(window.label, "the last 1h");
    }

    #[test]
    fn window_from_to_and_conflicts() {
        let now = 1_789_285_221;
        let (y, mo, d, h, mi, _s) = time::local_parts(now);
        let from = format!("{h:02}:{mi:02}");
        let to_h = (h + 2) % 24;
        let to = format!("{to_h:02}:{mi:02}");
        let window = resolve_window(None, Some(&from), Some(&to), now).unwrap();
        let expected_from = time::local_datetime_unix(y, mo, d, h, mi, 0).unwrap();
        assert_eq!(window.from_unix, expected_from);
        assert!(window.to_unix > window.from_unix);

        assert!(resolve_window(Some("1h"), Some("14:00"), None, now).is_err());
        assert!(resolve_window(None, Some("nope"), None, now).is_err());
    }
}
