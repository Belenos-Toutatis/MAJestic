//! Minimal file logger: `%LOCALAPPDATA%\MAJestic\logs\<name>.log`, rotated
//! to `<name>.old.log` above 2 MB. Each process kind has its own file (the
//! elevated worker runs at the same time as the window).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};

const MAX_SIZE: u64 = 2 * 1024 * 1024;

struct FileLogger {
    file: Mutex<Option<File>>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::Level::Info || metadata.target().starts_with("majestic")
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} [{}] {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

pub fn init(name: &str) {
    let dir = majestic_core::paths::logs_dir();
    let path = dir.join(format!("{name}.log"));
    if std::fs::metadata(&path).map(|m| m.len() > MAX_SIZE).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join(format!("{name}.old.log")));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    let logger = Box::leak(Box::new(FileLogger { file: Mutex::new(file) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
}
