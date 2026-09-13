//! Snapshot persistence and diff calculation tests.

mod common;

use common::{TempDir, allocated_bytes, write_file};
use diskdrift::core::diff;
use diskdrift::core::scan::{self, ScanConfig};
use diskdrift::core::store::Store;
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
    assert_eq!(store.schema_version().unwrap(), 1);

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
