# DiskDrift

[![CI](https://github.com/VIUK-Light/Diskdrift/actions/workflows/ci.yml/badge.svg)](https://github.com/VIUK-Light/Diskdrift/actions/workflows/ci.yml)
[![Release](https://github.com/VIUK-Light/Diskdrift/actions/workflows/release.yml/badge.svg)](https://github.com/VIUK-Light/Diskdrift/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

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
- **`watch` / `events` / `what-happened`** — record filesystem changes locally
  and explain what happened in a time window.
- **macOS GUI** — a native SwiftUI app (Dashboard, Storage, History, What
  Happened) that links the same Rust core directly.

Every command supports `--json` (stable schema, `version: 1`) for scripting and
future GUIs.

DiskDrift is **read-only by design**. There is no `clean`, `delete`, `rm` or
`purge` command; destructive cleanup remains out of scope.

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

## Interfaces

DiskDrift core is a Rust library. The CLI and the SwiftUI GUI both use it
directly:

```text
                 ┌──────────────────┐
                 │   DiskDrift Core │  Rust: scan / snapshot / history / events
                 └────────┬─────────┘
                          │ C ABI (JSON, schema 1)
              ┌───────────┴───────────┐
              │                       │
         diskdrift CLI          DiskDrift.app (SwiftUI)
```

The GUI links `libdiskdrift.a` (`gui/include/CDiskDrift.h`) — it never spawns
the CLI and never reimplements storage logic.

## Installation

### macOS GUI

```bash
./gui/build.sh
open dist/DiskDrift.app
```

Requires the Xcode command line tools (Swift) and Rust. The app is built
locally, ad-hoc signed and targets macOS 13 or later (the CLI supports
macOS 11+). Unsigned `.app.zip` archives are also attached to
[Releases](https://github.com/VIUK-Light/Diskdrift/releases); unzip, move to
`/Applications` and right-click → Open the first time.

The GUI has four screens plus a resident menu bar:

- **Dashboard** — volume usage, last 24 hours growth, "Scan now" / "Take snapshot"
- **Storage** — category bars and the largest tracked directories
- **History** — day-over-day changes per category (needs snapshots)
- **What Happened** — incidents from the watch event log
- **Menu bar** — free space, today's change or disk usage in the menu bar,
  with a panel showing the biggest growth and a local alert when free space
  is low or storage grows faster than your thresholds (Settings, ⌘,)

### Prebuilt binaries

Download the archive for your Mac from the
[Releases](https://github.com/VIUK-Light/Diskdrift/releases) page
(Apple Silicon: `aarch64-apple-darwin`, Intel: `x86_64-apple-darwin`),
verify the checksum and install the binary:

```bash
# Replace <version> with the release you downloaded, e.g. 0.2.0
shasum -a 256 -c diskdrift-v<version>-aarch64-apple-darwin.tar.gz.sha256
tar -xzf diskdrift-v<version>-aarch64-apple-darwin.tar.gz
sudo install -m 755 diskdrift-v<version>-aarch64-apple-darwin/diskdrift /usr/local/bin/diskdrift
```

The release binaries are unsigned; see the Gatekeeper note below.

### Build from source (recommended)

```bash
git clone https://github.com/VIUK-Light/Diskdrift.git
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
diskdrift watch [--root <path>]... [--debounce <duration>] [--min-change <size>]
                [--baseline-depth <n>] [--rebaseline] [--run-for <duration>]
                [--threads <n>] [--json]
diskdrift events [--since <duration>] [--limit <n>] [--json]
diskdrift what-happened [--since <duration>] [--from <HH:MM>] [--to <HH:MM>]
                        [--limit <n>] [--json]
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
| `--config <path>` | Configuration file (default: `~/.config/diskdrift/config.toml`) |
| `--root <path>` | Scan a specific path instead of the default locations (repeatable) |
| `--depth <n>` | Directory tracking depth, 1–8 (default: per-location, usually 2) |
| `--threads <n>` | Walker parallelism (default: auto, capped at 8) |
| `--top <n>` | Number of diff rows (default: 20) |
| `--limit <n>` | Number of rows for `top` (default: 20) |

Environment variables:

| Variable | Meaning |
| --- | --- |
| `DISKDRIFT_DATA_DIR` | Override the data directory |
| `DISKDRIFT_HOME` | Override the home directory used for scan locations |
| `DISKDRIFT_CONFIG` | Override the configuration file path |

### scan

```bash
diskdrift scan                  # default locations
diskdrift scan ~/Downloads      # one specific path
diskdrift scan --depth 4        # deeper directory tracking
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

`scan` never writes to the database. When a path or `--depth` is given, a
`Largest directories` section is added so the extra depth is actually
visible.

### top

```bash
diskdrift top
diskdrift top ~/Library --depth 4 --limit 10
```

```text
Largest directories

Scanned 64.4 GB in 6.4s

   1.  17.1 GB  ~/Library/Containers/629C1EE0-…
   2.   6.0 GB  ~/Library/Developer/Xcode/iOS DeviceSupport
   3.   2.4 GB  ~/Library/Developer/CoreSimulator/Devices
   4.   2.4 GB  ~/Library/Group Containers/VUTU7AKEUR.jp.naver.line.mac
   5.   1.8 GB  ~/.lmstudio/models
```

`top` answers "what is big right now?" in one command.

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

Snapshot management:

```bash
diskdrift snapshot list
diskdrift snapshot show 12
diskdrift snapshot delete 12 --yes
```

`snapshot delete` removes only DiskDrift's own metadata — never user files.
In a terminal it asks for confirmation; without a TTY it requires `--yes`.
The legacy `diskdrift snapshots` command is an alias of `snapshot list`.

### diff

```bash
diskdrift diff              # latest two snapshots
diskdrift diff 12 15        # by ID
diskdrift diff 2026-09-12   # by date prefix, compared against latest
diskdrift diff --since 24h  # newest snapshot vs the newest one >= 24h older
diskdrift diff --since 7d
diskdrift diff foo bar      # invalid tokens are reported clearly
```

`--since` accepts durations (`30m`, `24h`, `7d`, `2w`) and fails with a clear
message when no snapshot is old enough yet.

Display rules: Developer/AI changes are shown as categories; everything else
is shown as tracked directories. The JSON output contains the full data at
both category and directory level.

### history

```bash
diskdrift history           # tracked size per day
diskdrift history xcode     # one category over time
```

```text
Storage History

Date              Tracked       Change
2026-09-10        284.2 GB            -
2026-09-11        286.1 GB       +1.9 GB
2026-09-12        287.4 GB       +1.3 GB
2026-09-13        304.8 GB      +17.4 GB
```

When several snapshots land on the same day, the last one represents that
day and `snapshot_count` reports how many were taken. `--json` exposes the
same data as `days[]` with `change_bytes` (null for the first day).

### watch

```bash
diskdrift watch
diskdrift watch --root ~/Library --debounce 1s --min-change 50MB
```

```text
Building baseline...
Watching 15 locations (debounce 2000ms, min change 1.0 MB). Press Ctrl+C to stop.
14:22  ~/Library/Developer/CoreSimulator  +1.2 GB
14:27  ~/Library/Developer/CoreSimulator  +3.8 GB
18:13  ~/.ollama  +4.7 GB
Stopped. 3 event(s) recorded. See `diskdrift events`.
```

`watch` uses macOS FSEvents. It never rescans everything per event:

```text
filesystem event -> directory queue -> debounce -> affected directory scan
                 -> compare with last known size -> store event
```

- Events are debounced (default 2 s) and only changes above `--min-change`
  (default 1 MB) are recorded.
- Dirty paths are mapped to the nearest directory with a known baseline, so
  files and new subdirectories are handled without a full scan.
- The baseline is built once, persisted in the database and reused on the
  next run; use `--rebaseline` to rebuild it.
- `--run-for 10m` makes it exit automatically (useful for scripts);
  `--json` prints one event object per line (JSON Lines).

### events

```bash
diskdrift events
diskdrift events --since 24h --limit 20
```

```text
Storage events

14:22  ~/Library/Developer/CoreSimulator                    +1.2 GB
14:27  ~/Library/Developer/CoreSimulator                    +3.8 GB
18:13  ~/.ollama                                            +4.7 GB
```

Events are stored as metadata only (time, path, category, byte delta) in the
same local SQLite database. `events` reads them; it never touches files.

### what-happened

```bash
diskdrift what-happened
diskdrift what-happened --since 1h
diskdrift what-happened --from 14:00 --to 16:00
```

```text
What happened in the last 24 hours?

Disk usage increased by 23.7 GB (412 events).

1. Xcode / CoreSimulator                    +14.8 GB
   14:21–14:39

2. Ollama                                    +6.1 GB
   18:04–18:18

3. Caches                                    +2.3 GB
   09:12–22:40
```

`what-happened` aggregates the raw `watch` event log into incidents:

- Developer / AI data is reported at its most specific category
  (`Xcode / CoreSimulator`, `Ollama`);
- other data is grouped by category (`Caches`, `Application Support`) or by
  directory when the category is a fallback ("Other");
- each incident shows net change, direction and the time span of its events;
- the headline total is the sum of recorded event deltas — exactly what
  `watch` observed. Use `diff` / `history` for snapshot-based totals.

`--since` and `--from`/`--to` are mutually exclusive. `--from`/`--to` accept
local `HH:MM` (or `HH:MM:SS`) and handle windows that cross midnight.

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
├── OrbStack
├── npm
├── pnpm
├── Yarn
└── Other Developer Data

AI Models
├── Ollama
├── Hugging Face
├── LM Studio
├── MLX
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

## Configuration

Optional configuration lives at `~/.config/diskdrift/config.toml`
(`$XDG_CONFIG_HOME/diskdrift/config.toml` when set, or `--config <path>`):

```toml
# Never scan or snapshot these paths (in addition to DiskDrift's own
# database, which is always excluded).
exclude = ["~/Downloads", "/Volumes/External"]

# Defaults; command-line flags always win.
threads = 4
depth = 3
```

A missing default config file is fine. If the file exists but is invalid,
commands report the parse error; `diskdrift doctor` shows the status and the
configured exclusions.

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
- DiskDrift scans a curated set of locations (see `src/scanners/`), not the entire
  filesystem. Use `--root` to scan anything else.
- Sizes are allocated (on-disk) bytes in terminal output; JSON contains both
  `logical_bytes` and `allocated_bytes`.
- Snapshot entries are tracked at a bounded depth below each scan root
  (usually 1–2 levels) for diffing; `explain` does a live scan for detail.
- `watch` only records changes that happen while it is running; it does not
  replay events from before it started, and events are reported at directory
  granularity, not per file. Deep paths without a baseline are compared
  against their nearest known directory.

## Development

```bash
cargo test          # unit + integration tests (temporary dirs only)
cargo clippy --all-targets
cargo fmt
```

Tests never depend on the real machine's data; they build temporary trees and
exercise the walker, classification, SQLite store, diff engine and the CLI
binary end to end. `gui/smoke.sh` additionally builds a small Swift program
against the C ABI and verifies scan/snapshot/history/error handling without a
GUI.

Project layout:

```text
src/core/       filesystem walker, classification, snapshot store, diff, explain
src/scanners/   per-tool location modules (xcode, homebrew, docker, ollama, …)
src/cli/        argument parsing, commands, rendering, progress
src/ffi.rs      C ABI for native frontends (JSON, schema 1)
gui/            SwiftUI app (CDiskDrift.h, Sources/, build.sh, smoke.sh)
tests/          integration tests
docs/DESIGN.md  architecture and design decisions
```

## Roadmap

| Version | Theme |
| --- | --- |
| v0.1 | Core + scan + snapshot + diff (released) |
| v0.2 | Better scanner: `top`, path scans, `--depth`, config, more tools |
| v0.3 | History: `history`, `diff --since`, snapshot management |
| v0.4 | Watch: FSEvents-based change monitoring |
| v0.5 | What happened: turn event history into explanations |
| v0.6 | macOS GUI (SwiftUI on top of the core library) |
| v0.7 | Menu bar app and local storage alerts |
| v0.8 | Deep macOS storage: APFS snapshots, purgeable space, VM/swap |
| v0.9 | Duplicates and recommendations |
| v1.0 | Stable CLI + GUI + menu bar suite |

The full plan is in [docs/ROADMAP.md](docs/ROADMAP.md). Destructive cleanup
is explicitly out of scope at every stage: DiskDrift is an analyzer, not a
cleaner.

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
