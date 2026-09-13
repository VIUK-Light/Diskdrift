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
    Command::new(bin())
        .args(args)
        .env("DISKDRIFT_HOME", home)
        .env("DISKDRIFT_DATA_DIR", data)
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
