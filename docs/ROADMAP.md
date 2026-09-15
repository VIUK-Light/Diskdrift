# DiskDrift Roadmap (v0.2 → v1.0)

DiskDrift の最終目標は、

> **macOS のストレージが「どこで」「いつ」「なぜ」増えたのかを説明する、無料・OSS の Storage Forensics ツール**

である。

最終的には3つのインターフェースを提供する。

```text
DiskDrift Core
      │
      ├── CLI
      │
      ├── macOS GUI
      │
      └── Menu Bar
```

ストレージ解析ロジックを GUI 内に直接実装しない。

| Version | Theme | Status |
| --- | --- | --- |
| v0.1 | Core + Scan + Snapshot + Diff | ✅ released |
| v0.2 | Better Scanner — 現在のストレージ構造を正確に理解する | ✅ released |
| v0.3 | History — 「どれだけ増えた？」 | ✅ released |
| v0.4 | Watch — 「いつ増えた？」 | ✅ released |
| v0.5 | What Happened? — 低レベル変更履歴を説明に変える | ✅ released |
| v0.6 | macOS GUI | ✅ released |
| v0.7 | Menu Bar | ✅ released |
| v0.8 | macOS Deep Storage | ✅ released |
| v0.9 | Duplicates + Recommendations | ✅ released |
| v1.0 | Stable CLI + GUI + Menu Bar | ✅ released |

---

## v0.2 — Better Scanner

### テーマ

**現在のストレージ構造を正確に理解する。**

追加:

```bash
diskdrift top
diskdrift scan <path>
diskdrift scan --depth 3
```

対応カテゴリを増やす。

```text
Developer
├── Xcode
├── CoreSimulator
├── Homebrew
├── Docker
├── OrbStack
├── npm
├── pnpm
└── Yarn

AI
├── Ollama
├── Hugging Face
├── LM Studio
└── MLX

Applications
├── Application Support
├── Containers
├── Group Containers
├── Caches
└── Logs
```

設定:

```text
~/.config/diskdrift/config.toml
```

除外設定にも対応する。

> **実装ノート**
> v0.1 の既存 Snapshot との差分互換性を保つため、`Caches` / `Logs` は
> カテゴリ ID を変更せず `System/User Data` 配下のまま維持する
> (`system.caches` / `system.logs`)。カテゴリ ID は History の同一性の
> 基盤であり、表示上の分類よりも安定性を優先する。

---

## v0.3 — History

### テーマ

**「どれだけ増えた？」**

Snapshot を本格化する。

```bash
diskdrift history
diskdrift history xcode

diskdrift diff --since 24h
diskdrift diff --since 7d
```

例:

```text
Storage History


Sep 10    284.2 GB
Sep 11    286.1 GB    +1.9 GB
Sep 12    287.4 GB    +1.3 GB
Sep 13    304.8 GB   +17.4 GB
```

Snapshot 管理:

```bash
diskdrift snapshot list
diskdrift snapshot show <id>
diskdrift snapshot delete <id>
```

---

## v0.4 — Watch

### テーマ

**「いつ増えた？」**

FSEvents などを利用してファイルシステム変更を監視する。

```bash
diskdrift watch
diskdrift events
```

例:

```text
14:22
~/Library/Developer/CoreSimulator
+1.2 GB


14:27
~/Library/Developer/CoreSimulator
+3.8 GB


18:13
~/.ollama
+4.7 GB
```

変更イベントごとに全ストレージを再スキャンしてはいけない。

```text
Filesystem event
      ↓
Directory queue
      ↓
Debounce
      ↓
Affected directory scan
      ↓
Database update
```

---

## v0.5 — What Happened?

### テーマ

**DiskDrift の核心機能。**

低レベルな変更履歴をまとめ、

> 何が起きたのか

を説明する。

```bash
diskdrift what-happened
```

例:

```text
What happened in the last 24 hours?


Disk usage increased by 23.7 GB.


1. Xcode / CoreSimulator
   +14.8 GB
   14:21–14:39


2. Ollama
   +6.1 GB
   18:04–18:18


3. Application Caches
   +2.3 GB
```

