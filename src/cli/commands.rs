//! Command implementations.

use crate::cli::args::{
    Command, CommonArgs, DiffArgs, DuplicatesArgs, EventsArgs, ExplainArgs, HistoryArgs, LargeArgs,
    RecommendationsArgs, ScanArgs, SnapshotDeleteArgs, SnapshotShowArgs, TopArgs, WatchArgs,
    WhatHappenedArgs,
};
use crate::cli::progress::Progress;
use crate::cli::render;
use crate::core::categories;
use crate::core::config::Config;
use crate::core::diff;
use crate::core::doctor;
use crate::core::duplicates;
use crate::core::error::{Error, Result};
use crate::core::explain::{self, ResolvedQuery};
use crate::core::fs::ProgressCounters;
use crate::core::history;
use crate::core::json;
use crate::core::large;
use crate::core::local_snapshot;
use crate::core::paths;
use crate::core::recommendations;
use crate::core::scan::{self, ScanConfig, ScanTarget};
use crate::core::size;
use crate::core::store::Store;
use crate::core::system;
use crate::core::time;
use crate::core::volumes;
use crate::core::watch::{self, WatchEntry, WatchState};
use crate::core::what_happened;
use crate::scanners;
use notify::{RecursiveMode, Watcher};
use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_INTERRUPTED: i32 = 130;

pub fn run(cmd: Command) -> Result<i32> {
    match cmd {
        Command::Scan(args) => cmd_scan(args),
        Command::Top(args) => cmd_top(args),
        Command::Snapshot(args) => cmd_snapshot(args),
        Command::Diff(args) => cmd_diff(args),
        Command::History(args) => cmd_history(args),
        Command::Watch(args) => cmd_watch(args),
        Command::Events(args) => cmd_events(args),
        Command::WhatHappened(args) => cmd_what_happened(args),
        Command::Large(args) => cmd_large(args),
        Command::Duplicates(args) => cmd_duplicates(args),
        Command::Recommendations(args) => cmd_recommendations(args),
        Command::SnapshotShow(args) => cmd_snapshot_show(args),
        Command::SnapshotDelete(args) => cmd_snapshot_delete(args),
        Command::Explain(args) => cmd_explain(args),
        Command::Doctor(common) => cmd_doctor(common),
        Command::Snapshots(common) => cmd_snapshots(common),
        Command::MacSnapshots(common) => cmd_mac_snapshots(common),
        Command::Volumes(common) => cmd_volumes(common),
        Command::System(common) => cmd_system(common),
        Command::Help(_) => {
            print!("{}", crate::cli::args::usage());
            Ok(EXIT_OK)
        }
        Command::Version => {
            println!("diskdrift {}", env!("CARGO_PKG_VERSION"));
            Ok(EXIT_OK)
        }
    }
}

pub fn resolve_home() -> PathBuf {
    for key in ["DISKDRIFT_HOME", "HOME"] {
        if let Some(v) = std::env::var_os(key)
            && !v.is_empty()
        {
            return PathBuf::from(v);
        }
    }
    PathBuf::from("/")
}

fn resolve_data_dir(common: &CommonArgs, home: &Path) -> PathBuf {
    if let Some(dir) = &common.data_dir {
        return dir.clone();
    }
    if let Some(v) = std::env::var_os("DISKDRIFT_DATA_DIR")
        && !v.is_empty()
    {
        return PathBuf::from(v);
    }
    Store::data_dir_for(home)
}

fn load_config(common: &CommonArgs, home: &Path) -> Result<Config> {
    Config::load(home, common.config.as_deref())
}

/// Build the scan target list from a positional path, `--root` values or the
/// built-in defaults. Explicitly requested paths must exist and be
/// directories.
fn resolve_scan_targets(
    home: &Path,
    path: Option<&Path>,
    roots: &[PathBuf],
) -> Result<Vec<ScanTarget>> {
    if let Some(p) = path {
        let expanded = paths::expand_user_path(&p.to_string_lossy(), home);
        let md = std::fs::metadata(&expanded)
            .map_err(|e| Error::Message(format!("cannot access {}: {e}", expanded.display())))?;
        if !md.is_dir() {
            return Err(Error::Message(format!(
                "{} is not a directory",
                expanded.display()
            )));
        }
        return Ok(vec![scanners::generic::target_for(expanded)]);
    }
    if roots.is_empty() {
        return Ok(scanners::default_targets(home));
    }
    let mut targets = Vec::with_capacity(roots.len());
    for r in roots {
        let expanded = paths::expand_user_path(&r.to_string_lossy(), home);
        let md = std::fs::metadata(&expanded)
            .map_err(|e| Error::Message(format!("cannot access {}: {e}", expanded.display())))?;
        if !md.is_dir() {
            return Err(Error::Message(format!(
                "{} is not a directory",
                expanded.display()
            )));
        }
        targets.push(scanners::generic::target_for(expanded));
    }
    Ok(targets)
}

