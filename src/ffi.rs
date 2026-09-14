//! C ABI for native frontends (the macOS GUI).
//!
//! Every function returns a heap-allocated JSON string (schema version 1) or
//! `{"error":"..."}` on failure. Callers free the result with
//! `dd_free_string`. All storage logic stays in the Rust core; the GUI only
//! renders this data.

use crate::core::categories;
use crate::core::config::Config;
use crate::core::diff;
use crate::core::error::{Error, Result};
use crate::core::history;
use crate::core::json;
use crate::core::paths;
use crate::core::scan::{self, ScanConfig};
use crate::core::snapshot::CatVal;
use crate::core::store::Store;
use crate::core::time;
use crate::core::volume;
use crate::core::what_happened;
use crate::scanners;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};

fn cstr(ptr: *const c_char, what: &str) -> Result<Option<String>> {
    if ptr.is_null() {
        return Ok(None);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|s| Some(s.to_string()))
        .map_err(|_| Error::Message(format!("invalid UTF-8 in {what}")))
}

fn resolve_home(home: Option<String>) -> PathBuf {
    home.filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn resolve_data_dir(data_dir: Option<String>, home: &Path) -> PathBuf {
    data_dir
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Store::data_dir_for(home))
}

fn to_c(value: String) -> *mut c_char {
    CString::new(value)
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn ok<T: Serialize>(value: &T) -> *mut c_char {
    to_c(json::to_json_line(value))
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    error: &'a str,
}

fn err(error: &Error) -> *mut c_char {
    to_c(json::to_json_line(&ErrorPayload {
        error: &error.to_string(),
    }))
}

/// Free a string returned by any `dd_*` function.
///
/// # Safety
/// `ptr` must come from a `dd_*` function and not have been freed already.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dd_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

#[derive(Serialize)]
struct VersionPayload {
    version: &'static str,
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_version() -> *mut c_char {
    ok(&VersionPayload {
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Serialize)]
struct DiskUsagePayload {
    path: String,
    total_bytes: u64,
    free_bytes: u64,
    available_bytes: u64,
    used_bytes: u64,
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_disk_usage(path: *const c_char) -> *mut c_char {
    match disk_usage_impl(path) {
        Ok(value) => ok(&value),
        Err(e) => err(&e),
    }
}

fn disk_usage_impl(path: *const c_char) -> Result<DiskUsagePayload> {
    let home = resolve_home(None);
    let target = cstr(path, "path")?
        .filter(|p| !p.is_empty())
        .map(|p| paths::expand_user_path(&p, &home))
        .unwrap_or(home);
    let usage = volume::disk_usage(&target)?;
    Ok(DiskUsagePayload {
        path: target.to_string_lossy().to_string(),
        total_bytes: usage.total,
        free_bytes: usage.free,
        available_bytes: usage.available,
        used_bytes: usage.used,
    })
}

struct ScanRequest {
    home: PathBuf,
    data_dir: PathBuf,
    path: Option<PathBuf>,
    depth: Option<usize>,
    threads: Option<usize>,
}

fn scan_request(
    home: *const c_char,
    data_dir: *const c_char,
    path: *const c_char,
    depth: i32,
    threads: i32,
) -> Result<ScanRequest> {
    let home = resolve_home(cstr(home, "home")?);
    let data_dir = resolve_data_dir(cstr(data_dir, "data_dir")?, &home);
    let path = match cstr(path, "path")?.filter(|p| !p.is_empty()) {
        Some(p) => {
            let expanded = paths::expand_user_path(&p, &home);
            if !expanded.is_dir() {
                return Err(Error::Message(format!(
                    "{} is not a directory",
                    expanded.display()
                )));
            }
            Some(expanded)
        }
        None => None,
    };
    Ok(ScanRequest {
        home,
        data_dir,
        path,
        depth: (depth > 0).then_some(depth as usize),
        threads: (threads > 0).then_some(threads as usize),
    })
}

fn run_scan(request: &ScanRequest) -> Result<scan::ScanOutput> {
    let config = Config::load(&request.home, None)?;
    let targets = match &request.path {
        Some(path) => vec![scanners::generic::target_for(path.clone())],
        None => scanners::default_targets(&request.home),
    };
    let depth_override = request.depth.or(if request.path.is_some() {
        Some(config.depth.unwrap_or(2))
    } else {
        None
    });
    let mut exclusions = vec![request.data_dir.clone()];
    exclusions.extend(config.exclude.iter().cloned());
    let threads = request
        .threads
        .or(config.threads)
        .unwrap_or_else(scan::default_threads);
    Ok(scan::run(ScanConfig {
        home: &request.home,
        targets,
        threads,
        progress: None,
        exclusions,
        depth_override,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_scan(
    home: *const c_char,
    data_dir: *const c_char,
    path: *const c_char,
    depth: i32,
    threads: i32,
) -> *mut c_char {
    match scan_request(home, data_dir, path, depth, threads).and_then(|request| {
        let out = run_scan(&request)?;
        Ok(json::scan(&out))
    }) {
        Ok(value) => ok(&value),
        Err(e) => err(&e),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_snapshot(
    home: *const c_char,
    data_dir: *const c_char,
    path: *const c_char,
    depth: i32,
    threads: i32,
) -> *mut c_char {
    match scan_request(home, data_dir, path, depth, threads).and_then(|request| {
        let out = run_scan(&request)?;
        if out.walk.interrupted {
            return Err(Error::Message("scan was interrupted".into()));
        }
        let db_path = Store::default_path(&request.data_dir);
        let mut store = Store::open(&db_path)?;
        let meta = store.insert_snapshot(&out, env!("CARGO_PKG_VERSION"))?;
        Ok(json::snapshot(&meta))
    }) {
        Ok(value) => ok(&value),
        Err(e) => err(&e),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_history(
    home: *const c_char,
    data_dir: *const c_char,
    category: *const c_char,
) -> *mut c_char {
    match history_impl(home, data_dir, category) {
        Ok(value) => ok(&value),
        Err(e) => err(&e),
    }
}

fn history_impl(
    home: *const c_char,
    data_dir: *const c_char,
    category: *const c_char,
) -> Result<json::HistoryJson> {
    let home = resolve_home(cstr(home, "home")?);
    let data_dir = resolve_data_dir(cstr(data_dir, "data_dir")?, &home);
    let token = cstr(category, "category")?.filter(|c| !c.is_empty());
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no snapshots yet. Take a snapshot to start History.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let snapshots = store.all_snapshots()?;
    if snapshots.is_empty() {
        return Err(Error::Message(
            "no snapshots yet. Take a snapshot to start History.".into(),
        ));
    }

    let (category_idx, values) = match &token {
        None => (None, None),
        Some(token) => {
            let idx = categories::resolve_token(token)
                .ok_or_else(|| Error::Message(format!("unknown category '{token}'")))?;
            let mut map: HashMap<i64, CatVal> = HashMap::new();
            for meta in &snapshots {
                let leaf: HashMap<String, CatVal> = store
                    .load_categories(meta.id)?
                    .into_iter()
                    .map(|r| (r.category_id, r.val))
                    .collect();
                let rolled = diff::rollup(&leaf);
                map.insert(
                    meta.id,
                    rolled
                        .get(categories::def_by_index(idx).id)
                        .copied()
                        .unwrap_or_default(),
                );
            }
            (Some(idx), Some(map))
        }
    };
    let days = history::build_days(&snapshots, values.as_ref());
    Ok(json::history(&days, category_idx))
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_events(
    home: *const c_char,
    data_dir: *const c_char,
    since: *const c_char,
    limit: i32,
) -> *mut c_char {
    match events_impl(home, data_dir, since, limit) {
        Ok(value) => ok(&value),
        Err(e) => err(&e),
    }
}

fn events_impl(
    home: *const c_char,
    data_dir: *const c_char,
    since: *const c_char,
    limit: i32,
) -> Result<json::EventsJson> {
    let home = resolve_home(cstr(home, "home")?);
    let data_dir = resolve_data_dir(cstr(data_dir, "data_dir")?, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no events recorded yet. Run `diskdrift watch` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let since_unix = match cstr(since, "since")?.filter(|s| !s.is_empty()) {
        Some(value) => {
            let seconds = time::parse_duration(&value)
                .ok_or_else(|| Error::Message(format!("invalid duration '{value}'")))?;
            Some(time::now_unix() - seconds)
        }
        None => None,
    };
    let limit = if limit > 0 { limit as usize } else { 50 };
    let events = store.recent_events(since_unix, limit)?;
    Ok(json::events(&events, since_unix, &home))
}

#[unsafe(no_mangle)]
pub extern "C" fn dd_what_happened(
    home: *const c_char,
    data_dir: *const c_char,
    since: *const c_char,
    from: *const c_char,
    to: *const c_char,
    limit: i32,
) -> *mut c_char {
    match what_happened_impl(home, data_dir, since, from, to, limit) {
        Ok(value) => ok(&value),
        Err(e) => err(&e),
    }
}

fn what_happened_impl(
    home: *const c_char,
    data_dir: *const c_char,
    since: *const c_char,
    from: *const c_char,
    to: *const c_char,
    limit: i32,
) -> Result<json::WhatHappenedJson> {
    let home = resolve_home(cstr(home, "home")?);
    let data_dir = resolve_data_dir(cstr(data_dir, "data_dir")?, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no events recorded yet. Run `diskdrift watch` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let since = cstr(since, "since")?.filter(|s| !s.is_empty());
    let from = cstr(from, "from")?.filter(|s| !s.is_empty());
    let to = cstr(to, "to")?.filter(|s| !s.is_empty());
    let window = what_happened::resolve_window(
        since.as_deref(),
        from.as_deref(),
        to.as_deref(),
        time::now_unix(),
    )?;
    let events = store.events_between(window.from_unix, window.to_unix, 1_000_000)?;
    let limit = if limit > 0 { limit as usize } else { 10 };
    let report =
        what_happened::build_report(&events, window.from_unix, window.to_unix, &home, limit);
    Ok(json::what_happened(&report, &window, &home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(ptr: *mut c_char) -> String {
        assert!(!ptr.is_null(), "FFI returned a null pointer");
        let value = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        unsafe { dd_free_string(ptr) };
        value
    }

    #[test]
    fn version_payload() {
        let value = take(dd_version());
        assert!(value.contains(env!("CARGO_PKG_VERSION")), "{value}");
    }

    #[test]
    fn disk_usage_payload() {
        let path = CString::new("/").unwrap();
        let value = take(dd_disk_usage(path.as_ptr()));
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert!(parsed["total_bytes"].as_u64().unwrap() > 0);
        assert!(parsed.get("error").is_none());
    }

    #[test]
    fn scan_payload_for_temp_dir() {
        let dir = std::env::temp_dir().join(format!("diskdrift-ffi-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.bin"), vec![0u8; 4_000]).unwrap();

        let home = CString::new(dir.to_string_lossy().as_bytes()).unwrap();
        let data = CString::new(dir.join("data").to_string_lossy().as_bytes()).unwrap();
        // Scan only the temporary directory, never the real machine's
        // default locations.
        let path = CString::new(dir.to_string_lossy().as_bytes()).unwrap();
        let value = take(dd_scan(home.as_ptr(), data.as_ptr(), path.as_ptr(), 2, 2));
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert_eq!(parsed["command"], "scan");
        assert_eq!(parsed["totals"]["logical_bytes"].as_u64().unwrap(), 4_000);
        assert!(parsed.get("error").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_are_json() {
        let missing = CString::new("/definitely/not/here").unwrap();
        let value = take(dd_scan(
            std::ptr::null(),
            std::ptr::null(),
            missing.as_ptr(),
            0,
            0,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert!(
            parsed["error"]
                .as_str()
                .unwrap()
                .contains("not a directory")
        );
    }
}