時間指定:

```bash
diskdrift what-happened --since 1h
diskdrift what-happened --since 24h
diskdrift what-happened --from 14:00 --to 16:00
```

大量イベントを意味のある単位に集約する。

---

## v0.6 — macOS GUI

### テーマ

**ターミナルを使わない人でも DiskDrift を使えるようにする。**

macOS ネイティブ GUI を追加する。第一候補は SwiftUI。
ただしストレージ解析ロジックは GUI 側に再実装しない。

```text
┌─────────────────────────────┐
│ DiskDrift Core              │
│                             │
│ scanner / history / events  │
└─────────────┬───────────────┘
              │
       ┌──────┴──────┐
       │             │
      CLI           GUI
```

### Dashboard

```text
DiskDrift


Macintosh HD
━━━━━━━━━━━━━━━━━━━━━━━━━━━━


Used                  412 GB
Free                   88 GB


Last 24 hours
↑ 18.4 GB


Main growth


Xcode             +11.2 GB
Ollama             +4.7 GB
Caches             +1.8 GB
Other              +0.7 GB
```

### Storage Categories

```text
Developer             83.4 GB
Applications          67.8 GB
AI Models             42.1 GB
Caches                21.7 GB
System                18.2 GB
Other                 16.4 GB
```

### Directory Explorer

```text
Developer
│
├─ Xcode                    61.4 GB
│   ├─ CoreSimulator        31.2 GB
│   ├─ DerivedData          17.8 GB
│   ├─ Archives              8.1 GB
│   └─ Other                 4.3 GB
│
├─ Homebrew                 12.7 GB
└─ Docker                    9.3 GB
```

Finder のようなファイルブラウザではなく、
**ストレージ用途を理解する Explorer** とする。

### History Graph

```text
Storage


420 GB ┤                         ●
      │                    ●────
400 GB ┤              ●────
      │      ●────●───
380 GB ┼──────
      └────────────────────────
       Mon Tue Wed Thu Fri
```

カテゴリ単位にも切り替え可能: `All / Developer / AI / Caches / Applications`

### What Happened 画面

```text
What happened?


Last 24 hours


+23.7 GB


14:21–14:39
Xcode Simulator
+14.8 GB


18:04–18:18
Ollama
+6.1 GB


20:31–22:14
Application Caches
+2.3 GB
```

---

## v0.7 — Menu Bar

### テーマ

**DiskDrift を常駐させる。**

通常時:

```text
◉ 88 GB
```

または、

```text
◉ +3.2 GB
```

### Menu Bar Panel

```text
┌──────────────────────────────┐
│ DiskDrift                    │
│                              │
│ Free               88.4 GB  │
│ Today             +12.7 GB  │
│                              │
│ Biggest growth              │
│ Xcode              +7.8 GB  │
│ Ollama             +3.1 GB  │
│ Caches             +1.2 GB  │
│                              │
│ Last scan        2 min ago   │
│                              │
│ Open DiskDrift               │
└──────────────────────────────┘
```

### Menu Bar Modes

設定で表示を選択可能:

```text
Free space         88 GB
Today's change     +12.7 GB
Disk usage         412 / 500 GB
```

### Storage Alert

```text
DiskDrift


Storage increased by 18.4 GB
during the last hour.


Main cause:
Xcode CoreSimulator
+16.9 GB
```

通知は完全にローカルで行う。条件は設定可能:

```text
Free storage < 20 GB
Storage growth > 10 GB/hour
Storage growth > 25 GB/day
```

---

## v0.8 — macOS Deep Storage

### テーマ

**普通の disk analyzer では理解しにくい macOS 固有データを解析する。**

対象:

```text
APFS Volumes
APFS Snapshots
Time Machine local snapshots
Purgeable storage
Swap
VM
System caches
Data/System volume relationship
```

CLI:

```bash
diskdrift system
diskdrift snapshots
diskdrift volumes
```

GUI では:

```text
System


Local snapshots       21.4 GB
VM / Swap               8.1 GB
System caches           5.8 GB
Other                  11.2 GB
```

