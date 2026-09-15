//! End-to-end CLI tests: run the real binary against temporary trees.

mod common;

use common::{TempDir, allocated_bytes, write_file};
use diskdrift::core::snapshot::EventDraft;
use diskdrift::core::store::Store;
use diskdrift::core::time;
use serde_json::Value;
use std::path::{Path, PathBuf};
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

    let list = diskdrift(&home, &data, &["snapshot", "list", "--json"]);
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
    assert_eq!(v["database"]["schema_version"], 2);
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

    let s1 = diskdrift(
        &home,
        &data,
        &["snapshot", "--no-progress", "--root", &root],
    );
    assert!(s1.status.success());
    write_file(&home.join(".ollama/models/b.bin"), 90_000);
    let s2 = diskdrift(
        &home,
        &data,
        &["snapshot", "--no-progress", "--root", &root],
    );
    assert!(s2.status.success());

    // snapshot list (new spelling) and the legacy `snapshots` command agree.
    let list = diskdrift(&home, &data, &["snapshot", "list", "--json"]);
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert_eq!(json(&list)["snapshots"].as_array().unwrap().len(), 2);
    // Top-level `snapshots` now lists macOS local snapshots; DiskDrift's own
    // snapshots are under `snapshot list`.
    let mac = diskdrift(&home, &data, &["snapshots", "--json"]);
    assert_eq!(json(&mac)["command"], "snapshots");

    // snapshot show
    let show = diskdrift(&home, &data, &["snapshot", "show", "1", "--json"]);
    assert!(
        show.status.success(),
        "{}",
        String::from_utf8_lossy(&show.stderr)
    );
    let v = json(&show);
    assert_eq!(v["snapshot"]["id"], 1);
    assert!(!v["categories"].as_array().unwrap().is_empty());
    let human = diskdrift(&home, &data, &["snapshot", "show", "1"]);
    assert!(String::from_utf8_lossy(&human.stdout).contains("Snapshot #1"));

    // history: both snapshots are today, so one day with no change yet.
    let hist = diskdrift(&home, &data, &["history", "--json"]);
    assert!(
        hist.status.success(),
        "{}",
        String::from_utf8_lossy(&hist.stderr)
    );
    let v = json(&hist);
    assert_eq!(v["days"].as_array().unwrap().len(), 1);
    assert!(v["days"][0]["change_bytes"].is_null());
    assert_eq!(v["days"][0]["snapshot_count"], 2);

    // category history
    let cat = diskdrift(&home, &data, &["history", "ollama", "--json"]);
    assert!(
        cat.status.success(),
        "{}",
        String::from_utf8_lossy(&cat.stderr)
    );
    let v = json(&cat);
    assert_eq!(v["category"]["id"], "ai.ollama");
    assert_eq!(v["days"][0]["logical_bytes"].as_u64().unwrap(), 100_000);
    assert!(v["days"][0]["allocated_bytes"].as_u64().unwrap() >= 100_000);
    let human = diskdrift(&home, &data, &["history", "ollama"]);
    assert!(String::from_utf8_lossy(&human.stdout).contains("Storage History — Ollama"));

    assert!(
        !diskdrift(&home, &data, &["history", "nonsense"])
            .status
            .success()
    );

    // delete requires confirmation in non-interactive sessions
    let refused = diskdrift(&home, &data, &["snapshot", "delete", "1"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("refusing"));

    let del = diskdrift(
        &home,
        &data,
        &["snapshot", "delete", "1", "--yes", "--json"],
    );
    assert!(
        del.status.success(),
        "{}",
        String::from_utf8_lossy(&del.stderr)
    );
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
    assert!(
        diskdrift(
            &home,
            &data,
            &["snapshot", "--no-progress", "--root", &root]
        )
        .status
        .success()
    );

    let out = diskdrift(&home, &data, &["diff", "--since", "7d"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no snapshot is old enough"), "{stderr}");
    assert!(stderr.contains("--since 7d"), "{stderr}");

    assert!(
        !diskdrift(&home, &data, &["diff", "--since", "7x"])
            .status
            .success()
    );
    assert!(
        !diskdrift(&home, &data, &["diff", "--since", "7d", "1", "2"])
            .status
            .success()
    );
}

#[test]
fn events_json_reads_recorded_events() {
    let tmp = TempDir::new("cli-events");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 1_000);

    let now = time::now_unix();
    {
        let mut store = Store::open(&Store::default_path(&data)).unwrap();
        store
            .insert_events(&[
                EventDraft {
                    timestamp_unix: now - 60,
                    kind: "grow",
                    path: home.join(".ollama"),
                    category_id: "ai.ollama".to_string(),
                    delta_bytes: 2_000_000,
                    allocated_bytes: 5_000_000,
                    file_count: 10,
                    directory_count: 2,
                },
                EventDraft {
                    timestamp_unix: now - 30,
                    kind: "shrink",
                    path: PathBuf::from("/tmp/other"),
                    category_id: "system.caches".to_string(),
                    delta_bytes: -1_000,
                    allocated_bytes: 0,
                    file_count: 0,
                    directory_count: 0,
                },
            ])
            .unwrap();
    }

    let out = diskdrift(&home, &data, &["events", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["command"], "events");
    let events = v["events"].as_array().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["delta_bytes"].as_i64().unwrap(), 2_000_000);
    assert!(events[0]["path"].as_str().unwrap().contains(".ollama"));

    let human = diskdrift(&home, &data, &["events"]);
    assert!(human.status.success());
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("Storage events"), "{text}");
    assert!(text.contains("+2.0 MB"), "{text}");

    // --since filters by event time.
    let recent = diskdrift(&home, &data, &["events", "--since", "1h", "--json"]);
    assert_eq!(json(&recent)["events"].as_array().unwrap().len(), 2);
    let none = diskdrift(&home, &data, &["events", "--since", "1s", "--json"]);
    assert_eq!(json(&none)["events"].as_array().unwrap().len(), 0);
}

