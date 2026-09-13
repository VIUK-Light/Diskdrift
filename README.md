# DiskDrift

**Find out where your macOS storage went — safely, locally, and read-only.**

DiskDrift is a free, open-source CLI that answers the question macOS Storage
never answers clearly:

> Yesterday I had 120 GB free. Today I only have 82 GB. Where did 38 GB go?

```text
Storage changes

2026-09-12  →  2026-09-13

Xcode / CoreSimulator                       +21.4 GB
Ollama                                       +8.2 GB
~/Library/Caches/Google/Chrome               +4.7 GB
Docker                                       +2.6 GB
Other                                        +0.9 GB

Total                                       +37.8 GB
(38.0 GB added, 0.2 GB removed)
```

DiskDrift is **storage diagnosis**: it scans, snapshots, diffs and explains.
It is not a cleaner.

---

## What DiskDrift does

- **`scan`** — classifies the storage that tends to bloat on macOS into
  meaningful categories (Xcode, Caches, Application Support, Ollama, Hugging
  Face, Homebrew, Docker, npm/pnpm, …).
- **`snapshot`** — stores aggregate metadata in a local SQLite database so you
  can compare points in time later. Only sizes and counts are stored, never
  file contents.
- **`diff`** — compares two snapshots and reports added/removed/net change per
  category and per tracked directory.
- **`explain`** — focused, live analysis of one category or path, with a plain
  language description of what that data is for.
- **`doctor`** — shows database health, scan locations, skipped/protected
  locations and whether Full Disk Access would improve coverage.
- **`snapshots`** — lists stored snapshots (IDs, timestamps, tracked size).

Every command supports `--json` (stable schema, `version: 1`) for scripting and
future GUIs.

DiskDrift is **read-only by design**. There is no `clean`, `delete`, `rm` or
`purge` command, and there never will be in v0.1.

---

## What DiskDrift does NOT do

