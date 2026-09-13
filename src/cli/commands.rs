//! Command implementations.

use crate::cli::args::{Command, CommonArgs, DiffArgs, ExplainArgs, ScanArgs};
use crate::cli::progress::Progress;
use crate::cli::render;
use crate::core::diff;
use crate::core::doctor;
use crate::core::error::{Error, Result};
use crate::core::explain::{self, ResolvedQuery};
use crate::core::fs::ProgressCounters;
use crate::core::json;
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

fn build_targets(home: &Path, roots: &[PathBuf]) -> Vec<ScanTarget> {
    if roots.is_empty() {
        scanners::default_targets(home)
    } else {
        roots
            .iter()
            .map(|r| {
                let expanded = explain::expand_tilde(&r.to_string_lossy(), home);
                scanners::generic::target_for(expanded)
            })
            .collect()
    }
}

fn perform_scan(args: &ScanArgs, home: &Path, data_dir: &Path) -> scan::ScanOutput {
    let targets = build_targets(home, &args.roots);
    let counters = Arc::new(ProgressCounters::new());
    let progress_enabled = !args.common.json && !args.common.no_progress;
    let progress = Progress::start(counters.clone(), progress_enabled);
    let out = scan::run(ScanConfig {
        home,
        targets,
        threads: args.threads.unwrap_or_else(scan::default_threads),
        progress: Some(&counters),
        // Never include our own database in a scan/snapshot.
        exclusions: vec![data_dir.to_path_buf()],
    });
    drop(progress);
    out
}

fn cmd_scan(args: ScanArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let out = perform_scan(&args, &home, &data_dir);
    if args.common.json {
        println!("{}", json::to_pretty(&json::scan(&out)));
    } else {
        let stdout = io::stdout();
        let mut w = stdout.lock();
        render::render_scan(&mut w, &out, &home, args.common.verbose)?;
        w.flush()?;
    }
    Ok(if out.walk.interrupted {
        EXIT_INTERRUPTED
    } else {
        EXIT_OK
    })
}

fn cmd_snapshot(args: ScanArgs) -> Result<i32> {
    let home = resolve_home();
    let data_dir = resolve_data_dir(&args.common, &home);
    let out = perform_scan(&args, &home, &data_dir);
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
    let threads = args.threads.unwrap_or_else(scan::default_threads);
    let data_dir = resolve_data_dir(&args.common, &home);
    let output = match query {
        ResolvedQuery::Category(idx) => explain::explain_category(
            idx,
            &home,
            &all_targets,
            threads,
            Some(&counters),
            std::slice::from_ref(&data_dir),
        ),
        ResolvedQuery::Path(path) => explain::explain_path(
            &path,
            &home,
            threads,
            Some(&counters),
            std::slice::from_ref(&data_dir),
        )?,
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
    let report = doctor::run(&home, &data_dir, &targets)?;
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
