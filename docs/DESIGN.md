# DiskDrift v0.1 — Design

This document records the architecture and the technical decisions that shape
DiskDrift v0.1. The priority order is:

1. **Trustworthiness of scan results** over feature count.
2. **Safety** — DiskDrift must never be able to destroy data.
3. **Explainability** — "where did the space go?" must have a readable answer.

---

## 1. Language choice

Rust was chosen (Swift and Go were the alternatives) for these reasons:

| Requirement | Rust |
| --- | --- |
| Accurate filesystem metadata | `std::os::unix::fs::MetadataExt` exposes `dev`, `ino`, `nlink`, `size`, `blocks` directly; no bridging layer. |
| Hundreds of thousands of files | Zero-cost abstractions, no GC pauses, deterministic memory. |
| Single-binary distribution | `cargo build --release` produces one native executable with no runtime dependency. |
| Low memory | Streaming aggregation only; no per-file allocations kept alive. |
| Parallel scanning | `std::thread::scope` + a bounded work queue; measured 43 s → 7.6 s from 1 to 8 threads on a 496k-file scan. |
| Future macOS APIs (FSEvents, `getattrlistbulk`) | Available through `libc`/`objc2` with thin FFI, no rewrite. |

Swift would also satisfy these, but Rust was the project's first choice and
gives the cleanest path to a portable, dependency-light release binary. Go was
rejected mainly because of GC pressure and larger binaries for a short-lived,
metadata-heavy scan.

---

## 2. Module layout

```text
src/
  core/
    fs.rs          streaming parallel walker + aggregation
    classify.rs    path → category rules (data-driven)
    categories.rs  the category tree (data)
    scan.rs        scan orchestration, target preparation
    snapshot.rs    snapshot data model (CatVal, SnapshotMeta, rows)
    store.rs       SQLite persistence + migrations
    diff.rs        snapshot comparison (category + directory level)
    explain.rs     focused live analysis of one category/path
    doctor.rs      environment and permission diagnostics
    size.rs        decimal byte formatting (matches macOS)
    time.rs        timestamp formatting/parsing (no chrono)
    json.rs        versioned JSON output structs (`version: 1`)
    error.rs       small error type (no anyhow)
    interrupt.rs   SIGINT flag + SIGPIPE reset
  scanners/
    xcode.rs, homebrew.rs, docker.rs, ollama.rs, huggingface.rs,
    lmstudio.rs, caches.rs, app_support.rs, containers.rs, logs.rs,
    npm.rs, pnpm.rs, generic.rs
  cli/
    args.rs        hand-rolled parser (no clap)
    commands.rs    command dispatch
    render.rs      human-readable output
    progress.rs    TTY-only live progress
```

Dependency policy: `rusqlite` (bundled SQLite), `serde`/`serde_json`, `libc`.
Everything else is standard library. The CLI parser, time formatting and JSON
models are intentionally local code to keep the binary lean and the schema
explicit.

Scanner modules declare *where* tool data lives:

```rust
ScanTarget { path, category_hint, tracked_depth, scanner }
```

Classification rules are separate. Adding support for a new tool means adding a
scanner module and classification rules — the walker, store, diff and CLI do
not change.

---

## 3. Scanning algorithm

### 3.1 Work queue

- Every scan target root is a seed `WorkItem`.
- A fixed pool of worker threads (`available_parallelism`, capped at 8) pops
  directories from a shared queue, lists them, pushes subdirectories and
  aggregates files.
- Bounded parallelism avoids thrashing the disk; the user can override with
  `--threads`.
- The queue tracks `queued` and `in_flight` counts. Termination is when both
  reach zero; waiting workers are notified via `Condvar`. Poisoned mutexes are
  recovered (`unwrap_or_else(|e| e.into_inner())`) so one failing worker cannot
  deadlock the rest.

### 3.2 Streaming aggregation

Per-directory work produces only aggregates:

