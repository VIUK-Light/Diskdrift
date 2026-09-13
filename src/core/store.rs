//! SQLite-backed snapshot store.
//!
//! Only metadata and aggregates are stored — never file contents.
//! Schema changes go through `SCHEMA_VERSION` + `migrate`.

use crate::core::categories;
use crate::core::error::{Error, Result};
use crate::core::scan::ScanOutput;
use crate::core::snapshot::{
    CatVal, CategoryRow, DirectoryRow, EventDraft, EventRow, SnapshotMeta,
};
use crate::core::time;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: i64 = 2;
pub const DB_FILE_NAME: &str = "diskdrift.sqlite3";

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS snapshots (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at           TEXT    NOT NULL,
    created_at_local     TEXT    NOT NULL,
    total_logical_bytes  INTEGER NOT NULL,
    total_allocated_bytes INTEGER NOT NULL,
    file_count           INTEGER NOT NULL,
    directory_count      INTEGER NOT NULL,
    symlink_count        INTEGER NOT NULL DEFAULT 0,
    skipped_count        INTEGER NOT NULL DEFAULT 0,
    duration_ms          INTEGER NOT NULL DEFAULT 0,
    app_version          TEXT    NOT NULL
);
CREATE TABLE IF NOT EXISTS categories (
    snapshot_id      INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    category_id      TEXT    NOT NULL,
    logical_bytes    INTEGER NOT NULL,
    allocated_bytes  INTEGER NOT NULL,
    file_count       INTEGER NOT NULL,
    directory_count  INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, category_id)
);
CREATE TABLE IF NOT EXISTS scan_entries (
    snapshot_id      INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    kind             TEXT    NOT NULL,
    path             TEXT    NOT NULL,
    category_id      TEXT    NOT NULL,
    logical_bytes    INTEGER NOT NULL,
    allocated_bytes  INTEGER NOT NULL,
    file_count       INTEGER NOT NULL,
    directory_count  INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, kind, path)
);
CREATE INDEX IF NOT EXISTS idx_scan_entries_snapshot ON scan_entries(snapshot_id);
CREATE TABLE IF NOT EXISTS skipped_locations (
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    reason      TEXT    NOT NULL,
    kind        TEXT    NOT NULL DEFAULT 'error',
    PRIMARY KEY (snapshot_id, path)
);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_unix    INTEGER NOT NULL,
    timestamp_local   TEXT    NOT NULL,
    kind              TEXT    NOT NULL,
    path              TEXT    NOT NULL,
    category_id       TEXT    NOT NULL,
    delta_bytes       INTEGER NOT NULL,
    allocated_bytes   INTEGER NOT NULL,
    file_count        INTEGER NOT NULL DEFAULT 0,
    directory_count   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_events_time ON events(timestamp_unix);
