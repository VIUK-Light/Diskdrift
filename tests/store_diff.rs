//! Snapshot persistence and diff calculation tests.

mod common;

use common::{TempDir, allocated_bytes, write_file};
use diskdrift::core::diff;
use diskdrift::core::scan::{self, ScanConfig};
use diskdrift::core::snapshot::EventDraft;
use diskdrift::core::store::Store;
use diskdrift::core::time;
use diskdrift::scanners;
use std::path::Path;

fn scan_home(home: &Path) -> scan::ScanOutput {
    let targets: Vec<_> = scanners::default_targets(home)
        .into_iter()
        .filter(|t| t.path.starts_with(home))
        .collect();
    scan::run(ScanConfig {
        home,
        targets,
        threads: 2,
        progress: None,
        exclusions: Vec::new(),
        depth_override: None,
    })
}

#[test]
fn snapshot_roundtrip_diff_and_resolution() {
    let tmp = TempDir::new("store");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let file = home.join(".ollama/models/a.bin");
    write_file(&file, 5_000);

    let db_path = Store::default_path(&data);
    let mut store = Store::open(&db_path).expect("open store");
    assert_eq!(store.schema_version().unwrap(), 2);

    let out1 = scan_home(&home);
    let m1 = store.insert_snapshot(&out1, "test").unwrap();
    let before = allocated_bytes(&file);

    write_file(&file, 40_000);
    let out2 = scan_home(&home);
    let m2 = store.insert_snapshot(&out2, "test").unwrap();
    let after = allocated_bytes(&file);

    assert!(m2.id > m1.id);
    assert!(m2.total_allocated_bytes > m1.total_allocated_bytes);

    let s1 = diff::load_side(&store, m1.id).unwrap();
    let s2 = diff::load_side(&store, m2.id).unwrap();
    let result = diff::compute(s1, s2, 10);

    assert_eq!(
        result.totals.net_change_bytes,
        m2.total_allocated_bytes as i64 - m1.total_allocated_bytes as i64
    );
    let ollama = result
        .categories
        .iter()
        .find(|c| c.id == "ai.ollama")
        .expect("ollama delta");
    assert_eq!(ollama.net, after as i64 - before as i64);
    assert_eq!(result.totals.added_bytes, after - before);
    assert_eq!(result.totals.removed_bytes, 0);

    // Snapshot resolution: id, latest, date prefix.
    assert_eq!(
        store.resolve_snapshot(&m1.id.to_string()).unwrap().id,
        m1.id
    );
    assert_eq!(store.resolve_snapshot("latest").unwrap().id, m2.id);
    let date = &m2.created_at_local[..10];
    assert_eq!(store.resolve_snapshot(date).unwrap().id, m2.id);
    assert!(store.resolve_snapshot("1999-01-01").is_err());

    // Persistence across reopen.
    drop(store);
    let reopened = Store::open(&db_path).unwrap();
    assert_eq!(reopened.snapshot_count().unwrap(), 2);
    let side = diff::load_side(&reopened, m2.id).unwrap();
    assert_eq!(side.meta.total_allocated_bytes, m2.total_allocated_bytes);
    assert!(!side.directories.is_empty());
}

#[test]
fn removed_data_shows_as_removed() {
    let tmp = TempDir::new("store-remove");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let file = home.join(".ollama/models/a.bin");
    write_file(&file, 20_000);

    let mut store = Store::open(&Store::default_path(&data)).unwrap();
    let m1 = store.insert_snapshot(&scan_home(&home), "test").unwrap();
    let before = allocated_bytes(&file);

    std::fs::remove_file(&file).unwrap();
    let m2 = store.insert_snapshot(&scan_home(&home), "test").unwrap();

    let result = diff::compute(
        diff::load_side(&store, m1.id).unwrap(),
        diff::load_side(&store, m2.id).unwrap(),
        10,
    );
    assert!(result.totals.net_change_bytes < 0);
    assert_eq!(result.totals.removed_bytes, before);
    assert_eq!(result.totals.added_bytes, 0);
}