- per category (index-aligned with the compile-time category table),
- per tracked directory bucket (bounded path depth),
- per scan target,
- per worker, merged once at the end.

No file list is retained. Memory is proportional to the number of categories
and tracked buckets, not to the number of files.

### 3.3 Bucket depth

A file is attributed to the prefix of its parent directory of at most
`tracked_depth` components below the scan root. This produces disjoint buckets
(no parent/child double counting) while keeping the directory diff readable
(e.g. `~/Library/Caches/Google/Chrome`).

### 3.4 Progress

A separate thread samples shared atomic counters every 150 ms and rewrites a
small block on stderr using ANSI cursor movement. The block is cleared on
completion. Progress is disabled when stderr is not a TTY or when `--json` is
in use.

### 3.5 Interruption

`SIGINT` sets an atomic flag (async-signal-safe). Workers check it before each
directory and every 512 entries inside a directory. On abort the queue is
cleared and every in-flight item is released. `scan` exits 130 with partial
results; `snapshot` discards the scan and stores nothing.

---

## 4. Size accounting

- **logical_bytes** = `st_size` (bytes the file logically contains).
- **allocated_bytes** = `st_blocks * 512` (bytes actually allocated on disk).
- Terminal output prefers allocated bytes; JSON exposes both.
- macOS decimal units (1 GB = 10^9 B) are used to stay comparable with the
  Storage pane.

Known differences from Apple's "System Data":

- **APFS clones** share extents; both clones report the same allocated size.
- **APFS snapshots** retain blocks that are not visible as files.
- **Purgeable space** is reclaimable but allocated.
- Some Apple-internal data is not accessible from user space.

DiskDrift therefore documents that its numbers are *observed*, not
authoritative, and never pretends to reproduce Apple's classification.

---

## 5. Symlinks and hard links

- Symlinks are **never followed**. They are counted (for transparency,
  `symlink_count`) but contribute no bytes. This prevents cycles, double
  counting and scanning outside the intended roots.
- Hard links are de-duplicated by `(st_dev, st_ino)` **only when
  `st_nlink > 1`**; the common case takes no lock. The first link encountered
  carries the bytes, later links increment file counts but add no bytes.
  `hardlink_deduplicated_count` is reported.

---

## 6. Permissions and error handling

Everything below is a warning, never a fatal error:

- permission denied (`EPERM`/`EACCES`),
- broken symlinks,
- files deleted mid-scan,
- directories that disappear,
- unreadable metadata.

Skipped locations are counted and up to 1000 examples are retained per scan
(500 stored per snapshot). `scan` prints a summary; `doctor` lists details and
probes TCC-protected paths (`~/Library/Mail`, `~/Library/Messages`, …) to tell
the user whether Full Disk Access would improve coverage. No root privileges
are required, and DiskDrift never asks for them.

---

## 7. Snapshot storage

SQLite, at `~/Library/Application Support/DiskDrift/diskdrift.sqlite3`, with
`PRAGMA user_version` used as the schema version (currently 2).

```sql
metadata(key, value)
snapshots(id, created_at, created_at_local, total_logical_bytes,
          total_allocated_bytes, file_count, directory_count,
          symlink_count, skipped_count, duration_ms, app_version)
categories(snapshot_id, category_id, logical_bytes, allocated_bytes,
           file_count, directory_count)
scan_entries(snapshot_id, kind, path, category_id, logical_bytes,
             allocated_bytes, file_count, directory_count)
skipped_locations(snapshot_id, path, reason, kind)
```

- `snapshots.created_at` is UTC RFC 3339 (sortable); `created_at_local` is for
  display and date-prefix lookup.
- `scan_entries.kind` is `directory` or `root`.
- v2 adds `events` (watch change log) and `watch_dirs` (last measured sizes
  for incremental monitoring).
- Inserts are one transaction; snapshots are never partially written.
- Migrations run on open: version 0 creates v1, newer versions are rejected
  with a clear message.
- Only metadata is stored: no file contents, no hashes of contents.

