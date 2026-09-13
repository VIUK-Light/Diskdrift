//! Walker + classification integration tests using temporary directories.

mod common;

use common::{TempDir, allocated_bytes, logical_bytes, write_file};
use diskdrift::core::categories;
use diskdrift::core::fs::{self, SkipKind};
use diskdrift::core::scan::{self, ScanConfig};
use diskdrift::scanners;
use std::path::Path;

/// Home-relative targets only: tests must never touch the real machine's
/// `/opt/homebrew` or `/Library` data.
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

fn rolled(out: &scan::ScanOutput, id: &str) -> fs::Accum {
    let rolled = fs::rollup_categories(&out.walk.categories);
    rolled[categories::index_of(id).expect("known category")]
}

#[test]
fn classifies_known_locations() {
    let tmp = TempDir::new("walker-classify");
    let home = &tmp.path;

    let ollama = home.join(".ollama/models/a.bin");
    let derived = home.join("Library/Developer/Xcode/DerivedData/b.bin");
    let chrome = home.join("Library/Caches/Google/Chrome/c.bin");
    write_file(&ollama, 5_000);
    write_file(&derived, 3_000);
    write_file(&chrome, 1_000);

    let out = scan_home(home);

    assert_eq!(
        rolled(&out, "ai.ollama").allocated,
        allocated_bytes(&ollama)
    );
    assert_eq!(
        rolled(&out, "developer.xcode.derived_data").allocated,
        allocated_bytes(&derived)
    );
    assert_eq!(
        rolled(&out, "system.caches").allocated,
        allocated_bytes(&chrome)
    );
    assert_eq!(
        out.walk.totals.logical,
        logical_bytes(&ollama) + logical_bytes(&derived) + logical_bytes(&chrome)
    );
}

#[test]
fn tracked_directories_are_bucketed_at_configured_depth() {
    let tmp = TempDir::new("walker-buckets");
    let home = &tmp.path;
    let chrome = home.join("Library/Caches/Google/Chrome/c.bin");
    write_file(&chrome, 1_000);
    let out = scan_home(home);

    let key = home.join("Library/Caches/Google/Chrome");
    let entry = out
        .walk
        .directories
        .get(&key)
        .expect("tracked directory at depth 2");
    assert_eq!(entry.acc.allocated, allocated_bytes(&chrome));
}

#[test]
fn hard_links_are_counted_once() {
    let tmp = TempDir::new("walker-hardlink");
    let home = &tmp.path;
    let a = home.join("Library/Caches/a.bin");
    write_file(&a, 4_000);
    std::fs::hard_link(&a, home.join("Library/Caches/b-hardlink.bin")).expect("hard link");

    let out = scan_home(home);
    assert_eq!(out.walk.totals.hardlinks_deduped, 1);
    // File entries are counted, but bytes only once.
    assert_eq!(
        out.walk.totals.logical,
        logical_bytes(&a),
        "hard-linked bytes must only be counted once"
    );
}

#[test]
fn symlinks_are_never_followed() {
    let tmp = TempDir::new("walker-symlink");
    let home = &tmp.path;
    // Big directory outside of every scan target.
    let outside = home.join("outside/big.bin");
    write_file(&outside, 2_000_000);
    std::fs::create_dir_all(home.join("Library/Caches")).expect("mkdir");
    std::os::unix::fs::symlink(
        home.join("outside"),
        home.join("Library/Caches/outside-link"),
    )
    .expect("symlink");

    let out = scan_home(home);
    assert!(out.walk.totals.symlinks >= 1);
    assert!(
        out.walk.totals.allocated < 1_000_000,
        "data behind a symlink must not be counted"
    );
}

#[test]
fn permission_errors_do_not_fail_the_scan() {
    use std::os::unix::fs::PermissionsExt;
    let euid = unsafe { libc::geteuid() };
    if euid == 0 {
        eprintln!("running as root; permission test skipped");
        return;
    }

    let tmp = TempDir::new("walker-perm");
    let home = &tmp.path;
    let secret_dir = home.join("Library/Caches/secret");
    write_file(&secret_dir.join("hidden.bin"), 100);
    std::fs::set_permissions(&secret_dir, std::fs::Permissions::from_mode(0o000))
        .expect("chmod 000");

    let out = scan_home(home);

    // Restore so TempDir::drop can clean up.
    std::fs::set_permissions(&secret_dir, std::fs::Permissions::from_mode(0o755))
        .expect("chmod 755");

    assert!(out.walk.skipped_count >= 1);
    assert!(
        out.walk
            .skipped
            .iter()
            .any(|s| s.kind == SkipKind::Permission),
        "expected at least one permission skip"
    );
}

#[test]
fn excluded_paths_are_not_counted_or_reported() {
    let tmp = TempDir::new("walker-exclude");
    let home = &tmp.path;
    write_file(&home.join("Library/Caches/keep.bin"), 1_000);
    write_file(&home.join("Library/Caches/skipme/data.bin"), 5_000);
    let excluded = home.join("Library/Caches/skipme");

    let targets: Vec<_> = scanners::default_targets(home)
        .into_iter()
        .filter(|t| t.path.starts_with(home))
        .collect();
    let out = scan::run(ScanConfig {
        home,
        targets,
        threads: 1,
        progress: None,
        exclusions: vec![excluded],
        depth_override: None,
    });

    let rolled = fs::rollup_categories(&out.walk.categories);
    let caches = rolled[categories::index_of("system.caches").unwrap()];
    assert_eq!(
        caches.allocated,
        allocated_bytes(&home.join("Library/Caches/keep.bin"))
    );
    assert_eq!(out.walk.skipped_count, 0, "exclusions are silent");
}