CREATE TABLE IF NOT EXISTS watch_dirs (
    path           TEXT    PRIMARY KEY,
    category_id    TEXT    NOT NULL,
    allocated_bytes INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL
);
"#;

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let store = Store {
            conn,
            path: path.to_path_buf(),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn data_dir_for(home: &Path) -> PathBuf {
        home.join("Library/Application Support/DiskDrift")
    }

    pub fn default_path(data_dir: &Path) -> PathBuf {
        data_dir.join(DB_FILE_NAME)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::Message(format!(
                "database schema version {version} is newer than this build supports ({SCHEMA_VERSION}); upgrade DiskDrift"
            )));
        }
        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
        }
        if version < 2 {
            self.conn.execute_batch(SCHEMA_V2)?;
        }
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        self.conn.execute(
            "INSERT INTO metadata(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    pub fn insert_snapshot(&mut self, out: &ScanOutput, app_version: &str) -> Result<SnapshotMeta> {
        let created_at = time::format_utc(out.started_at);
        let created_at_local = time::format_local(out.started_at);
        let totals = &out.walk.totals;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshots
             (created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
              file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                created_at,
                created_at_local,
                totals.logical as i64,
                totals.allocated as i64,
                totals.files as i64,
                totals.dirs as i64,
                totals.symlinks as i64,
                out.walk.skipped_count as i64,
                out.duration.as_millis() as i64,
                app_version,
            ],
        )?;
        let id = tx.last_insert_rowid();

        {
            let mut stmt = tx.prepare(
                "INSERT INTO categories
                 (snapshot_id, category_id, logical_bytes, allocated_bytes, file_count, directory_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (idx, acc) in out.walk.categories.iter().enumerate() {
                if acc.is_zero() {
                    continue;
                }
                stmt.execute(params![
                    id,
                    categories::def_by_index(idx).id,
                    acc.logical as i64,
                    acc.allocated as i64,
                    acc.files as i64,
                    acc.dirs as i64,
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare(
                "INSERT INTO scan_entries
                 (snapshot_id, kind, path, category_id, logical_bytes, allocated_bytes, file_count, directory_count)
                 VALUES (?1, 'directory', ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (path, dir) in &out.walk.directories {
                if dir.acc.is_zero() {
                    continue;
                }
                stmt.execute(params![
                    id,
                    path.to_string_lossy(),
                    categories::def_by_index(dir.category).id,
                    dir.acc.logical as i64,
                    dir.acc.allocated as i64,
                    dir.acc.files as i64,
                    dir.acc.dirs as i64,
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO scan_entries
                 (snapshot_id, kind, path, category_id, logical_bytes, allocated_bytes, file_count, directory_count)
                 VALUES (?1, 'root', ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (i, target) in out.targets.iter().enumerate() {
                let acc = out.walk.targets.get(i).copied().unwrap_or_default();
                stmt.execute(params![
                    id,
                    target.path.to_string_lossy(),
                    target.category_hint,
                    acc.logical as i64,
                    acc.allocated as i64,
                    acc.files as i64,
                    acc.dirs as i64,
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO skipped_locations
                 (snapshot_id, path, reason, kind)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for skip in out.walk.skipped.iter().take(500) {
                stmt.execute(params![
                    id,
                    skip.path.to_string_lossy(),
                    skip.reason,
                    skip.kind_str(),
                ])?;
            }
        }

        tx.commit()?;

        self.snapshot_by_id(id)?
            .ok_or_else(|| Error::Message("failed to read back inserted snapshot".into()))
    }

    pub fn snapshot_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?)
    }

    pub fn list_snapshots(&self, limit: usize) -> Result<Vec<SnapshotMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
                    file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version
             FROM snapshots ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_meta)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// All snapshots in ascending (oldest first) order.
    pub fn all_snapshots(&self) -> Result<Vec<SnapshotMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
                    file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version
             FROM snapshots ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], row_to_meta)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Resolve `--since <duration>`: the newest snapshot and the newest
    /// snapshot that is at least `seconds` older than it.
    pub fn resolve_since(&self, seconds: i64) -> Result<(SnapshotMeta, SnapshotMeta)> {
        let all = self.all_snapshots()?;
        let new = all
            .last()
            .cloned()
            .ok_or_else(|| Error::Message("no snapshots stored yet".into()))?;
        let target = new.unix_time().saturating_sub(seconds);
        let old = all
            .iter()
            .rev()
            .find(|m| m.unix_time() <= target)
            .cloned()
            .ok_or_else(|| {
                Error::Message(
                    "no snapshot is old enough to cover that period. Run `diskdrift snapshot` periodically."
                        .into(),
                )
            })?;
        Ok((old, new))
    }

    /// Delete a snapshot (and its entries, via foreign key cascade).
    /// Only DiskDrift's own metadata is removed — never user files.
    pub fn delete_snapshot(&self, id: i64) -> Result<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM snapshots WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn snapshot_by_id(&self, id: i64) -> Result<Option<SnapshotMeta>> {
        let meta = self
            .conn
            .query_row(
                "SELECT id, created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
                        file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version
                 FROM snapshots WHERE id = ?1",
                params![id],
                row_to_meta,
            )
            .optional()?;
        Ok(meta)
    }

    /// Resolve `latest`, a numeric id, or a timestamp prefix
    /// (e.g. `2026-09-13` or `2026-09-13T16:40:21`).
    pub fn resolve_snapshot(&self, token: &str) -> Result<SnapshotMeta> {
        let token = token.trim();
        if token.eq_ignore_ascii_case("latest") {
            return self
                .list_snapshots(1)?
                .into_iter()
                .next()
                .ok_or_else(|| Error::Message("no snapshots stored yet".into()));
        }
        if let Ok(id) = token.parse::<i64>() {
            return self
                .snapshot_by_id(id)?
                .ok_or_else(|| Error::Message(format!("snapshot id {id} not found")));
        }
        let like = format!("{token}%");
        let meta = self
            .conn
            .query_row(
                "SELECT id, created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
                        file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version
                 FROM snapshots
                 WHERE created_at LIKE ?1 OR created_at_local LIKE ?1
                 ORDER BY id DESC LIMIT 1",
                params![like],
                row_to_meta,
            )
            .optional()?;
        meta.ok_or_else(|| Error::Message(format!("no snapshot matches '{token}'")))
    }

    /// Resolve a snapshot token to the newest match older than `before_id`.
    /// Used by `diskdrift diff <old>` where `<old>` should not resolve to the
    /// same snapshot that is being used as `<new>`.
    pub fn resolve_snapshot_before(&self, token: &str, before_id: i64) -> Result<SnapshotMeta> {
        let token = token.trim();
        if let Ok(id) = token.parse::<i64>() {
            return self
                .snapshot_by_id(id)?
                .filter(|m| m.id < before_id)
                .ok_or_else(|| {
                    Error::Message(format!(
                        "snapshot {id} is not older than snapshot {before_id}"
                    ))
                });
        }
        let (sql, param): (&str, String) = if token.eq_ignore_ascii_case("latest") {
            (
                "SELECT id, created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
                        file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version
                 FROM snapshots WHERE id < ?1 ORDER BY id DESC LIMIT 1",
                String::new(),
            )
        } else {
            (
                "SELECT id, created_at, created_at_local, total_logical_bytes, total_allocated_bytes,
                        file_count, directory_count, symlink_count, skipped_count, duration_ms, app_version
                 FROM snapshots
                 WHERE (created_at LIKE ?2 || '%' OR created_at_local LIKE ?2 || '%') AND id < ?1
                 ORDER BY id DESC LIMIT 1",
                token.to_string(),
            )
        };
        let meta = self
            .conn
            .query_row(sql, params![before_id, param], row_to_meta)
            .optional()?;
        meta.ok_or_else(|| {
            Error::Message(format!(
                "no snapshot older than #{before_id} matches '{token}'"
            ))
        })
    }

    pub fn load_categories(&self, snapshot_id: i64) -> Result<Vec<CategoryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT category_id, logical_bytes, allocated_bytes, file_count, directory_count
             FROM categories WHERE snapshot_id = ?1",
        )?;
        let rows = stmt.query_map(params![snapshot_id], |r| {
            Ok(CategoryRow {
                category_id: r.get(0)?,
                val: CatVal {
                    logical: r.get::<_, i64>(1)? as u64,
                    allocated: r.get::<_, i64>(2)? as u64,
                    files: r.get::<_, i64>(3)? as u64,
                    dirs: r.get::<_, i64>(4)? as u64,
                },
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn load_directories(&self, snapshot_id: i64) -> Result<Vec<DirectoryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, category_id, logical_bytes, allocated_bytes, file_count, directory_count
             FROM scan_entries WHERE snapshot_id = ?1 AND kind = 'directory'",
        )?;
        let rows = stmt.query_map(params![snapshot_id], |r| {
            Ok(DirectoryRow {
                path: PathBuf::from(r.get::<_, String>(0)?),
                category_id: r.get(1)?,
                val: CatVal {
                    logical: r.get::<_, i64>(2)? as u64,
                    allocated: r.get::<_, i64>(3)? as u64,
                    files: r.get::<_, i64>(4)? as u64,
                    dirs: r.get::<_, i64>(5)? as u64,
                },
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn load_skipped(&self, snapshot_id: i64) -> Result<Vec<(PathBuf, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, reason, kind FROM skipped_locations WHERE snapshot_id = ?1 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![snapshot_id], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // --- watch events -----------------------------------------------------

    pub fn insert_events(&mut self, drafts: &[EventDraft]) -> Result<()> {
        if drafts.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO events
                 (timestamp_unix, timestamp_local, kind, path, category_id,
                  delta_bytes, allocated_bytes, file_count, directory_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for d in drafts {
                stmt.execute(params![
                    d.timestamp_unix,
                    time::format_local(d.timestamp_unix),
                    d.kind,
                    d.path.to_string_lossy(),
                    d.category_id,
                    d.delta_bytes,
                    d.allocated_bytes as i64,
                    d.file_count as i64,
                    d.directory_count as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Newest `limit` events (optionally since a unix time), oldest first.
    pub fn recent_events(&self, since_unix: Option<i64>, limit: usize) -> Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp_unix, timestamp_local, kind, path, category_id,
                    delta_bytes, allocated_bytes, file_count, directory_count
             FROM (
                 SELECT * FROM events
                 WHERE (?1 IS NULL OR timestamp_unix >= ?1)
                 ORDER BY timestamp_unix DESC, id DESC
                 LIMIT ?2
             )
             ORDER BY timestamp_unix ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![since_unix, limit as i64], |r| {
            Ok(EventRow {
                id: r.get(0)?,
                timestamp_unix: r.get(1)?,
                timestamp_local: r.get(2)?,
                kind: r.get(3)?,
                path: PathBuf::from(r.get::<_, String>(4)?),
                category_id: r.get(5)?,
                delta_bytes: r.get(6)?,
                allocated_bytes: r.get::<_, i64>(7)? as u64,
                file_count: r.get::<_, i64>(8)? as u64,
                directory_count: r.get::<_, i64>(9)? as u64,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn load_watch_dirs(&self) -> Result<Vec<(PathBuf, String, u64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, category_id, allocated_bytes FROM watch_dirs")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn upsert_watch_dirs(&mut self, rows: &[(PathBuf, String, u64)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let now = time::now_unix();
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO watch_dirs(path, category_id, allocated_bytes, updated_at_unix)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(path) DO UPDATE SET
                   category_id = excluded.category_id,
                   allocated_bytes = excluded.allocated_bytes,
                   updated_at_unix = excluded.updated_at_unix",
            )?;
            for (path, category, allocated) in rows {
                stmt.execute(params![
                    path.to_string_lossy(),
                    category,
                    *allocated as i64,
                    now
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_watch_dirs(&mut self, paths: &[PathBuf]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM watch_dirs WHERE path = ?1")?;
            for path in paths {
                stmt.execute(params![path.to_string_lossy()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn db_size_bytes(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }
}

fn row_to_meta(r: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotMeta> {
    Ok(SnapshotMeta {
        id: r.get(0)?,
        created_at: r.get(1)?,
        created_at_local: r.get(2)?,
        total_logical_bytes: r.get::<_, i64>(3)? as u64,
        total_allocated_bytes: r.get::<_, i64>(4)? as u64,
        file_count: r.get::<_, i64>(5)? as u64,
        directory_count: r.get::<_, i64>(6)? as u64,
        symlink_count: r.get::<_, i64>(7)? as u64,
        skipped_count: r.get::<_, i64>(8)? as u64,
        duration_ms: r.get::<_, i64>(9)? as u64,
        app_version: r.get(10)?,
    })
}
