//! Mounted volume information (`statfs` via `getmntinfo`).

use crate::core::error::{Error, Result};
use std::ffi::CStr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub mount_point: PathBuf,
    pub device: String,
    pub fs_type: String,
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub available: u64,
    pub is_system: bool,
}

fn c_field(field: &[libc::c_char]) -> String {
    let cstr = unsafe { CStr::from_ptr(field.as_ptr()) };
    cstr.to_string_lossy().to_string()
}

/// All mounted local volumes, excluding pseudo filesystems.
pub fn volumes() -> Result<Vec<VolumeInfo>> {
    let mut buf: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut buf, libc::MNT_NOWAIT) };
    if count <= 0 || buf.is_null() {
        return Err(Error::Message("cannot enumerate mounted volumes".into()));
    }
    let entries = unsafe { std::slice::from_raw_parts(buf, count as usize) };

    let mut out = Vec::new();
    for fs in entries {
        let fs_type = c_field(&fs.f_fstypename);
        if matches!(
            fs_type.as_str(),
            "devfs" | "autofs" | "map" | "nfs" | "smbfs" | "afpfs" | "webdav" | "nullfs"
        ) {
            continue;
        }
        let mount_point = c_field(&fs.f_mntonname);
        if mount_point.is_empty() {
            continue;
        }
        let block = fs.f_bsize as u64;
        let total = fs.f_blocks * block;
        let free = fs.f_bfree * block;
        let available = fs.f_bavail * block;
        let is_system =
            mount_point.starts_with("/System/") || mount_point == "/System/Volumes/Data";
        out.push(VolumeInfo {
            mount_point: PathBuf::from(&mount_point),
            device: c_field(&fs.f_mntfromname),
            fs_type,
            total,
            used: total.saturating_sub(available),
            free,
            available,
            is_system,
        });
    }
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_at_least_the_root_volume() {
        let volumes = volumes().unwrap();
        assert!(!volumes.is_empty());
        let root = volumes
            .iter()
            .find(|v| v.mount_point == std::path::Path::new("/"))
            .expect("root volume");
        assert!(root.total > 0);
        assert!(root.available <= root.total);
    }
}
