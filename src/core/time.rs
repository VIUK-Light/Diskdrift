//! Timestamp formatting without external crates.
//!
//! Snapshot IDs are integers; timestamps are stored in UTC (RFC 3339) for
//! ordering and rendered in local time for humans.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_unix() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    }
}

/// Howard Hinnant's civil_from_days algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn split_secs(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    (
        y,
        mo,
        d,
        (rem / 3_600) as u32,
        ((rem % 3_600) / 60) as u32,
        (rem % 60) as u32,
    )
}

/// `2026-09-13T07:40:21Z`
pub fn format_utc(secs: i64) -> String {
    let (y, mo, d, h, mi, s) = split_secs(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Local timezone offset in seconds for the given instant.
#[cfg(target_os = "macos")]
fn local_offset(secs: i64) -> i64 {
    unsafe {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff
    }
}

#[cfg(not(target_os = "macos"))]
fn local_offset(_secs: i64) -> i64 {
    0
}

/// `2026-09-13T16:40:21+09:00` (local time with offset)
pub fn format_local(secs: i64) -> String {
    let off = local_offset(secs);
    let (ly, lmo, ld, lh, lmi, ls) = split_secs(secs + off);
    let sign = if off >= 0 { '+' } else { '-' };
    let a = off.abs();
    format!(
        "{ly:04}-{lmo:02}-{ld:02}T{lh:02}:{lmi:02}:{ls:02}{sign}{:02}:{:02}",
        a / 3_600,
        (a % 3_600) / 60
    )
}

/// `2026-09-13 16:40:21` (local, for terminal output)
pub fn format_local_display(secs: i64) -> String {
    let s = format_local(secs);
    let s = s.replace('T', " ");
    match s.char_indices().nth(19) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s,
    }
}

/// `2026-09-13` (local date)
pub fn local_date(secs: i64) -> String {
    format_local_display(secs)[..10].to_string()
}

/// Parse a small subset of RFC-3339 / local timestamps for snapshot lookup.
/// Returns unix seconds if the input looks like `YYYY-MM-DD[THH:MM[:SS]]`.
pub fn parse_datetime_prefix(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = match s.split_once(['T', ' ']) {
        Some((d, t)) => (d, Some(t.trim_end_matches('Z'))),
        None => (s, None),
    };
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let mo: u32 = dp.next()?.parse().ok()?;
    let d: u32 = dp.next()?.parse().ok()?;
    if dp.next().is_some() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let (mut h, mut mi, mut sec) = (0u32, 0u32, 0u32);
    if let Some(t) = time {
        let mut tp = t.split(':');
        h = tp.next()?.parse().ok()?;
        if let Some(m) = tp.next() {
            mi = m.parse().ok()?;
        }
        if let Some(secs) = tp.next() {
            sec = secs.parse().ok()?;
        }
        if tp.next().is_some() {
            return None;
        }
    }
    days_from_civil(y, mo, d)
        .map(|days| days * 86_400 + (h as i64) * 3_600 + (mi as i64) * 60 + sec as i64)
}

/// Parse a duration such as `30s`, `45m`, `24h`, `7d` or `2w`.
pub fn parse_duration(input: &str) -> Option<i64> {
    let s = input.trim();
    if s.len() < 2 {
        return None;
    }
    let (num, unit) = s.split_at(s.len() - 1);
    let n: i64 = num.parse().ok()?;
    if n <= 0 {
        return None;
    }
    let multiplier = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        "w" => 604_800,
        _ => return None,
    };
    n.checked_mul(multiplier)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_epoch() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn utc_known_value() {
        // 2026-09-13T07:40:21Z
        assert_eq!(format_utc(1_789_285_221), "2026-09-13T07:40:21Z");
    }

    #[test]
    fn parse_roundtrip() {
        let secs = parse_datetime_prefix("1970-01-01T00:00:00").unwrap();
        assert_eq!(secs, 0);
    }

    #[test]
    fn parse_date_only() {
        let secs = parse_datetime_prefix("1970-01-02").unwrap();
        assert_eq!(secs, 86_400);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration("45m"), Some(2_700));
        assert_eq!(parse_duration("24h"), Some(86_400));
        assert_eq!(parse_duration("7d"), Some(604_800));
        assert_eq!(parse_duration("2w"), Some(1_209_600));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("7"), None);
        assert_eq!(parse_duration("7x"), None);
        assert_eq!(parse_duration("0d"), None);
    }
}
