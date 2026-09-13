//! `diskdrift doctor` — permissions, database and scan-location diagnostics.

use crate::core::config::Config;
use crate::core::error::Result;
use crate::core::scan::ScanTarget;
use crate::core::snapshot::SnapshotMeta;
use crate::core::store::Store;
use std::path::{Path, PathBuf};

pub struct LocationCheck {
    pub path: PathBuf,
    pub label: &'static str,
    pub status: &'static str,
    pub detail: Option<String>,
}

pub struct DoctorReport {
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub config_exists: bool,
    pub config_error: Option<String>,
    pub excluded: Vec<PathBuf>,
    pub db_path: PathBuf,
    pub db_exists: bool,
    pub db_size_bytes: u64,
    pub db_writable: bool,
    pub schema_version: i64,
    pub snapshot_count: i64,
    pub latest: Option<SnapshotMeta>,
    pub locations: Vec<LocationCheck>,
    pub protected: Vec<LocationCheck>,
    pub latest_skipped: Vec<(PathBuf, String, String)>,
    pub warnings: Vec<String>,
}

fn probe(path: &Path) -> LocationCheck {
    let label: &'static str = "";
    let (status, detail) = match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ("missing", None),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            ("denied", Some(e.to_string()))
        }
        Err(e) => ("error", Some(e.to_string())),
        Ok(md) if !md.is_dir() => ("error", Some("not a directory".to_string())),
        Ok(_) => match std::fs::read_dir(path) {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                ("denied", Some(e.to_string()))
            }
            Err(e) => ("error", Some(e.to_string())),
            Ok(mut it) => match it.next() {
                Some(Err(e)) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    ("denied", Some(e.to_string()))
                }
                Some(Err(e)) => ("error", Some(e.to_string())),
                _ => ("ok", None),
            },
        },
    };
    LocationCheck {
        path: path.to_path_buf(),
        label,
        status,
        detail,
    }
}

fn with_label(mut c: LocationCheck, label: &'static str) -> LocationCheck {
    c.label = label;
    c
}

pub fn run(
    home: &Path,
    data_dir: &Path,
    targets: &[ScanTarget],
    config: &Config,
    config_error: Option<String>,
) -> Result<DoctorReport> {
    let db_path = Store::default_path(data_dir);

    // Data directory / database checks.
    let mut warnings = Vec::new();
    let db_writable = if db_path.exists() {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&db_path)
        {
            Ok(_) => true,
            Err(e) => {
                warnings.push(format!("database is not writable: {e}"));
                false
            }
        }
    } else {
        match std::fs::create_dir_all(data_dir) {
            Ok(()) => {
                let probe_file = data_dir.join(".diskdrift-write-test");
                match std::fs::write(&probe_file, b"ok") {
                    Ok(()) => {
                        let _ = std::fs::remove_file(&probe_file);
                        true
                    }
                    Err(e) => {
                        warnings.push(format!("data directory is not writable: {e}"));
                        false
                    }
                }
            }
            Err(e) => {
                warnings.push(format!("cannot create data directory: {e}"));
                false
            }
        }
    };

    let (schema_version, snapshot_count, latest, db_size_bytes, db_exists, latest_skipped) =
        if db_path.exists() {
            let store = Store::open(&db_path)?;
            let latest = store.list_snapshots(1)?.into_iter().next();
            let skipped = match &latest {
                Some(meta) => store.load_skipped(meta.id)?,
                None => Vec::new(),
            };
            (
                store.schema_version()?,
                store.snapshot_count()?,
                latest,
                store.db_size_bytes(),
                true,
                skipped,
            )
        } else {
            (
                crate::core::store::SCHEMA_VERSION,
                0,
                None,
                0,
                false,
                Vec::new(),
            )
        };

    let locations: Vec<LocationCheck> = targets
        .iter()
        .map(|t| with_label(probe(&t.path), t.scanner))
        .collect();

    // TCC-protected locations: readable only with Full Disk Access.
    let protected_paths = [
        home.join("Library/Application Support/com.apple.TCC"),
        home.join("Library/Mail"),
        home.join("Library/Messages"),
        home.join("Library/Safari"),
        home.join("Library/Cookies"),
        home.join("Library/Calendars"),
        home.join("Library/Reminders"),
        home.join("Library/Address Book"),
        home.join("Library/HomeKit"),
    ];
    let protected: Vec<LocationCheck> = protected_paths
        .iter()
        .map(|p| with_label(probe(p), "tcc"))
        .collect();

    let denied_count = protected.iter().filter(|c| c.status == "denied").count();
    if denied_count > 0 {
        warnings.push(format!(
            "{denied_count} protected locations are not readable. Grant Full Disk Access to include them (optional)."
        ));
    }
    if let Some(meta) = &latest
        && meta.skipped_count > 0
    {
        warnings.push(format!(
            "the latest scan skipped {} protected or unreadable locations",
            meta.skipped_count
        ));
    }

    Ok(DoctorReport {
        data_dir: data_dir.to_path_buf(),
        config_path: config.path.clone(),
        config_exists: config.exists,
        config_error,
        excluded: config.exclude.clone(),
        db_path,
        db_exists,
        db_size_bytes,
        db_writable,
        schema_version,
        snapshot_count,
        latest,
        locations,
        protected,
        latest_skipped,
        warnings,
    })
}
