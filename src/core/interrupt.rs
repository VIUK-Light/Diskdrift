//! Ctrl+C handling without external crates.
//!
//! The handler only sets an atomic flag (async-signal-safe). Scanners poll it
//! and stop as soon as the current directory entry batch is finished.

use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_sig: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

/// Install SIGINT handler (idempotent).
pub fn install() {
    unsafe {
        let handler: extern "C" fn(libc::c_int) = on_sigint;
        libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
        // Restore default SIGPIPE behaviour so `diskdrift scan | head` exits
        // quietly instead of panicking on a broken pipe.
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

#[cfg(test)]
pub fn reset() {
    INTERRUPTED.store(false, Ordering::SeqCst);
}