- It does **not** delete, move or modify files.
- It is **not** a macOS cleaner.
- It is **not** an antivirus.
- It is **not** a duplicate-file finder.
- It is **not** a Finder or DaisyDisk replacement.
- It does **not** reproduce Apple's exact "System Data" number (see
  [Limitations](#limitations)).
- It does **not** use the network — not for telemetry, analytics, crash
  reports, updates or anything else.

---

## Privacy

DiskDrift is privacy-first and works entirely offline:

- No telemetry, analytics, crash upload, user tracking or cloud sync.
- No external API calls. Scan results never leave your Mac.
- Snapshots store only metadata: paths, categories, byte counts, file counts
  and timestamps. File contents are never stored or transmitted.
- The SQLite database lives at
  `~/Library/Application Support/DiskDrift/diskdrift.sqlite3` and is only
  touched by `snapshot`, `diff`, `snapshots` and `doctor`.

You can verify the absence of network code yourself:
`grep -rn "TcpStream\|reqwest\|http" src/` finds nothing.

---

## Supported macOS versions

- macOS 11 (Big Sur) or later, Apple Silicon and Intel.
- Built with stable Rust; the release binary links only against system
  libraries.
- Full Disk Access is **not required**. Without it, protected locations are
  skipped with a warning instead of failing the scan.

---

## Installation

### Build from source (recommended)

```bash
git clone https://github.com/<your-account>/diskdrift.git
cd diskdrift
cargo build --release
install -m 755 target/release/diskdrift /usr/local/bin/diskdrift
```

Requires the Rust toolchain (<https://rustup.rs>). The first build compiles a
bundled SQLite, so it takes a minute.

### cargo install

```bash
cargo install --path .
```

### Gatekeeper note

If you download an unsigned prebuilt binary and macOS blocks it, either build
from source or remove the quarantine attribute:

```bash
xattr -d com.apple.quarantine ./diskdrift
```

---

## Commands

```text
diskdrift scan [--json] [--no-progress] [--verbose] [--root <path>]... [--threads <n>]
diskdrift snapshot [--json] [--no-progress] [--root <path>]... [--threads <n>]
diskdrift diff [<old>] [<new>] [--json] [--top <n>]
diskdrift explain <category|path> [--json] [--no-progress] [--threads <n>]
diskdrift doctor [--json]
diskdrift snapshots [--json]
diskdrift help [command]
diskdrift version
```

Global options:

| Option | Meaning |
| --- | --- |
| `--json` | Machine-readable output (schema `version: 1`) |
| `--no-progress` | Disable the live progress display |
| `--verbose` | Show skipped location details |
| `--data-dir <path>` | Override the data directory |
| `--root <path>` | Scan a specific path instead of the default locations (repeatable) |
| `--threads <n>` | Walker parallelism (default: auto, capped at 8) |
| `--top <n>` | Number of diff rows (default: 20) |

Environment variables:

| Variable | Meaning |
| --- | --- |
| `DISKDRIFT_DATA_DIR` | Override the data directory |
| `DISKDRIFT_HOME` | Override the home directory used for scan locations |

### scan

```bash
diskdrift scan
```

```text
DiskDrift

System-related storage
─────────────────────────────────
Developer                  12.8 GB
  Xcode                     9.2 GB
  Homebrew                  2.0 GB
  npm                       1.6 GB
AI Models                   4.8 GB
  Hugging Face            162.6 MB
  LM Studio                 4.7 GB
Applications               40.7 GB
  Application Support      14.7 GB
  Containers               22.6 GB
  Group Containers          3.4 GB
System/User Data            4.9 GB
  Caches                    4.8 GB
  Logs                     44.0 MB

Detected total             63.2 GB
Scanned 496,668 files, 89,283 directories in 7.6s (8 threads)
```

`scan` never writes to the database.

### snapshot

```bash
diskdrift snapshot
```

```text
Snapshot created

ID: 2
Created: 2026-09-13 17:40:10
Tracked: 63.2 GB (496,674 files)
Skipped: 151 protected or unreadable locations
```

Snapshots are stored in SQLite with a schema version and support future
migrations. A Ctrl+C during the scan cancels the whole operation: partial
snapshots are never saved.

### diff

```bash
diskdrift diff              # latest two snapshots
diskdrift diff 12 15        # by ID
diskdrift diff 2026-09-12   # by date prefix, compared against latest
diskdrift diff foo bar      # invalid tokens are reported clearly
```

Display rules: Developer/AI changes are shown as categories; everything else
is shown as tracked directories. The JSON output contains the full data at
both category and directory level.

### explain

```bash
diskdrift explain xcode
diskdrift explain ollama
diskdrift explain ~/Library/Developer
```

```text
Xcode
────────────────────────────────

Total: 9.2 GB

DeviceSupport                         6.0 GB
CoreSimulator                         2.4 GB
Other                               797.8 MB
DerivedData                          12.3 KB

Purpose:
Data created by Xcode during development, builds and iOS
Simulator usage.

DiskDrift does not delete this data.
```

`explain` performs a focused live scan of just the relevant locations, so it
works without any snapshot.

### doctor

```bash
diskdrift doctor
```

Shows the data directory, database version and size, snapshot count, each scan
location's status (`ok` / `missing` / `denied` / `error`), TCC-protected
locations, and the locations skipped in the latest snapshot.

### JSON

```bash
diskdrift scan --json
```

```json
{
  "version": 1,
  "command": "scan",
  "timestamp": "2026-09-13T16:40:21+09:00",
  "duration_ms": 7630,
  "threads": 8,
  "totals": {
    "logical_bytes": 66000000000,
    "allocated_bytes": 63200000000,
    "file_count": 496668,
    "directory_count": 89283
  },
  "categories": [
    {
      "id": "developer",
      "name": "Developer",
      "parent_id": null,
      "logical_bytes": 12800000000,
      "allocated_bytes": 12000000000,
      "file_count": 100000,
      "directory_count": 10000
    }
  ],
  "directories": [],
  "roots": [],
  "missing_roots": [],
  "skipped": [],
  "skipped_count": 0,
  "symlink_count": 1234,
  "hardlink_deduplicated_count": 3699,
  "interrupted": false
}
```

`categories` contains **rolled-up** totals: a parent's `allocated_bytes`
includes its descendants. Do not sum across levels.

---

## Classification

Paths are mapped to a small tree by data-driven prefix rules
(`src/core/classify.rs`), separate from the scanner core:

```text
Developer
├── Xcode (DerivedData, CoreSimulator, Archives, DeviceSupport)
├── Homebrew
├── Docker
├── npm
├── pnpm
└── Other Developer Data

AI Models
├── Ollama
├── Hugging Face
├── LM Studio
└── Other Models

Applications
├── Application Support
├── Containers
└── Group Containers

System/User Data
├── Caches
├── Logs
├── Temporary Data
└── Other
```

## Implementation language

DiskDrift is written in **Rust**, chosen for concrete technical reasons:

- **Correct syscall access** — `lstat`/`read_dir` via `std::os::unix` expose
  device/inode/block counts, which are required for hard-link de-duplication
  and allocated-size accounting.
- **Single binary distribution** — no runtime to install; `cargo build
  --release` produces one executable.
- **Bounded memory and parallelism** — the streaming walker and worker pool use
  `std::thread` with a work queue; measured on a real machine, 8 threads
  reduce a 496k-file scan from 43 s to 7.6 s.
- **Predictable performance** — no GC pauses while traversing hundreds of
  thousands of entries.
- **Future macOS APIs** — FSEvents and `getattrlistbulk` are reachable through
  the `libc` crate with no FFI scaffolding.

See [docs/DESIGN.md](docs/DESIGN.md) for the architecture, guarantees and
trade-offs.

---

## Limitations

- DiskDrift's numbers **do not match macOS Storage's "System Data" exactly**.
  APFS clones, APFS snapshots, purgeable space and Apple's private
  classification rules are not fully observable from user space. DiskDrift
  reports what it can actually see and count.
- **Symbolic links are never followed.** Circular links cannot cause loops or
  double counting; data that is only reachable through a symlink is not
  counted.
- **Hard links are counted once per (device, inode)** for byte totals. File
  counts still count each directory entry.
- **Protected locations are skipped** unless Full Disk Access is granted. The
  scan reports how many locations were skipped; run `diskdrift doctor` for
  details.
- v0.1 scans a curated set of locations (see `src/scanners/`), not the entire
  filesystem. Use `--root` to scan anything else.
- Sizes are allocated (on-disk) bytes in terminal output; JSON contains both
  `logical_bytes` and `allocated_bytes`.
- Snapshot entries are tracked at a bounded depth below each scan root
  (usually 1–2 levels) for diffing; `explain` does a live scan for detail.

## Development

```bash
cargo test          # unit + integration tests (temporary dirs only)
cargo clippy --all-targets
cargo fmt
```

Tests never depend on the real machine's data; they build temporary trees and
exercise the walker, classification, SQLite store, diff engine and the CLI
binary end to end.

Project layout:

```text
src/core/       filesystem walker, classification, snapshot store, diff, explain
src/scanners/   per-tool location modules (xcode, homebrew, docker, ollama, …)
src/cli/        argument parsing, commands, rendering, progress
tests/          integration tests
docs/DESIGN.md  architecture and design decisions
```

## Roadmap (not in v0.1)

FSEvents-based timeline tracking, unusual-growth detection, APFS snapshot
analysis, reclaimable-space estimation, Homebrew formula, TUI/menu bar app.
Destructive cleanup is explicitly out of scope.

## License

MIT. See [LICENSE](LICENSE).

---

## 日本語概要

DiskDrift は macOS のストレージが「いつ・どこで・どれだけ増えたか」を調査する
無料・OSS の CLI ツールです。`scan` / `snapshot` / `diff` / `explain` /
`doctor` を提供し、`--json` で機械可読な出力も得られます。

- 完全ローカル動作。通信・テレメトリ・解析は一切ありません。
- ファイル内容は保存せず、削除も行いません（`clean` 等のコマンドは存在しません）。
- symlink は追跡せず、hard link は inode 単位で重複排除します。
- 権限エラーがあってもスキャン全体は失敗せず、`doctor` で確認できます。
- APFS の clone・snapshot・purgeable 領域のため、macOS 設定の「システムデータ」と
  完全一致はしません。DiskDrift は実際に観測できる値を提示します。
