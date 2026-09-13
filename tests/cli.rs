//! End-to-end CLI tests: run the real binary against temporary trees.

mod common;

use common::{TempDir, allocated_bytes, write_file};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_diskdrift")
}

fn diskdrift(home: &Path, data: &Path, args: &[&str]) -> Output {
    // Keep tests independent from any real user configuration. The config
    // lives next to the data directory, outside the scanned home tree.
    let config = data.with_file_name("test-config.toml");
    if !config.exists() {
        std::fs::create_dir_all(home).ok();
        std::fs::write(&config, "").ok();
    }
    Command::new(bin())
        .args(args)
        .env("DISKDRIFT_HOME", home)
        .env("DISKDRIFT_DATA_DIR", data)
        .env("DISKDRIFT_CONFIG", &config)
        .output()
        .expect("failed to run diskdrift")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON ({}): {}\nstderr: {}",
            e,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn scan_json_is_machine_readable_and_does_not_touch_the_database() {
    let tmp = TempDir::new("cli-scan");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 5_000);
    write_file(&home.join("Library/Caches/Google/Chrome/c.bin"), 1_000);
    let root = home.to_string_lossy().to_string();

    let out = diskdrift(
        &home,
        &data,
        &["scan", "--json", "--no-progress", "--root", &root],
    );
    assert!(
        out.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["version"], 1);
    assert_eq!(v["command"], "scan");
    assert_eq!(v["totals"]["logical_bytes"].as_u64().unwrap(), 6_000);

    let ids: Vec<&str> = v["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"ai.ollama"), "categories: {ids:?}");
    assert!(ids.contains(&"system.caches"), "categories: {ids:?}");
    assert!(
        !data.exists(),
        "`scan` must not create the data directory or database"
    );
}

#[test]
fn snapshot_then_diff_reports_changes() {
    let tmp = TempDir::new("cli-diff");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 5_000);
    let root = home.to_string_lossy().to_string();

    let s1 = diskdrift(
        &home,
        &data,
        &["snapshot", "--no-progress", "--root", &root],
    );
    assert!(
        s1.status.success(),
        "snapshot 1 failed: {}",
        String::from_utf8_lossy(&s1.stderr)
    );

    let added = home.join(".ollama/models/b.bin");
    write_file(&added, 20_000);

    let s2 = diskdrift(
        &home,
        &data,
        &["snapshot", "--json", "--no-progress", "--root", &root],
    );
    assert!(
        s2.status.success(),
        "snapshot 2 failed: {}",
        String::from_utf8_lossy(&s2.stderr)
    );
    let v2 = json(&s2);
    assert_eq!(v2["snapshot"]["id"], 2);

    let d = diskdrift(&home, &data, &["diff", "--json"]);
    assert!(
        d.status.success(),
        "diff failed: {}",
        String::from_utf8_lossy(&d.stderr)
    );
    let vd = json(&d);
    assert_eq!(vd["old"]["id"], 1);
    assert_eq!(vd["new"]["id"], 2);
    assert_eq!(
        vd["total"]["net_change_bytes"].as_i64().unwrap(),
        allocated_bytes(&added) as i64
    );

    let list = diskdrift(&home, &data, &["snapshots", "--json"]);
    assert!(list.status.success());
    assert_eq!(json(&list)["snapshots"].as_array().unwrap().len(), 2);

    // Diff by explicit snapshot ids and by date prefix must work too.
    let d2 = diskdrift(&home, &data, &["diff", "1", "2", "--json"]);
    assert!(d2.status.success());
    let date = v2["snapshot"]["created_at_local"].as_str().unwrap()[..10].to_string();
    let d3 = diskdrift(&home, &data, &["diff", &date, "--json"]);
    assert!(
        d3.status.success(),
        "date diff failed: {}",
        String::from_utf8_lossy(&d3.stderr)
    );
}

#[test]
fn doctor_does_not_create_a_database() {
    let tmp = TempDir::new("cli-doctor");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 1_000);

    let out = diskdrift(&home, &data, &["doctor", "--json"]);
    assert!(
        out.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["version"], 1);
    assert_eq!(v["database"]["schema_version"], 1);
    assert_eq!(v["database"]["exists"], false);
    assert!(!data.join("diskdrift.sqlite3").exists());
}