#[allow(clippy::too_many_arguments)]
fn scan_with(
    home: &Path,
    data_dir: &Path,
    config: &Config,
    path: Option<&Path>,
    roots: &[PathBuf],
    depth: Option<usize>,
    threads: Option<usize>,
    progress_enabled: bool,
) -> Result<scan::ScanOutput> {
    let targets = resolve_scan_targets(home, path, roots)?;
    let custom = path.is_some() || !roots.is_empty();
    // Config `depth` is the default for explicitly scanned paths;
    // `--depth` overrides everything.
    let depth_override = depth.or(if custom {
        Some(config.depth.unwrap_or(2))
    } else {
        None
    });

    // Never include our own database (or config exclusions) in a scan.
    let mut exclusions = vec![data_dir.to_path_buf()];
    exclusions.extend(config.exclude.iter().cloned());

    let counters = Arc::new(ProgressCounters::new());
    let progress = Progress::start(counters.clone(), progress_enabled);
    let out = scan::run(ScanConfig {
        home,
        targets,
        threads: threads
            .or(config.threads)
            .unwrap_or_else(scan::default_threads),
        progress: Some(&counters),
        exclusions,
        depth_override,
    });
    drop(progress);
    Ok(out)
}

fn print_scan_result(
    out: &scan::ScanOutput,
    home: &Path,
    common: &CommonArgs,
    show_directories: bool,
) -> Result<()> {
    if common.json {
        println!("{}", json::to_pretty(&json::scan(out)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_scan(&mut w, out, home, common.verbose, show_directories)?;
        w.flush()?;
    }
    Ok(())
}

fn cmd_scan(args: ScanArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let progress_enabled = !args.common.json && !args.common.no_progress;
    let out = scan_with(
        &home,
        &data_dir,
        &config,
        args.path.as_deref(),
        &args.roots,
        args.depth,
        args.threads,
        progress_enabled,
    )?;
    // Directory details are shown for explicit paths and depth overrides,
    // where the user asked for a more detailed structure.
    let show_directories = args.path.is_some() || args.depth.is_some();
    print_scan_result(&out, &home, &args.common, show_directories)?;
    Ok(if out.walk.interrupted {
        EXIT_INTERRUPTED
    } else {
        EXIT_OK
    })
}

fn cmd_top(args: TopArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let progress_enabled = !args.common.json && !args.common.no_progress;
    let out = scan_with(
        &home,
        &data_dir,
        &config,
        args.path.as_deref(),
        &args.roots,
        args.depth,
        args.threads,
        progress_enabled,
    )?;
    if args.common.json {
        println!("{}", json::to_pretty(&json::top(&out, args.limit)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_top(&mut w, &out, &home, args.limit)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_snapshot(args: ScanArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let progress_enabled = !args.common.json && !args.common.no_progress;
    let out = scan_with(
        &home,
        &data_dir,
        &config,
        args.path.as_deref(),
        &args.roots,
        args.depth,
        args.threads,
        progress_enabled,
    )?;
    if out.walk.interrupted {
        eprintln!("Interrupted — snapshot was not saved (partial data is never stored).");
        return Ok(EXIT_INTERRUPTED);
    }
    let db_path = Store::default_path(&data_dir);
    let mut store = Store::open(&db_path)?;
    let meta = store.insert_snapshot(&out, env!("CARGO_PKG_VERSION"))?;

    if args.common.json {
        println!("{}", json::to_pretty(&json::snapshot(&meta)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_snapshot_created(&mut w, &meta)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_diff(args: DiffArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(format!(
            "no snapshots found ({} does not exist).\nRun `diskdrift snapshot` first.",
            db_path.display()
        )));
    }
    let store = Store::open(&db_path)?;
    if store.snapshot_count()? == 0 {
        return Err(Error::Message(
            "no snapshots stored yet. Run `diskdrift snapshot` first.".into(),
        ));
    }

    let (old_meta, new_meta) = if let Some(since) = &args.since {
        let seconds = time::parse_duration(since).ok_or_else(|| {
            Error::Message(format!(
                "--since expects a duration like 24h or 7d, got '{since}'"
            ))
        })?;
        store
            .resolve_since(seconds)
            .map_err(|e| Error::Message(format!("{e}\n(requested: --since {since})")))?
    } else {
        match (&args.old, &args.new) {
            (None, None) => {
                let list = store.list_snapshots(2)?;
                if list.len() < 2 {
                    return Err(Error::Message(
                    "need at least two snapshots to diff. Run `diskdrift snapshot` again later."
                        .into(),
                ));
                }
                (list[1].clone(), list[0].clone())
            }
            (Some(old), None) => {
                let new = store.resolve_snapshot("latest")?;
                // The explicit argument must resolve to something older than
                // the latest snapshot, otherwise diffing is meaningless.
                let old = store.resolve_snapshot_before(old, new.id)?;
                (old, new)
            }
            (Some(old), Some(new)) => (store.resolve_snapshot(old)?, store.resolve_snapshot(new)?),
            (None, Some(new)) => {
                let new = store.resolve_snapshot(new)?;
                let old = store.resolve_snapshot_before("latest", new.id)?;
                (old, new)
            }
        }
    };
    if old_meta.id == new_meta.id {
        return Err(Error::Message(
            "cannot diff a snapshot against itself".into(),
        ));
    }

    let old_side = diff::load_side(&store, old_meta.id)?;
    let new_side = diff::load_side(&store, new_meta.id)?;
    let result = diff::compute(old_side, new_side, args.top);

    if args.common.json {
        println!("{}", json::to_pretty(&json::diff(&result)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_diff(&mut w, &result, &home, args.top)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_history(args: HistoryArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no snapshots found. Run `diskdrift snapshot` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let snapshots = store.all_snapshots()?;
    if snapshots.is_empty() {
        return Err(Error::Message(
            "no snapshots stored yet. Run `diskdrift snapshot` first.".into(),
        ));
    }

    let (category, values) = match &args.category {
        None => (None, None),
        Some(token) => {
            let idx = categories::resolve_token(token).ok_or_else(|| {
                Error::Message(format!(
                    "unknown category '{token}'. Try `diskdrift explain {token}` or see the README"
                ))
            })?;
            let mut map: HashMap<i64, crate::core::snapshot::CatVal> = HashMap::new();
            for meta in &snapshots {
                let leaf: HashMap<String, crate::core::snapshot::CatVal> = store
                    .load_categories(meta.id)?
                    .into_iter()
                    .map(|r| (r.category_id, r.val))
                    .collect();
                let rolled = diff::rollup(&leaf);
                let value = rolled
                    .get(categories::def_by_index(idx).id)
                    .copied()
                    .unwrap_or_default();
                map.insert(meta.id, value);
            }
            (Some(idx), Some(map))
        }
    };

    let days = history::build_days(&snapshots, values.as_ref());
    if args.common.json {
        println!("{}", json::to_pretty(&json::history(&days, category)));
    } else {
        let category_name = category.map(|i| categories::def_by_index(i).name);
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_history(&mut w, &days, category_name)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_large(args: LargeArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let targets = resolve_scan_targets(&home, args.path.as_deref(), &args.roots)?;
    let paths: Vec<PathBuf> = targets.iter().map(|target| target.path.clone()).collect();
    let min_size = args
        .min_size
        .as_deref()
        .and_then(size::parse_size)
        .unwrap_or(10_000_000);
    let mut exclusions = vec![data_dir];
    exclusions.extend(config.exclude.iter().cloned());

    let files = large::largest_files(&paths, args.limit, min_size, &exclusions);
    if args.common.json {
        println!("{}", json::to_pretty(&json::large(&files, min_size)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_large(&mut w, &files, &home, min_size)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_duplicates(args: DuplicatesArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let targets = resolve_scan_targets(&home, args.path.as_deref(), &args.roots)?;
    let paths: Vec<PathBuf> = targets.iter().map(|target| target.path.clone()).collect();
    let min_size = args
        .min_size
        .as_deref()
        .and_then(size::parse_size)
        .unwrap_or(1_000_000);
    let mut exclusions = vec![data_dir];
    exclusions.extend(config.exclude.iter().cloned());

    let report =
        duplicates::find_duplicates(&paths, min_size, args.limit, args.models, &exclusions);
    if args.common.json {
        println!("{}", json::to_pretty(&json::duplicates(&report, &home)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_duplicates(&mut w, &report, &home)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_recommendations(args: RecommendationsArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let targets = resolve_scan_targets(&home, args.path.as_deref(), &args.roots)?;
    let mut exclusions = vec![data_dir];
    exclusions.extend(config.exclude.iter().cloned());

    let counters = Arc::new(ProgressCounters::new());
    let progress_enabled = !args.common.json && !args.common.no_progress;
    let progress = Progress::start(counters.clone(), progress_enabled);
    let out = scan::run(ScanConfig {
        home: &home,
        targets,
        threads: args
            .threads
            .or(config.threads)
            .unwrap_or_else(scan::default_threads),
        progress: Some(&counters),
        exclusions,
        depth_override: None,
    });
    drop(progress);

    let rolled = crate::core::fs::rollup_categories(&out.walk.categories);
    let insights = recommendations::recommendations(&rolled);
    if args.common.json {
        println!("{}", json::to_pretty(&json::recommendations(&insights)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_recommendations(&mut w, &insights)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_watch(args: WatchArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let config = load_config(&args.common, &home)?;
    let targets = resolve_scan_targets(&home, None, &args.roots)?;
    // FSEvents reports canonical paths (e.g. /private/var for /var), so all
    // watcher paths, baseline keys and measurements use canonical paths too.
    let canonical_targets: Vec<ScanTarget> = targets
        .iter()
        .map(|target| {
            let mut canonical = target.clone();
            canonical.path =
                std::fs::canonicalize(&target.path).unwrap_or_else(|_| target.path.clone());
            canonical
        })
        .collect();

    let debounce = Duration::from_millis(match &args.debounce {
        Some(value) => time::parse_duration_ms(value).ok_or_else(|| {
            Error::Message(format!("--debounce expects a duration, got '{value}'"))
        })?,
        None => 2_000,
    });
    let min_change = match &args.min_change {
        Some(value) => size::parse_size(value)
            .ok_or_else(|| Error::Message(format!("--min-change expects a size, got '{value}'")))?,
        None => 1_000_000,
    };
    let baseline_depth = args
        .baseline_depth
        .or(config.depth)
        .unwrap_or(3)
        .clamp(1, crate::core::config::MAX_DEPTH);
    let run_for = match &args.run_for {
        Some(value) => Some(time::parse_duration_ms(value).ok_or_else(|| {
            Error::Message(format!("--run-for expects a duration, got '{value}'"))
        })?),
        None => None,
    };
    let threads = args
        .threads
        .or(config.threads)
        .unwrap_or_else(scan::default_threads);

    let canonical_home = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone());
    let mut exclusions = vec![data_dir.clone()];
    exclusions.extend(config.exclude.iter().cloned());
    let exclusions: Vec<PathBuf> = exclusions
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect();

    let db_path = Store::default_path(&data_dir);
    let mut store = Store::open(&db_path)?;
    let classifier = crate::core::classify::Classifier::new(&canonical_home);

    let mut state: WatchState = store
        .load_watch_dirs()?
        .into_iter()
        .map(|(path, category_id, allocated)| {
            (
                path,
                WatchEntry {
                    category_id,
                    allocated,
                },
            )
        })
        .collect();
    if state.is_empty() || args.rebaseline {
        eprintln!("Building baseline...");
        let (baseline, out) = watch::build_baseline(
            &canonical_home,
            &canonical_targets,
            baseline_depth,
            threads,
            &exclusions,
        );
        let rows: Vec<(PathBuf, String, u64)> = baseline
            .iter()
            .map(|(path, entry)| (path.clone(), entry.category_id.clone(), entry.allocated))
            .collect();
        store.upsert_watch_dirs(&rows)?;
        state = baseline;
        eprintln!(
            "Baseline: {} directories, {} tracked in {:.1}s",
            size::format_count(rows.len() as u64),
            size::format_bytes(out.walk.totals.allocated),
            out.duration.as_secs_f64()
        );
    } else {
        eprintln!(
            "Loaded baseline: {} directories (use --rebaseline to rescan)",
            size::format_count(state.len() as u64)
        );
    }

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| Error::Message(format!("cannot start filesystem watcher: {e}")))?;
    for target in &canonical_targets {
        watcher
            .watch(&target.path, RecursiveMode::Recursive)
            .map_err(|e| {
                Error::Message(format!(
                    "cannot watch {}: {e}",
                    paths::display_path(&target.path, &home)
                ))
            })?;
    }
    eprintln!(
        "Watching {} locations (debounce {}ms, min change {}). Press Ctrl+C to stop.",
        canonical_targets.len(),
        debounce.as_millis(),
        size::format_bytes(min_change)
    );

    let started = Instant::now();
    let mut pending: Vec<PathBuf> = Vec::new();
    let mut last_event = Instant::now();
    let mut recorded = 0u64;

    loop {
        if crate::core::interrupt::interrupted() {
            break;
        }
        if let Some(limit) = run_for {
            if started.elapsed().as_millis() as u64 >= limit {
                break;
            }
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(event)) => {
                for path in event.paths {
                    pending.push(dirty_path_for(&path));
                }
                last_event = Instant::now();
            }
            Ok(Err(_)) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if !pending.is_empty() && last_event.elapsed() >= debounce {
            let dirty = std::mem::take(&mut pending);
            let outcome = watch::process_batch(
                &dirty,
                &canonical_home,
                &classifier,
                &mut state,
                min_change,
                threads,
                &exclusions,
                time::now_unix(),
            );
            if !outcome.events.is_empty() {
                store.insert_events(&outcome.events)?;
                store.upsert_watch_dirs(&outcome.updated)?;
                store.delete_watch_dirs(&outcome.removed)?;
                recorded += outcome.events.len() as u64;
                for event in &outcome.events {
                    print_watch_event(event, &home, args.common.json)?;
                }
            }
        }
    }

    eprintln!(
        "Stopped. {} event(s) recorded. See `diskdrift events`.",
        size::format_count(recorded)
    );
    Ok(if crate::core::interrupt::interrupted() {
        EXIT_INTERRUPTED
    } else {
        EXIT_OK
    })
}

fn dirty_path_for(path: &Path) -> PathBuf {
    // FSEvents reports both files and directories, using canonical paths.
    // Files are measured through their parent directory; deleted paths fall
    // back to the (canonicalised) parent.
    if let Ok(canonical) = std::fs::canonicalize(path) {
        if canonical.is_dir() {
            return canonical;
        }
        return canonical
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(canonical);
    }
    match path.parent() {
        Some(parent) => std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()),
        None => path.to_path_buf(),
    }
}

fn print_watch_event(
    event: &crate::core::snapshot::EventDraft,
    home: &Path,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", json::to_json_line(&json::watch_event(event, home)));
    } else {
        let time = time::format_local(event.timestamp_unix);
        let short = time.get(11..16).unwrap_or("--:--");
        println!(
            "{short}  {}  {}",
            paths::display_path(&event.path, home),
            size::format_delta(event.delta_bytes)
        );
    }
    Ok(())
}

fn cmd_events(args: EventsArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no event database found. Run `diskdrift watch` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let since = match &args.since {
        Some(value) => Some(
            time::now_unix()
                - time::parse_duration(value).ok_or_else(|| {
                    Error::Message(format!("--since expects a duration, got '{value}'"))
                })?,
        ),
        None => None,
    };
    let events = store.recent_events(since, args.limit)?;

    if args.common.json {
        println!("{}", json::to_pretty(&json::events(&events, since, &home)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_events(&mut w, &events, &home)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_what_happened(args: WhatHappenedArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no event database found. Run `diskdrift watch` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let window = what_happened::resolve_window(
        args.since.as_deref(),
        args.from.as_deref(),
        args.to.as_deref(),
        time::now_unix(),
    )?;
    let events = store.events_between(window.from_unix, window.to_unix, 1_000_000)?;
    let report =
        what_happened::build_report(&events, window.from_unix, window.to_unix, &home, args.limit);

    if args.common.json {
        println!(
            "{}",
            json::to_pretty(&json::what_happened(&report, &window, &home))
        );
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_what_happened(&mut w, &report, &window)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_snapshot_show(args: SnapshotShowArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no snapshots found. Run `diskdrift snapshot` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let meta = store.resolve_snapshot(&args.id)?;
    let leaf: HashMap<String, crate::core::snapshot::CatVal> = store
        .load_categories(meta.id)?
        .into_iter()
        .map(|r| (r.category_id, r.val))
        .collect();
    let rolled = diff::rollup(&leaf);
    let mut rows: Vec<(usize, crate::core::snapshot::CatVal)> = rolled
        .iter()
        .filter_map(|(id, val)| categories::index_of(id).map(|idx| (idx, *val)))
        .filter(|(_, val)| !val.is_zero())
        .collect();
    rows.sort_by_key(|(_, val)| std::cmp::Reverse(val.allocated));
    rows.truncate(10);

    if args.common.json {
        println!("{}", json::to_pretty(&json::snapshot_show(&meta, &rows)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_snapshot_show(&mut w, &meta, &rows)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_snapshot_delete(args: SnapshotDeleteArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let db_path = Store::default_path(&data_dir);
    if !db_path.exists() {
        return Err(Error::Message(
            "no snapshots found. Run `diskdrift snapshot` first.".into(),
        ));
    }
    let store = Store::open(&db_path)?;
    let meta = store.resolve_snapshot(&args.id)?;

    let mut confirmed = args.yes;
    if !confirmed {
        if std::io::stdin().is_terminal() {
            eprint!(
                "Delete snapshot #{} ({} tracked, {})? [y/N] ",
                meta.id,
                size::format_bytes(meta.total_allocated_bytes),
                time::format_local_display(meta.unix_time())
            );
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            confirmed = matches!(line.trim().to_lowercase().as_str(), "y" | "yes");
        } else {
            return Err(Error::Message(
                "refusing to delete without --yes in a non-interactive session".into(),
            ));
        }
    }
    if !confirmed {
        println!("Cancelled.");
        return Ok(EXIT_OK);
    }

    if !store.delete_snapshot(meta.id)? {
        return Err(Error::Message(format!("snapshot #{} not found", meta.id)));
    }
    if args.common.json {
        println!("{}", json::to_pretty(&json::snapshot_deleted(&meta)));
    } else {
        println!(
            "Snapshot #{} deleted ({} tracked, {})",
            meta.id,
            size::format_bytes(meta.total_allocated_bytes),
            time::format_local_display(meta.unix_time())
        );
    }
    Ok(EXIT_OK)
}

fn cmd_explain(args: ExplainArgs) -> Result<i32> {
    let home = resolve_home();
    let all_targets = scanners::default_targets(&home);
    let query = explain::resolve_query(&args.query, &home)?;
    let counters = Arc::new(ProgressCounters::new());
    let progress_enabled = !args.common.json && !args.common.no_progress;
    let progress = Progress::start(counters.clone(), progress_enabled);
    let config = load_config(&args.common, &home)?;
    let threads = args
        .threads
        .or(config.threads)
        .unwrap_or_else(scan::default_threads);
    let data_dir = resolve_data_dir(&args.common, &home);
    let mut exclusions = vec![data_dir];
    exclusions.extend(config.exclude.iter().cloned());
    let output = match query {
        ResolvedQuery::Category(idx) => explain::explain_category(
            idx,
            &home,
            &all_targets,
            threads,
            Some(&counters),
            &exclusions,
        ),
        ResolvedQuery::Path(path) => {
            explain::explain_path(&path, &home, threads, Some(&counters), &exclusions)?
        }
    };
    drop(progress);

    if args.common.json {
        println!("{}", json::to_pretty(&json::explain(&output, &home)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_explain(&mut w, &output)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_doctor(common: CommonArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&common, &home);
    let targets = scanners::default_targets(&home);
    // Doctor still works when the config is broken; it reports the problem.
    let (config, config_error) = match Config::load(&home, common.config.as_deref()) {
        Ok(config) => (config, None),
        Err(e) => (Config::disabled(), Some(e.to_string())),
    };
    let report = doctor::run(&home, &data_dir, &targets, &config, config_error)?;
    if common.json {
        println!("{}", json::to_pretty(&json::doctor(&report, &home)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_doctor(&mut w, &report, &home)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_mac_snapshots(common: CommonArgs) -> Result<i32> {
    let snapshots = local_snapshot::list();
    if common.json {
        println!("{}", json::to_pretty(&json::local_snapshots(&snapshots)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_local_snapshots(&mut w, &snapshots)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_volumes(common: CommonArgs) -> Result<i32> {
    let volumes = volumes::volumes()?;
    if common.json {
        println!("{}", json::to_pretty(&json::volumes(&volumes)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_volumes(&mut w, &volumes)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_system(common: CommonArgs) -> Result<i32> {
    let home = resolve_home();
    let report = system::report(&home, scan::default_threads());
    if common.json {
        println!("{}", json::to_pretty(&json::system(&report)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_system(&mut w, &report)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}

fn cmd_snapshots(common: CommonArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&common, &home);
    let db_path = Store::default_path(&data_dir);
    let list = if db_path.exists() {
        let store = Store::open(&db_path)?;
        store.list_snapshots(100)?
    } else {
        Vec::new()
    };
    if common.json {
        println!("{}", json::to_pretty(&json::snapshots(&list)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_snapshots(&mut w, &list, &home)?;
        w.flush()?;
    }
    Ok(EXIT_OK)
}