`diff` resolves snapshots by numeric ID, `latest`, or a date/time prefix, and
`diff <old>` resolves to the newest match strictly older than the new side.

---

## 8. Diff semantics

- Category totals are rolled up so each category includes its subtree.
- `added_bytes` / `removed_bytes` are computed from the directory buckets,
  which partition every scanned byte. This means
  `added - removed == new_total - old_total` exactly.
- Snapshot comparison is by key (category id, or directory path), so deleted
  paths appear as removals and new paths as additions.
- Terminal display avoids duplication: Developer/AI bytes are shown as
  category rows (rolled to the most specific category with data), everything
  else as tracked directory rows. JSON contains both full lists.

---

## 8b. Watch (v0.4)

`diskdrift watch` uses macOS FSEvents (through the `notify` crate, which is
CC0-licensed and uses the FSEvents backend on macOS).

Pipeline:

```text
FSEvents -> dirty path queue -> debounce (default 2s)
         -> map to nearest directory with a known baseline
         -> collapse descendants -> focused directory scan
         -> delta vs last known size -> events table
```

- No full rescan is triggered by an event.
- A baseline of directory sizes is built once (`--baseline-depth`, default 3,
  including empty directories), persisted in `watch_dirs` and reused.
- Paths are canonicalised (`/var` vs `/private/var`) so watcher events,
  baselines and measurements agree.
- Events below `--min-change` are ignored; disappearing directories are
  recorded as negative "removed" events.
- The pipeline (`process_batch`, `collapse_dirty`, `build_baseline`) is
  independent of FSEvents and unit-tested with temporary directories.

## 9. JSON schema (version 1)

All outputs share `version`, `command` and `timestamp`. Sizes are integers
(bytes); timestamps are local RFC 3339 with offset.

- `scan`: `totals`, `categories` (rolled up, with `parent_id`), `directories`,
  `roots`, `missing_roots`, `skipped`, counts, `interrupted`.
- `snapshot` / `snapshots`: `snapshot(s)` metadata.
- `diff`: `old`, `new`, `total` (added/removed/net), `categories`,
  `directories` with per-key deltas.
- `explain`: `resolved`, `title`, `total`, `breakdown`, `purpose`,
  `deletes_data: false`.
- `doctor`: database status, snapshot count, locations, protected locations,
  last skipped, warnings.

Fields may be added in future versions; existing fields keep their meaning.
Consumers should check `version`.

---

## 10. Privacy and safety guarantees

- No network code; no telemetry, analytics, crash upload or update check.
- No destructive command exists anywhere in the CLI surface.
- `scan` and `explain` never write to the database.
- The data directory is excluded from scans (also when it lives inside a
  scanned root).
- SIGPIPE is restored to default so piping into `head` exits quietly.

---

## 11. Testing strategy

- **Unit tests** for category resolution, classification rules, byte/time
  formatting, target de-duplication, roll-up math and argument parsing.
- **Integration tests** build temporary directory trees and exercise:
  - classification and tracked buckets,
  - hard-link de-duplication,
  - symlink non-following,
  - permission errors (skipped when running as root),
  - exclusions,
  - SQLite round-trip, snapshot resolution and diff math,
  - the compiled CLI binary end to end (`scan --json`, `snapshot`, `diff`,
    `doctor`, `explain`, destructive command rejection, no-DB side effects).
- No test depends on the real machine's directories. Absolute system targets
  (`/opt/homebrew`, `/Library`) are filtered out in tests.

---

## 12. Future work (explicitly not v0.1)

- FSEvents-based change timeline: "between 13:20 and 13:30, CoreSimulator grew
  by 17 GB".
- Anomalous growth detection, reclaimable-space estimation.
- `getattrlistbulk` fast path for directory enumeration.
- APFS snapshot and Time Machine analysis.
- Menu bar / TUI front ends on top of the JSON schema.
- Safe, explicitly opt-in cleanup — requires a separate design and review
  process; v0.1 has no deletion capability at all.
