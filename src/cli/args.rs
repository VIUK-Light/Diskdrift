//! Hand-rolled argument parsing (no clap): the CLI surface is small and
//! stable, and this keeps the binary dependency-light.

use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct CommonArgs {
    pub json: bool,
    pub no_progress: bool,
    pub verbose: bool,
    pub data_dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ScanArgs {
    pub common: CommonArgs,
    pub roots: Vec<PathBuf>,
    pub path: Option<PathBuf>,
    pub threads: Option<usize>,
    pub depth: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TopArgs {
    pub common: CommonArgs,
    pub roots: Vec<PathBuf>,
    pub path: Option<PathBuf>,
    pub threads: Option<usize>,
    pub depth: Option<usize>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct DiffArgs {
    pub common: CommonArgs,
    pub old: Option<String>,
    pub new: Option<String>,
    pub top: usize,
    pub since: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HistoryArgs {
    pub common: CommonArgs,
    pub category: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WatchArgs {
    pub common: CommonArgs,
    pub roots: Vec<PathBuf>,
    pub debounce: Option<String>,
    pub min_change: Option<String>,
    pub baseline_depth: Option<usize>,
    pub rebaseline: bool,
    pub run_for: Option<String>,
    pub threads: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct EventsArgs {
    pub common: CommonArgs,
    pub since: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct WhatHappenedArgs {
    pub common: CommonArgs,
    pub since: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct SnapshotShowArgs {
    pub common: CommonArgs,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct SnapshotDeleteArgs {
    pub common: CommonArgs,
    pub id: String,
    pub yes: bool,
}

#[derive(Debug, Clone)]
pub struct ExplainArgs {
    pub common: CommonArgs,
    pub query: String,
    pub threads: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum Command {
    Scan(ScanArgs),
    Top(TopArgs),
    Snapshot(ScanArgs),
    Diff(DiffArgs),
    History(HistoryArgs),
    Watch(WatchArgs),
    Events(EventsArgs),
    WhatHappened(WhatHappenedArgs),
    SnapshotShow(SnapshotShowArgs),
    SnapshotDelete(SnapshotDeleteArgs),
    Explain(ExplainArgs),
    Doctor(CommonArgs),
    Snapshots(CommonArgs),
    Help(Option<String>),
    Version,
}

pub fn parse(argv: &[String]) -> Result<Command, String> {
    if argv.is_empty() {
        return Ok(Command::Help(None));
    }
    let cmd = argv[0].as_str();
    let rest = &argv[1..];
    match cmd {
        "scan" => match parse_scan_args(rest) {
            Ok(args) => Ok(Command::Scan(args)),
            Err(e) if e == "HELP" => Ok(Command::Help(Some(cmd.to_string()))),
            Err(e) => Err(e),
        },
        "snapshot" => parse_snapshot(rest),
        "top" => parse_top(rest),
        "diff" => parse_diff(rest),
        "history" => parse_history(rest),
        "watch" => parse_watch(rest),
        "events" => parse_events(rest),
        "what-happened" => parse_what_happened(rest),
        "explain" => parse_explain(rest),
        "doctor" => parse_doctor(rest),
        "snapshots" | "list" => parse_snapshots(rest),
        "help" | "--help" | "-h" => Ok(Command::Help(rest.first().cloned())),
        "version" | "--version" | "-V" => Ok(Command::Version),
        other => Err(format!("unknown command '{other}'")),
    }
}

struct Args<'a> {
    items: &'a [String],
    i: usize,
}

impl<'a> Args<'a> {
    fn new(items: &'a [String]) -> Self {
        Args { items, i: 0 }
    }

    /// Returns (name, inline value) for the next argument.
    fn next(&mut self) -> Option<(String, Option<String>)> {
        let raw = self.items.get(self.i)?;
        self.i += 1;
        if let Some((name, value)) = raw.split_once('=')
            && name.starts_with("--")
        {
            return Some((name.to_string(), Some(value.to_string())));
        }
        Some((raw.clone(), None))
    }

    fn value(&mut self, flag: &str, inline: Option<String>) -> Result<String, String> {
        if let Some(v) = inline {
            return Ok(v);
        }
        let v = self
            .items
            .get(self.i)
            .cloned()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        self.i += 1;
        Ok(v)
    }
}

fn parse_common(
    args: &mut Args<'_>,
    name: &str,
    inline: Option<String>,
    common: &mut CommonArgs,
) -> Result<bool, String> {
    match name {
        "--json" => common.json = true,
        "--no-progress" => common.no_progress = true,
        "--verbose" | "-v" => common.verbose = true,
        "--data-dir" => {
            common.data_dir = Some(PathBuf::from(args.value(name, inline)?));
        }
        "--config" => {
            common.config = Some(PathBuf::from(args.value(name, inline)?));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn parse_count(
    args: &mut Args<'_>,
    flag: &str,
    inline: Option<String>,
    min: usize,
    max: usize,
) -> Result<usize, String> {
    let v = args.value(flag, inline)?;
    let n = v
        .parse::<usize>()
        .map_err(|_| format!("{flag} expects a number, got '{v}'"))?;
    if n < min || n > max {
        return Err(format!("{flag} must be between {min} and {max}"));
    }
    Ok(n)
}

fn parse_scan_args(rest: &[String]) -> Result<ScanArgs, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut roots = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut threads = None;
    let mut depth = None;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--root" => roots.push(PathBuf::from(args.value("--root", inline)?)),
            "--threads" => {
                threads = Some(parse_count(&mut args, "--threads", inline, 1, 1024)?);
            }
            "--depth" => {
                depth = Some(parse_count(
                    &mut args,
                    "--depth",
                    inline,
                    1,
                    crate::core::config::MAX_DEPTH,
                )?);
            }
            "-h" | "--help" => return Err("HELP".into()),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            other => {
                if path.is_some() {
                    return Err("scan accepts at most one path".into());
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    if path.is_some() && !roots.is_empty() {
        return Err("use either a path argument or --root, not both".into());
    }
    Ok(ScanArgs {
        common,
        roots,
        path,
        threads,
        depth,
    })
}

fn parse_top(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut roots = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut threads = None;
    let mut depth = None;
    let mut limit = 20usize;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--root" => roots.push(PathBuf::from(args.value("--root", inline)?)),
            "--threads" => {
                threads = Some(parse_count(&mut args, "--threads", inline, 1, 1024)?);
            }
            "--depth" => {
                depth = Some(parse_count(
                    &mut args,
                    "--depth",
                    inline,
                    1,
                    crate::core::config::MAX_DEPTH,
                )?);
            }
            "--limit" => {
                limit = parse_count(&mut args, "--limit", inline, 1, 1000)?;
            }
            "-h" | "--help" => return Ok(Command::Help(Some("top".into()))),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            other => {
                if path.is_some() {
                    return Err("top accepts at most one path".into());
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    if path.is_some() && !roots.is_empty() {
        return Err("use either a path argument or --root, not both".into());
    }
    Ok(Command::Top(TopArgs {
        common,
        roots,
        path,
        threads,
        depth,
        limit,
    }))
}

fn parse_diff(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut positional: Vec<String> = Vec::new();
    let mut top = 20usize;
    let mut since: Option<String> = None;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--top" => {
                let v = args.value("--top", inline)?;
                top = v
                    .parse::<usize>()
                    .map_err(|_| format!("--top expects a number, got '{v}'"))?;
            }
            "--since" => {
                let v = args.value("--since", inline)?;
                if crate::core::time::parse_duration(&v).is_none() {
                    return Err(format!(
                        "--since expects a duration like 24h or 7d, got '{v}'"
                    ));
                }
                since = Some(v);
            }
            "-h" | "--help" => return Ok(Command::Help(Some("diff".into()))),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            other => positional.push(other.to_string()),
        }
    }
    if positional.len() > 2 {
        return Err("diff accepts at most two snapshot arguments".into());
    }
    let mut it = positional.into_iter();
    let old = it.next();
    let new = it.next();
    if since.is_some() && (old.is_some() || new.is_some()) {
        return Err("--since cannot be combined with snapshot arguments".into());
    }
    Ok(Command::Diff(DiffArgs {
        common,
        old,
        new,
        top: top.max(1),
        since,
    }))
}

fn parse_history(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut category: Option<String> = None;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "-h" | "--help" => return Ok(Command::Help(Some("history".into()))),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            other => {
                if category.is_some() {
                    return Err("history accepts at most one category".into());
                }
                category = Some(other.to_string());
            }
        }
    }
    Ok(Command::History(HistoryArgs { common, category }))
}

fn parse_watch(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut roots = Vec::new();
    let mut debounce = None;
    let mut min_change = None;
    let mut baseline_depth = None;
    let mut rebaseline = false;
    let mut run_for = None;
    let mut threads = None;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--root" => roots.push(PathBuf::from(args.value("--root", inline)?)),
            "--debounce" => {
                let v = args.value("--debounce", inline)?;
                if crate::core::time::parse_duration_ms(&v).is_none() {
                    return Err(format!(
                        "--debounce expects a duration like 2s or 500ms, got '{v}'"
                    ));
                }
                debounce = Some(v);
            }
            "--min-change" => {
                let v = args.value("--min-change", inline)?;
                if crate::core::size::parse_size(&v).is_none() {
                    return Err(format!(
                        "--min-change expects a size like 10MB or 1GB, got '{v}'"
                    ));
                }
                min_change = Some(v);
            }
            "--baseline-depth" => {
                baseline_depth = Some(parse_count(
                    &mut args,
                    "--baseline-depth",
                    inline,
                    1,
                    crate::core::config::MAX_DEPTH,
                )?);
            }
            "--rebaseline" => rebaseline = true,
            "--run-for" => {
                let v = args.value("--run-for", inline)?;
                if crate::core::time::parse_duration_ms(&v).is_none() {
                    return Err(format!(
                        "--run-for expects a duration like 10s or 5m, got '{v}'"
                    ));
                }
                run_for = Some(v);
            }
            "--threads" => {
                threads = Some(parse_count(&mut args, "--threads", inline, 1, 1024)?);
            }
            "-h" | "--help" => return Ok(Command::Help(Some("watch".into()))),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(Command::Watch(WatchArgs {
        common,
        roots,
        debounce,
        min_change,
        baseline_depth,
        rebaseline,
        run_for,
        threads,
    }))
}

fn parse_events(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut since = None;
    let mut limit = 50usize;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--since" => {
                let v = args.value("--since", inline)?;
                if crate::core::time::parse_duration(&v).is_none() {
                    return Err(format!(
                        "--since expects a duration like 24h or 7d, got '{v}'"
                    ));
                }
                since = Some(v);
            }
            "--limit" => {
                limit = parse_count(&mut args, "--limit", inline, 1, 100_000)?;
            }
            "-h" | "--help" => return Ok(Command::Help(Some("events".into()))),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(Command::Events(EventsArgs {
        common,
        since,
        limit,
    }))
}

fn parse_what_happened(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut since = None;
    let mut from = None;
    let mut to = None;
    let mut limit = 10usize;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--since" => {
                let v = args.value("--since", inline)?;
                if crate::core::time::parse_duration(&v).is_none() {
                    return Err(format!(
                        "--since expects a duration like 1h or 24h, got '{v}'"
                    ));
                }
                since = Some(v);
            }
            "--from" => {
                let v = args.value("--from", inline)?;
                if crate::core::time::parse_hhmm(&v).is_none() {
                    return Err(format!("--from expects a time like 14:00, got '{v}'"));
                }
                from = Some(v);
            }
            "--to" => {
                let v = args.value("--to", inline)?;
                if crate::core::time::parse_hhmm(&v).is_none() {
                    return Err(format!("--to expects a time like 16:00, got '{v}'"));
                }
                to = Some(v);
            }
            "--limit" => {
                limit = parse_count(&mut args, "--limit", inline, 1, 1000)?;
            }
            "-h" | "--help" => return Ok(Command::Help(Some("what-happened".into()))),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    if since.is_some() && (from.is_some() || to.is_some()) {
        return Err("--since cannot be combined with --from/--to".into());
    }
    Ok(Command::WhatHappened(WhatHappenedArgs {
        common,
        since,
        from,
        to,
        limit,
    }))
}

fn parse_snapshot(rest: &[String]) -> Result<Command, String> {
    let sub = rest.first().map(|s| s.as_str());
    match sub {
        Some("list") => parse_snapshots(&rest[1..]),
        Some("show") => {
            let mut args = Args::new(&rest[1..]);
            let mut common = CommonArgs::default();
            let mut id: Option<String> = None;
            while let Some((name, inline)) = args.next() {
                if parse_common(&mut args, &name, inline.clone(), &mut common)? {
                    continue;
                }
                match name.as_str() {
                    "-h" | "--help" => return Ok(Command::Help(Some("snapshot".into()))),
                    other if other.starts_with('-') && other != "-" => {
                        return Err(format!("unknown option '{other}'"));
                    }
                    other => {
                        if id.is_some() {
                            return Err("snapshot show accepts one snapshot id".into());
                        }
                        id = Some(other.to_string());
                    }
                }
            }
            let id = id.ok_or_else(|| {
                "missing snapshot id: expected `diskdrift snapshot show <id>`".to_string()
            })?;
            Ok(Command::SnapshotShow(SnapshotShowArgs { common, id }))
        }
        Some("delete") => {
            let mut args = Args::new(&rest[1..]);
            let mut common = CommonArgs::default();
            let mut id: Option<String> = None;
            let mut yes = false;
            while let Some((name, inline)) = args.next() {
                if parse_common(&mut args, &name, inline.clone(), &mut common)? {
                    continue;
                }
                match name.as_str() {
                    "--yes" | "-y" => yes = true,
                    "-h" | "--help" => return Ok(Command::Help(Some("snapshot".into()))),
                    other if other.starts_with('-') && other != "-" => {
                        return Err(format!("unknown option '{other}'"));
                    }
                    other => {
                        if id.is_some() {
                            return Err("snapshot delete accepts one snapshot id".into());
                        }
                        id = Some(other.to_string());
                    }
                }
            }
            let id = id.ok_or_else(|| {
                "missing snapshot id: expected `diskdrift snapshot delete <id>`".to_string()
            })?;
            Ok(Command::SnapshotDelete(SnapshotDeleteArgs {
                common,
                id,
                yes,
            }))
        }
        _ => match parse_scan_args(rest) {
            Ok(args) => Ok(Command::Snapshot(args)),
            Err(e) if e == "HELP" => Ok(Command::Help(Some("snapshot".into()))),
            Err(e) => Err(e),
        },
    }
}

fn parse_explain(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut query: Option<String> = None;
    let mut threads = None;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--threads" => {
                let v = args.value("--threads", inline)?;
                threads = Some(
                    v.parse::<usize>()
                        .map_err(|_| format!("--threads expects a number, got '{v}'"))?,
                );
            }
            "-h" | "--help" => return Ok(Command::Help(Some("explain".into()))),
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unknown option '{other}'"));
            }
            other => {
                if query.is_some() {
                    return Err("explain accepts exactly one category or path".into());
                }
                query = Some(other.to_string());
            }
        }
    }
    let query = query.ok_or_else(|| {
        "missing argument: expected a category or path (e.g. `diskdrift explain xcode`)".to_string()
    })?;
    Ok(Command::Explain(ExplainArgs {
        common,
        query,
        threads,
    }))
}

fn parse_doctor(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "-h" | "--help" => return Ok(Command::Help(Some("doctor".into()))),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(Command::Doctor(common))
}

fn parse_snapshots(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "-h" | "--help" => return Ok(Command::Help(Some("snapshots".into()))),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(Command::Snapshots(common))
}

pub fn usage() -> &'static str {
    r#"DiskDrift — find out where your macOS storage went.

Usage:
  diskdrift scan [<path>] [--depth <n>] [--root <path>]... [--threads <n>]
                 [--json] [--no-progress] [--verbose]
  diskdrift top [<path>] [--depth <n>] [--limit <n>] [--root <path>]...
                [--threads <n>] [--json] [--no-progress]
  diskdrift snapshot [<path>] [--depth <n>] [--root <path>]... [--threads <n>]
                     [--json] [--no-progress]
  diskdrift snapshot list [--json]
  diskdrift snapshot show <id> [--json]
  diskdrift snapshot delete <id> [--yes]
  diskdrift diff [<old>] [<new>] [--json] [--top <n>]
  diskdrift diff --since <duration> [--json] [--top <n>]
  diskdrift history [<category>] [--json]
  diskdrift watch [--root <path>]... [--debounce <duration>]
                  [--min-change <size>] [--baseline-depth <n>] [--rebaseline]
                  [--run-for <duration>] [--threads <n>] [--json]
  diskdrift events [--since <duration>] [--limit <n>] [--json]
  diskdrift what-happened [--since <duration>] [--from <HH:MM>] [--to <HH:MM>]
                          [--limit <n>] [--json]
  diskdrift explain <category|path> [--json] [--no-progress] [--threads <n>]
  diskdrift doctor [--json]
  diskdrift snapshots [--json]
  diskdrift help [command]
  diskdrift version

Snapshot arguments for `diff` may be snapshot IDs, "latest", or a date/time
prefix such as 2026-09-13 or 2026-09-13T16:40. `--since` accepts durations
such as 30m, 24h, 7d or 2w and compares the newest snapshot with the newest
one at least that old.

Global options:
  --data-dir <path>   Override data directory
                      (default: ~/Library/Application Support/DiskDrift)
  --config <path>     Configuration file
                      (default: ~/.config/diskdrift/config.toml)
  --json              Machine-readable output (schema version 1)
  --no-progress       Disable the live progress display
  --verbose           Show skipped location details
  --root <path>       Scan a specific path instead of the default locations
  --depth <n>         Directory tracking depth (1-8)
  --threads <n>       Walker parallelism (default: auto, capped at 8)

Environment:
  DISKDRIFT_DATA_DIR  Override data directory
  DISKDRIFT_HOME      Override home directory used for scan locations
  DISKDRIFT_CONFIG    Override configuration file path

DiskDrift is read-only. It never deletes files and never uses the network.
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_scan() {
        let cmd = parse(&args(&[
            "scan",
            "--json",
            "--root",
            "/tmp/x",
            "--threads=4",
        ]))
        .unwrap();
        match cmd {
            Command::Scan(a) => {
                assert!(a.common.json);
                assert_eq!(a.roots, vec![PathBuf::from("/tmp/x")]);
                assert_eq!(a.threads, Some(4));
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_diff_positionals() {
        let cmd = parse(&args(&["diff", "1", "2026-09-13", "--top", "5"])).unwrap();
        match cmd {
            Command::Diff(a) => {
                assert_eq!(a.old.as_deref(), Some("1"));
                assert_eq!(a.new.as_deref(), Some("2026-09-13"));
                assert_eq!(a.top, 5);
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn explain_requires_query() {
        assert!(parse(&args(&["explain"])).is_err());
        assert!(matches!(
            parse(&args(&["explain", "xcode"])).unwrap(),
            Command::Explain(_)
        ));
    }

    #[test]
    fn parses_scan_path_and_depth() {
        let cmd = parse(&args(&["scan", "~/Library", "--depth", "3"])).unwrap();
        match cmd {
            Command::Scan(a) => {
                assert_eq!(a.path, Some(PathBuf::from("~/Library")));
                assert_eq!(a.depth, Some(3));
            }
            _ => panic!("wrong command"),
        }
        assert!(parse(&args(&["scan", "/a", "/b"])).is_err());
        assert!(parse(&args(&["scan", "/a", "--root", "/b"])).is_err());
        assert!(parse(&args(&["scan", "--depth", "0"])).is_err());
        assert!(parse(&args(&["scan", "--depth", "9"])).is_err());
    }

    #[test]
    fn parses_top() {
        let cmd = parse(&args(&["top", "/tmp", "--limit", "5", "--depth=2"])).unwrap();
        match cmd {
            Command::Top(a) => {
                assert_eq!(a.path, Some(PathBuf::from("/tmp")));
                assert_eq!(a.limit, 5);
                assert_eq!(a.depth, Some(2));
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_config_flag() {
        let cmd = parse(&args(&["scan", "--config", "/tmp/dd.toml"])).unwrap();
        match cmd {
            Command::Scan(a) => assert_eq!(a.common.config, Some(PathBuf::from("/tmp/dd.toml"))),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_watch() {
        let cmd = parse(&args(&[
            "watch",
            "--debounce",
            "500ms",
            "--min-change",
            "10MB",
            "--run-for",
            "5m",
            "--baseline-depth",
            "4",
            "--rebaseline",
            "--root",
            "/tmp/x",
        ]))
        .unwrap();
        match cmd {
            Command::Watch(a) => {
                assert_eq!(a.debounce.as_deref(), Some("500ms"));
                assert_eq!(a.min_change.as_deref(), Some("10MB"));
                assert_eq!(a.run_for.as_deref(), Some("5m"));
                assert_eq!(a.baseline_depth, Some(4));
                assert!(a.rebaseline);
                assert_eq!(a.roots, vec![PathBuf::from("/tmp/x")]);
            }
            _ => panic!("wrong command"),
        }
        assert!(parse(&args(&["watch", "--debounce", "nope"])).is_err());
        assert!(parse(&args(&["watch", "--min-change", "nope"])).is_err());
    }

    #[test]
    fn parses_events() {
        let cmd = parse(&args(&["events", "--since", "24h", "--limit", "10"])).unwrap();
        match cmd {
            Command::Events(a) => {
                assert_eq!(a.since.as_deref(), Some("24h"));
                assert_eq!(a.limit, 10);
            }
            _ => panic!("wrong command"),
        }
        assert!(parse(&args(&["events", "--since", "yesterday"])).is_err());
    }

    #[test]
    fn parses_what_happened() {
        let cmd = parse(&args(&["what-happened", "--since", "24h", "--limit", "5"])).unwrap();
        match cmd {
            Command::WhatHappened(a) => {
                assert_eq!(a.since.as_deref(), Some("24h"));
                assert_eq!(a.limit, 5);
            }
            _ => panic!("wrong command"),
        }
        let cmd = parse(&args(&[
            "what-happened",
            "--from",
            "14:00",
            "--to",
            "16:00",
        ]))
        .unwrap();
        match cmd {
            Command::WhatHappened(a) => {
                assert_eq!(a.from.as_deref(), Some("14:00"));
                assert_eq!(a.to.as_deref(), Some("16:00"));
            }
            _ => panic!("wrong command"),
        }
        assert!(
            parse(&args(&[
                "what-happened",
                "--since",
                "1h",
                "--from",
                "14:00"
            ]))
            .is_err()
        );
        assert!(parse(&args(&["what-happened", "--from", "25:00"])).is_err());
        assert!(parse(&args(&["what-happened", "--since", "yesterday"])).is_err());
    }

    #[test]
    fn rejects_unknown_option() {
        assert!(parse(&args(&["scan", "--nope"])).is_err());
    }
}