推測値の場合は明確に `Estimated` / `Likely reclaimable` /
`macOS managed` と表示する。

---

## v0.9 — Duplicates + Recommendations

### Duplicate Detection

```bash
diskdrift duplicates
diskdrift duplicates --models
diskdrift large
```

判定:

```text
size
↓
metadata
↓
partial hash
↓
full hash
```

巨大ファイル全部を最初から hash しない。

### AI Model Analysis

```text
Qwen3


LM Studio
5.21 GB


Downloads
5.21 GB


SHA-256 identical


Potential duplicate:
5.21 GB
```

GGUF / safetensors metadata も解析する。

### Recommendations

```bash
diskdrift recommendations
```

```text
Storage Insights


Xcode DerivedData
13.7 GB


Risk
LOW


What is it?
Temporary build output generated by Xcode.


DiskDrift recommendation:
Review if old projects are no longer needed.
```

安全度: `Informational` / `Low` / `Medium` / `High` / `Unknown`

---

## v1.0 — DiskDrift Stable

> **CLI + GUI + Menu Bar を持つ macOS Storage Forensics Suite**

### CLI

```bash
diskdrift scan
diskdrift snapshot
diskdrift diff
diskdrift history
diskdrift watch
diskdrift what-happened
diskdrift top
diskdrift system
diskdrift duplicates
diskdrift recommendations
diskdrift doctor
```

### GUI

```text
Dashboard / Storage / History / What Happened / Explore /
System Data / Duplicates / Insights / Settings
```

### Menu Bar

`Free space` / `Today's growth` / `Major growth source` / `Last scan`、
異常増加通知に対応。

---

## Architecture

```text
                  ┌──────────────────┐
                  │   DiskDrift Core │
                  │                  │
                  │ Filesystem       │
                  │ Classification   │
                  │ Snapshots        │
                  │ History          │
                  │ FSEvents         │
                  │ Diff             │
                  │ Insights         │
                  └────────┬─────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
             CLI          GUI       Menu Bar
```

GUI と Menu Bar から core 機能を直接利用できる設計を優先する。
外部 CLI プロセスを毎回起動して結果を parse するだけの設計には極力しない。

## Background Service

常時監視のため、将来的に `diskdriftd` 相当の background component を検討する。

```text
FSEvents monitoring
Periodic snapshots
Directory size refresh
Growth detection
Notification triggers
```

ただし CPU・SSD 負荷を最小限にする。idle 時にはほぼ動作しないことを目標とする。

## GUI Safety

v1.0 時点でも `Delete` / `Clean All` / `Optimize` などの危険な操作を
中心機能にしない。DiskDrift は **Cleaner ではなく Analyzer** である。
`Show in Finder` などは実装可能。

## Privacy

全インターフェース共通:

```text
No account / No cloud / No telemetry / No analytics /
No AI API / No tracking
```

完全ローカル。

## Distribution

- プロジェクト自体は無料・OSS。
- CLI は GitHub Releases から配布する。
- GUI も GitHub Releases から配布可能な構成にする。
- Apple Developer Program を契約していない段階では
  `source build` / `unsigned binary/app` を基本とする。
- 将来的な署名・notarization に依存しない設計にする。

## Release Structure

```text
v0.1  Core + Scan + Snapshot + Diff          ✅
v0.2  Detailed Scanner                        ✅
v0.3  History                                 ✅
v0.4  Watch / FSEvents                        ✅
v0.5  What Happened                           ✅
v0.6  macOS GUI                               ✅
v0.7  Menu Bar                                ✅
v0.8  Deep macOS Storage                      ✅
v0.9  Duplicates + Recommendations            ✅
v1.0  Stable CLI + GUI + Menu Bar             ✅
```

## 核心

競合と差別化する中心機能は「今どこが大きい？」だけではない。

```text
「昨日から30GB減ったのはなぜ？」
「何時ごろ増えた？」
「どのアプリ・開発環境に関連している？」
「System Dataの中で何が増えた？」
```

最終的な価値は、

> **Your disk is full. DiskDrift tells you why.**
