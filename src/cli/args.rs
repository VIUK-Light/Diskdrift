//! Hand-rolled argument parsing (no clap): the CLI surface is small and
//! stable, and this keeps the binary dependency-light.

use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct CommonArgs {
    pub json: bool,
    pub no_progress: bool,
    pub verbose: bool,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ScanArgs {
    pub common: CommonArgs,
    pub roots: Vec<PathBuf>,
    pub threads: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct DiffArgs {
    pub common: CommonArgs,
    pub old: Option<String>,
    pub new: Option<String>,
    pub top: usize,
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
    Snapshot(ScanArgs),
    Diff(DiffArgs),
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
        "scan" | "snapshot" => match parse_scan_args(rest) {
            Ok(args) => {
                if cmd == "scan" {
                    Ok(Command::Scan(args))
                } else {
                    Ok(Command::Snapshot(args))
                }
            }
            Err(e) if e == "HELP" => Ok(Command::Help(Some(cmd.to_string()))),
            Err(e) => Err(e),
        },
        "diff" => parse_diff(rest),
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
        _ => return Ok(false),
    }
    Ok(true)
}

fn parse_scan_args(rest: &[String]) -> Result<ScanArgs, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut roots = Vec::new();
    let mut threads = None;
    while let Some((name, inline)) = args.next() {
        if parse_common(&mut args, &name, inline.clone(), &mut common)? {
            continue;
        }
        match name.as_str() {
            "--root" => roots.push(PathBuf::from(args.value("--root", inline)?)),
            "--threads" => {
                let v = args.value("--threads", inline)?;
                threads = Some(
                    v.parse::<usize>()
                        .map_err(|_| format!("--threads expects a number, got '{v}'"))?,
                );
            }
            "-h" | "--help" => return Err("HELP".into()),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    if threads == Some(0) {
        return Err("--threads must be at least 1".into());
    }
    Ok(ScanArgs {
        common,
        roots,
        threads,
    })
}

fn parse_diff(rest: &[String]) -> Result<Command, String> {
    let mut args = Args::new(rest);
    let mut common = CommonArgs::default();
    let mut positional: Vec<String> = Vec::new();
    let mut top = 20usize;
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
    Ok(Command::Diff(DiffArgs {
        common,
        old,
        new,
        top: top.max(1),
    }))
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
  diskdrift scan [--json] [--no-progress] [--verbose] [--root <path>]... [--threads <n>]
  diskdrift snapshot [--json] [--no-progress] [--root <path>]... [--threads <n>]
  diskdrift diff [<old>] [<new>] [--json] [--top <n>]
  diskdrift explain <category|path> [--json] [--no-progress] [--threads <n>]
  diskdrift doctor [--json]
  diskdrift snapshots [--json]
  diskdrift help [command]
  diskdrift version

Snapshot arguments for `diff` may be snapshot IDs, "latest", or a date/time
prefix such as 2026-09-13 or 2026-09-13T16:40.

Global options:
  --data-dir <path>   Override data directory
                      (default: ~/Library/Application Support/DiskDrift)
  --json              Machine-readable output (schema version 1)
  --no-progress       Disable the live progress display
  --verbose           Show skipped location details

Environment:
  DISKDRIFT_DATA_DIR  Override data directory
  DISKDRIFT_HOME      Override home directory used for scan locations

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
    fn rejects_unknown_option() {
        assert!(parse(&args(&["scan", "--nope"])).is_err());
    }
}