#[test]
fn snapshot_stores_no_file_contents() {
    let tmp = TempDir::new("store-privacy");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let file = home.join(".ollama/models/secret.bin");
    let secret: Vec<u8> = b"DISKDRIFT_SECRET_CONTENT_DO_NOT_STORE".repeat(50);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&file, &secret).unwrap();

    let mut store = Store::open(&Store::default_path(&data)).unwrap();
    store.insert_snapshot(&scan_home(&home), "test").unwrap();
    drop(store);

    let bytes = std::fs::read(Store::default_path(&data)).unwrap();
    // The file's path is metadata and may be stored, but its contents
    // must never end up in the database.
    assert!(
        !bytes.windows(secret.len()).any(|w| w == secret),
        "database must not contain file contents"
    );
}

#[test]
fn backdated_snapshots_support_history_and_since() {
    let tmp = TempDir::new("store-history");
    let home = tmp.join("home");
    let data = tmp.join("data");
    let file = home.join(".ollama/models/a.bin");
    write_file(&file, 5_000);

    let mut store = Store::open(&Store::default_path(&data)).unwrap();
    let now = time::now_unix();

    let mut out1 = scan_home(&home);
    out1.started_at = now - 8 * 86_400;
    let m1 = store.insert_snapshot(&out1, "test").unwrap();

    write_file(&file, 20_000);
    let mut out2 = scan_home(&home);
    out2.started_at = now - 6 * 86_400;
    let m2 = store.insert_snapshot(&out2, "test").unwrap();

    write_file(&file, 30_000);
    let mut out3 = scan_home(&home);
    out3.started_at = now - 3_600;
    let m3 = store.insert_snapshot(&out3, "test").unwrap();

    let all = store.all_snapshots().unwrap();
    assert_eq!(all.len(), 3);
    assert!(all[0].unix_time() < all[1].unix_time());
    assert!(all[1].unix_time() < all[2].unix_time());

    // 7 days ago: m1 is old enough, m2 is not.
    let (old, new) = store.resolve_since(7 * 86_400).unwrap();
    assert_eq!(old.id, m1.id);
    assert_eq!(new.id, m3.id);

    // 24 hours: m2 is the newest snapshot older than the cutoff.
    let (old, new) = store.resolve_since(86_400).unwrap();
    assert_eq!(old.id, m2.id);
    assert_eq!(new.id, m3.id);

    // Deleting a snapshot removes its entries (metadata only).
    assert!(store.delete_snapshot(m2.id).unwrap());
    assert_eq!(store.all_snapshots().unwrap().len(), 2);
    assert!(store.snapshot_by_id(m2.id).unwrap().is_none());
    assert!(store.load_categories(m2.id).unwrap().is_empty());
    assert!(store.load_directories(m2.id).unwrap().is_empty());
}

#[test]
fn events_and_watch_dirs_round_trip() {
    let tmp = TempDir::new("store-events");
    let data = tmp.join("data");
    let mut store = Store::open(&Store::default_path(&data)).unwrap();
    assert_eq!(store.schema_version().unwrap(), 2);

    let drafts = vec![
        EventDraft {
            timestamp_unix: 1_000,
            kind: "grow",
            path: tmp.join("a"),
            category_id: "ai.ollama".to_string(),
            delta_bytes: 12_345,
            allocated_bytes: 99_999,
            file_count: 3,
            directory_count: 1,
        },
        EventDraft {
            timestamp_unix: 2_000,
            kind: "shrink",
            path: tmp.join("b"),
            category_id: "system.caches".to_string(),
            delta_bytes: -500,
            allocated_bytes: 1_000,
            file_count: 2,
            directory_count: 0,
        },
    ];
    store.insert_events(&drafts).unwrap();

    let rows = store.recent_events(None, 10).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].timestamp_unix, 1_000);
    assert_eq!(rows[1].delta_bytes, -500);
    assert_eq!(rows[1].category_id, "system.caches");

    let filtered = store.recent_events(Some(1_500), 10).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].timestamp_unix, 2_000);

    // Newest N, returned oldest-first.
    let limited = store.recent_events(None, 1).unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].timestamp_unix, 2_000);

    // watch_dirs upsert and delete
    let row = (tmp.join("dir"), "system.caches".to_string(), 111u64);
    store.upsert_watch_dirs(std::slice::from_ref(&row)).unwrap();
    store
        .upsert_watch_dirs(&[(tmp.join("dir"), "system.caches".to_string(), 222)])
        .unwrap();
    let dirs = store.load_watch_dirs().unwrap();
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].2, 222);
    store.delete_watch_dirs(&[tmp.join("dir")]).unwrap();
    assert!(store.load_watch_dirs().unwrap().is_empty());
}