#[test]
fn what_happened_groups_events_into_incidents() {
    let tmp = TempDir::new("cli-what");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let now = time::now_unix();
    {
        let mut store = Store::open(&Store::default_path(&data)).unwrap();
        store
            .insert_events(&[
                EventDraft {
                    timestamp_unix: now - 3_600,
                    kind: "grow",
                    path: home.join("Library/Developer/CoreSimulator"),
                    category_id: "developer.xcode.core_simulator".to_string(),
                    delta_bytes: 1_500_000,
                    allocated_bytes: 2_000_000,
                    file_count: 10,
                    directory_count: 2,
                },
                EventDraft {
                    timestamp_unix: now - 1_800,
                    kind: "grow",
                    path: home.join("Library/Developer/CoreSimulator"),
                    category_id: "developer.xcode.core_simulator".to_string(),
                    delta_bytes: 500_000,
                    allocated_bytes: 2_500_000,
                    file_count: 12,
                    directory_count: 3,
                },
                EventDraft {
                    timestamp_unix: now - 900,
                    kind: "shrink",
                    path: home.join("Library/Caches/a"),
                    category_id: "system.caches".to_string(),
                    delta_bytes: -200_000,
                    allocated_bytes: 100_000,
                    file_count: 1,
                    directory_count: 0,
                },
            ])
            .unwrap();
    }

    let out = diskdrift(&home, &data, &["what-happened", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["command"], "what-happened");
    assert_eq!(v["total"]["delta_bytes"].as_i64().unwrap(), 1_800_000);
    assert_eq!(v["total"]["event_count"], 3);
    let incidents = v["incidents"].as_array().unwrap();
    assert_eq!(incidents.len(), 2);
    assert_eq!(incidents[0]["label"], "Xcode / CoreSimulator");
    assert_eq!(incidents[0]["delta_bytes"].as_i64().unwrap(), 2_000_000);
    assert_eq!(incidents[0]["event_count"], 2);
    assert_eq!(incidents[1]["category_id"], "system.caches");
    assert_eq!(incidents[1]["delta_bytes"].as_i64().unwrap(), -200_000);

    let human = diskdrift(&home, &data, &["what-happened"]);
    assert!(human.status.success());
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("What happened in the last 24 hours?"),
        "{text}"
    );
    assert!(text.contains("Disk usage increased by 1.8 MB"), "{text}");
    assert!(text.contains("1. Xcode / CoreSimulator"), "{text}");
    assert!(text.contains("2. Caches"), "{text}");

    // --since 30m drops the oldest event.
    let recent = diskdrift(&home, &data, &["what-happened", "--since", "30m", "--json"]);
    assert_eq!(json(&recent)["total"]["event_count"], 2);

    // --limit caps the incident list.
    let limited = diskdrift(&home, &data, &["what-happened", "--limit", "1", "--json"]);
    assert_eq!(json(&limited)["incidents"].as_array().unwrap().len(), 1);

    // --from/--to within the same local day covers the events.
    let (_, _, _, hour_from, minute_from, _) = time::local_parts(now - 7_200);
    let (_, _, _, hour_to, minute_to, _) = time::local_parts(now + 60);
    let today = time::format_local(now)[..10].to_string();
    let same_day = time::format_local(now - 7_200)[..10] == today;
    if same_day {
        let from = format!("{hour_from:02}:{minute_from:02}");
        let to = format!("{hour_to:02}:{minute_to:02}");
        let window = diskdrift(
            &home,
            &data,
            &["what-happened", "--from", &from, "--to", &to, "--json"],
        );
        assert!(
            window.status.success(),
            "{}",
            String::from_utf8_lossy(&window.stderr)
        );
        assert_eq!(json(&window)["total"]["event_count"], 3);
    }

    // Empty period is explained instead of failing.
    let empty = diskdrift(&home, &data, &["what-happened", "--since", "1s", "--json"]);
    assert!(empty.status.success());
    assert_eq!(json(&empty)["total"]["event_count"], 0);
    let human = diskdrift(&home, &data, &["what-happened", "--since", "1s"]);
    assert!(String::from_utf8_lossy(&human.stdout).contains("No watch events"));
}

