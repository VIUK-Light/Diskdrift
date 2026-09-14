//! Disk/volume usage for the current machine.

use crate::core::error::{Error, Result};
use std::ffi::CString;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
pub struct DiskUsage {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub used: u64,
}

/// `statfs` for the volume containing `path`.
pub fn disk_usage(path: &Path) -> Result<DiskUsage> {
    let c_path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| Error::Message("path contains a NUL byte".into()))?;
    unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
            return Err(Error::Message(format!(
                "statfs failed for {}",
                path.display()
            )));
        }
        let block = stat.f_bsize as u64;
        let total = stat.f_blocks as u64 * block;
        let free = stat.f_bfree as u64 * block;
        let available = stat.f_bavail as u64 * block;
        Ok(DiskUsage {
            total,
            free,
            available,
            used: total.saturating_sub(available),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_volume_has_space() {
        let usage = disk_usage(Path::new("/")).unwrap();
        assert!(usage.total > 0);
        assert!(usage.available <= usage.total);
        assert!(usage.used <= usage.total);
        assert!(usage.free <= usage.total);
    }
}