#[test]
fn explain_path_json() {
    let tmp = TempDir::new("cli-explain");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let file = home.join(".ollama/models/a.bin");
    write_file(&file, 5_000);

    let query = home.join(".ollama/models").to_string_lossy().to_string();
    let out = diskdrift(
        &home,
        &data,
        &["explain", &query, "--json", "--no-progress"],
    );
    assert!(
        out.status.success(),
        "explain failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["resolved"]["kind"], "path");
    assert_eq!(v["total"]["logical_bytes"].as_u64().unwrap(), 5_000);
    assert_eq!(v["deletes_data"], false);
}

#[test]
fn destructive_commands_do_not_exist() {
    let tmp = TempDir::new("cli-destructive");
    let home = tmp.join("home");
    let data = tmp.join("data");
    for cmd in ["clean", "delete", "rm", "purge"] {
        let out = diskdrift(&home, &data, &[cmd]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "`{cmd}` must not be a valid command"
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("unknown command"));
    }
}

#[test]
fn help_and_version() {
    let tmp = TempDir::new("cli-help");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let help = diskdrift(&home, &data, &["help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
    let version = diskdrift(&home, &data, &["version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains("diskdrift"));
}

#[test]
fn top_lists_largest_directories() {
    let tmp = TempDir::new("cli-top");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 50_000);
    write_file(&home.join("Library/Caches/small/x.bin"), 1_000);
    let root = home.to_string_lossy().to_string();

    let out = diskdrift(
        &home,
        &data,
        &[
            "top",
            "--json",
            "--no-progress",
            "--root",
            &root,
            "--limit",
            "5",
        ],
    );
    assert!(
        out.status.success(),
        "top failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["command"], "top");
    let dirs = v["directories"].as_array().unwrap();
    assert!(!dirs.is_empty());
    assert!(dirs.len() <= 5);
    assert!(
        dirs[0]["path"].as_str().unwrap().contains(".ollama"),
        "largest directory should be the ollama model dir: {dirs:?}"
    );

    let human = diskdrift(&home, &data, &["top", "--no-progress", "--root", &root]);
    assert!(human.status.success());
    assert!(String::from_utf8_lossy(&human.stdout).contains("Largest directories"));
}

#[test]
fn scan_path_limits_scope_to_that_path() {
    let tmp = TempDir::new("cli-scan-path");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let project = home.join("project-a");
    write_file(&project.join("big.bin"), 30_000);
    write_file(&home.join("project-b/other.bin"), 10_000);

    let out = diskdrift(
        &home,
        &data,
        &["scan", "--json", "--no-progress", project.to_str().unwrap()],
    );
    assert!(
        out.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["totals"]["logical_bytes"].as_u64().unwrap(), 30_000);
    assert_eq!(v["totals"]["file_count"].as_u64().unwrap(), 1);

    // Missing paths are reported instead of silently scanning nothing.
    let missing = diskdrift(
        &home,
        &data,
        &["scan", "--no-progress", home.join("nope").to_str().unwrap()],
    );
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("cannot access"));
}

#[test]
fn scan_depth_controls_directory_buckets() {
    let tmp = TempDir::new("cli-depth");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let file = home.join("Library/Caches/Google/Chrome/c.bin");
    write_file(&file, 1_000);
    let root = home.to_string_lossy().to_string();

    let shallow = diskdrift(
        &home,
        &data,
        &[
            "scan",
            "--json",
            "--no-progress",
            "--root",
            &root,
            "--depth",
            "3",
        ],
    );
    assert!(shallow.status.success());
    let v = json(&shallow);
    let paths: Vec<String> = v["directories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap().to_string())
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("Library/Caches/Google")),
        "depth 3 should bucket at .../Google: {paths:?}"
    );

    let deep = diskdrift(
        &home,
        &data,
        &[
            "scan",
            "--json",
            "--no-progress",
            "--root",
            &root,
            "--depth",
            "4",
        ],
    );
    assert!(deep.status.success());
    let v = json(&deep);
    let paths: Vec<String> = v["directories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap().to_string())
        .collect();
    assert!(
        paths
            .iter()
            .any(|p| p.ends_with("Library/Caches/Google/Chrome")),
        "depth 4 should bucket at .../Chrome: {paths:?}"
    );
}

#[test]
fn config_exclusions_are_applied() {
    let tmp = TempDir::new("cli-config");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 20_000);
    write_file(&home.join("Library/Caches/Google/c.bin"), 1_000);
    let config = data.with_file_name("test-config.toml");
    std::fs::write(
        &config,
        format!("exclude = [\"{}\"]\n", home.join(".ollama").display()),
    )
    .unwrap();
    let root = home.to_string_lossy().to_string();

    let out = diskdrift(
        &home,
        &data,
        &["scan", "--json", "--no-progress", "--root", &root],
    );
    assert!(
        out.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["totals"]["logical_bytes"].as_u64().unwrap(), 1_000);
    let ids: Vec<&str> = v["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert!(
        !ids.contains(&"ai.ollama"),
        "excluded data must not appear: {ids:?}"
    );

    // A broken config is reported by doctor instead of crashing.
    std::fs::write(&config, "depth = 99\n").unwrap();
    let doctor = diskdrift(&home, &data, &["doctor", "--json"]);
    assert!(doctor.status.success());
    let v = json(&doctor);
    assert!(
        v["config"]["error"].as_str().is_some(),
        "expected config error"
    );
}

#[test]
fn snapshot_subcommands_and_history() {
    let tmp = TempDir::new("cli-history");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 10_000);
    let root = home.to_string_lossy().to_string();

    let s1 = diskdrift(&home, &data, &["snapshot", "--no-progress", "--root", &root]);
    assert!(s1.status.success());
    write_file(&home.join(".ollama/models/b.bin"), 90_000);
    let s2 = diskdrift(&home, &data, &["snapshot", "--no-progress", "--root", &root]);
    assert!(s2.status.success());

    // snapshot list (new spelling) and the legacy `snapshots` command agree.
    let list = diskdrift(&home, &data, &["snapshot", "list", "--json"]);
    assert!(list.status.success(), "{}", String::from_utf8_lossy(&list.stderr));
    assert_eq!(json(&list)["snapshots"].as_array().unwrap().len(), 2);
    let legacy = diskdrift(&home, &data, &["snapshots", "--json"]);
    assert_eq!(json(&legacy)["snapshots"].as_array().unwrap().len(), 2);

    // snapshot show
    let show = diskdrift(&home, &data, &["snapshot", "show", "1", "--json"]);
    assert!(show.status.success(), "{}", String::from_utf8_lossy(&show.stderr));
    let v = json(&show);
    assert_eq!(v["snapshot"]["id"], 1);
    assert!(!v["categories"].as_array().unwrap().is_empty());
    let human = diskdrift(&home, &data, &["snapshot", "show", "1"]);
    assert!(String::from_utf8_lossy(&human.stdout).contains("Snapshot #1"));

    // history: both snapshots are today, so one day with no change yet.
    let hist = diskdrift(&home, &data, &["history", "--json"]);
    assert!(hist.status.success(), "{}", String::from_utf8_lossy(&hist.stderr));
    let v = json(&hist);
    assert_eq!(v["days"].as_array().unwrap().len(), 1);
    assert!(v["days"][0]["change_bytes"].is_null());
    assert_eq!(v["days"][0]["snapshot_count"], 2);

    // category history
    let cat = diskdrift(&home, &data, &["history", "ollama", "--json"]);
    assert!(cat.status.success(), "{}", String::from_utf8_lossy(&cat.stderr));
    let v = json(&cat);
    assert_eq!(v["category"]["id"], "ai.ollama");
    assert_eq!(v["days"][0]["logical_bytes"].as_u64().unwrap(), 100_000);
    assert!(v["days"][0]["allocated_bytes"].as_u64().unwrap() >= 100_000);
    let human = diskdrift(&home, &data, &["history", "ollama"]);
    assert!(String::from_utf8_lossy(&human.stdout).contains("Storage History — Ollama"));

    assert!(!diskdrift(&home, &data, &["history", "nonsense"]).status.success());

    // delete requires confirmation in non-interactive sessions
    let refused = diskdrift(&home, &data, &["snapshot", "delete", "1"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("refusing"));

    let del = diskdrift(&home, &data, &["snapshot", "delete", "1", "--yes", "--json"]);
    assert!(del.status.success(), "{}", String::from_utf8_lossy(&del.stderr));
    assert_eq!(json(&del)["deleted"]["id"], 1);
    let list = diskdrift(&home, &data, &["snapshot", "list", "--json"]);
    assert_eq!(json(&list)["snapshots"].as_array().unwrap().len(), 1);
}

#[test]
fn diff_since_explains_missing_history() {
    let tmp = TempDir::new("cli-since");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 1_000);
    let root = home.to_string_lossy().to_string();
    assert!(diskdrift(&home, &data, &["snapshot", "--no-progress", "--root", &root])
        .status
        .success());

    let out = diskdrift(&home, &data, &["diff", "--since", "7d"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no snapshot is old enough"), "{stderr}");
    assert!(stderr.contains("--since 7d"), "{stderr}");

    assert!(!diskdrift(&home, &data, &["diff", "--since", "7x"]).status.success());
    assert!(!diskdrift(
        &home,
        &data,
        &["diff", "--since", "7d", "1", "2"]
    )
    .status
    .success());
}
