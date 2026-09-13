//! Command implementations.

use crate::cli::args::{Command, CommonArgs, DiffArgs, ExplainArgs, ScanArgs, TopArgs};
use crate::cli::progress::Progress;
use crate::cli::render;
use crate::core::config::Config;
use crate::core::diff;
use crate::core::doctor;
use crate::core::error::{Error, Result};
use crate::core::explain::{self, ResolvedQuery};
use crate::core::fs::ProgressCounters;
use crate::core::json;
use crate::core::paths;
use crate::core::scan::{self, ScanConfig, ScanTarget};
use crate::core::store::Store;
use crate::scanners;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_INTERRUPTED: i32 = 130;

pub fn run(cmd: Command) -> Result<i32> {
    match cmd {
        Command::Scan(args) => cmd_scan(args),
        Command::Top(args) => cmd_top(args),
        Command::Snapshot(args) => cmd_snapshot(args),
        Command::Diff(args) => cmd_diff(args),
        Command::Explain(args) => cmd_explain(args),
        Command::Doctor(common) => cmd_doctor(common),
        Command::Snapshots(common) => cmd_snapshots(common),
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

    let (old_meta, new_meta) = match (&args.old, &args.new) {
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