#[test]
fn system_volumes_and_macos_snapshots() {
    let tmp = TempDir::new("cli-system");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 1_000);

    let volumes = diskdrift(&home, &data, &["volumes", "--json"]);
    assert!(
        volumes.status.success(),
        "{}",
        String::from_utf8_lossy(&volumes.stderr)
    );
    let v = json(&volumes);
    assert_eq!(v["command"], "volumes");
    assert!(
        v["volumes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|volume| volume["mount_point"] == "/")
    );

    let system = diskdrift(&home, &data, &["system", "--json"]);
    assert!(
        system.status.success(),
        "{}",
        String::from_utf8_lossy(&system.stderr)
    );
    let v = json(&system);
    assert_eq!(v["command"], "system");
    assert!(!v["volumes"].as_array().unwrap().is_empty());
    assert!(v["snapshot_count"].as_u64().is_some());
    assert!(!v["notes"].as_array().unwrap().is_empty());
    if let Some(vm) = v["vm"].as_object() {
        assert_eq!(vm["label"], "macOS managed");
    }

    let snapshots = diskdrift(&home, &data, &["snapshots", "--json"]);
    assert!(snapshots.status.success());
    assert_eq!(json(&snapshots)["command"], "snapshots");

    let human = diskdrift(&home, &data, &["system"]);
    assert!(String::from_utf8_lossy(&human.stdout).contains("macOS System Storage"));

    // `snapshot list` still lists DiskDrift's own snapshots.
    assert!(
        diskdrift(
            &home,
            &data,
            &[
                "snapshot",
                "--no-progress",
                "--root",
                home.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    let list = diskdrift(&home, &data, &["snapshot", "list", "--json"]);
    assert_eq!(json(&list)["snapshots"].as_array().unwrap().len(), 1);
}

#[test]
fn large_duplicates_and_recommendations() {
    let tmp = TempDir::new("cli-tools");
    let home = tmp.join("home");
    let data = tmp.join("data");
    write_file(&home.join(".ollama/models/a.bin"), 3_000_000);
    write_file(&home.join("Downloads/a-copy.bin"), 3_000_000);
    write_file(&home.join("Downloads/other.bin"), 2_000_000);
    let root = home.to_string_lossy().to_string();

    let large = diskdrift(
        &home,
        &data,
        &[
            "large",
            "--json",
            "--root",
            &root,
            "--min-size",
            "1MB",
            "--limit",
            "5",
        ],
    );
    assert!(
        large.status.success(),
        "{}",
        String::from_utf8_lossy(&large.stderr)
    );
    let v = json(&large);
    assert_eq!(v["command"], "large");
    let files = v["files"].as_array().unwrap();
    assert!(!files.is_empty());
    assert!(files[0]["allocated_bytes"].as_u64().unwrap() >= 2_000_000);
    assert!(
        files[0]["allocated_bytes"].as_u64().unwrap()
            >= files[files.len() - 1]["allocated_bytes"].as_u64().unwrap()
    );

    let duplicates = diskdrift(
        &home,
        &data,
        &["duplicates", "--json", "--root", &root, "--min-size", "1MB"],
    );
    assert!(
        duplicates.status.success(),
        "{}",
        String::from_utf8_lossy(&duplicates.stderr)
    );
    let v = json(&duplicates);
    assert_eq!(v["command"], "duplicates");
    let groups = v["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "{v}");
    assert_eq!(groups[0]["files"].as_array().unwrap().len(), 2);
    assert!(v["reclaimable_bytes"].as_u64().unwrap() >= 3_000_000);
    let human = diskdrift(
        &home,
        &data,
        &["duplicates", "--root", &root, "--min-size", "1MB"],
    );
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("Duplicate files"), "{text}");
    assert!(text.contains("Potential duplicate"), "{text}");

    let recommendations = diskdrift(
        &home,
        &data,
        &[
            "recommendations",
            "--json",
            "--root",
            &root,
            "--threads",
            "2",
        ],
    );
    assert!(
        recommendations.status.success(),
        "{}",
        String::from_utf8_lossy(&recommendations.stderr)
    );
    let v = json(&recommendations);
    assert_eq!(v["command"], "recommendations");
    assert!(
        v["insights"]
            .as_array()
            .unwrap()
            .iter()
            .any(|insight| insight["category_id"] == "ai.ollama")
    );
    let human = diskdrift(&home, &data, &["recommendations", "--root", &root]);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("Storage Insights"), "{text}");
    assert!(text.contains("never deletes files"), "{text}");
}
