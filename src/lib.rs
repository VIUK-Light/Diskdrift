//! DiskDrift — understand where macOS storage went, safely and locally.
//!
//! v0.1 is read-only by design: it scans, snapshots, diffs and explains.
//! It never deletes files, never sends data anywhere.

pub mod cli;
pub mod core;
pub mod ffi;
pub mod scanners;
