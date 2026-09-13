//! Live progress display. Only active on a TTY; disabled for `--json`.

use crate::core::categories;
use crate::core::fs::ProgressCounters;
use crate::core::size;
use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Progress {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Progress {
    pub fn start(counters: Arc<ProgressCounters>, enabled: bool) -> Self {
        if !enabled || !std::io::stderr().is_terminal() {
            return Progress {
                stop: Arc::new(AtomicBool::new(true)),
                handle: None,
            };
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut printed = 0usize;
            while !stop_thread.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(150));
                let lines = build_lines(&counters);
                if render(&lines, printed).is_err() {
                    break;
                }
                printed = lines.len();
            }
            let _ = clear(printed);
        });
        Progress {
            stop,
            handle: Some(handle),
        }
    }

    pub fn finish(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build_lines(counters: &ProgressCounters) -> Vec<String> {
    let mut lines = vec!["Scanning storage...".to_string()];
    for (idx, bytes, _files) in counters.top_categories(3) {
        lines.push(format!(
            "{:<22}{:>12}",
            categories::def_by_index(idx).name,
            size::format_bytes(bytes)
        ));
    }
    lines.push(format!(
        "{} files scanned   {}",
        size::format_count(counters.files.load(Ordering::Relaxed)),
        size::format_bytes(counters.bytes.load(Ordering::Relaxed))
    ));
    lines
}

fn render(lines: &[String], previous: usize) -> std::io::Result<()> {
    let mut err = std::io::stderr().lock();
    if previous > 0 {
        write!(err, "\x1b[{previous}A")?;
    }
    let total = previous.max(lines.len());
    for i in 0..total {
        write!(err, "\x1b[2K\r")?;
        if let Some(line) = lines.get(i) {
            write!(err, "{line}")?;
        }
        writeln!(err)?;
    }
    err.flush()
}

fn clear(printed: usize) -> std::io::Result<()> {
    if printed == 0 {
        return Ok(());
    }
    let mut err = std::io::stderr().lock();
    write!(err, "\x1b[{printed}A")?;
    for _ in 0..printed {
        write!(err, "\x1b[2K\r\n")?;
    }
    write!(err, "\x1b[{printed}A")?;
    err.flush()
}
