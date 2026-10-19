//! Where MAJestic keeps its files: `%LOCALAPPDATA%\MAJestic`.
//!
//! The executable itself stays portable (nothing to install); settings,
//! history, caches and logs are per Windows user, which is what we want for a
//! tool that updates *this* PC.

use std::path::PathBuf;

/// Root data directory, created on demand.
pub fn data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("MAJestic");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn cache_dir() -> PathBuf {
    let dir = data_dir().join("cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn logs_dir() -> PathBuf {
    let dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn history_file() -> PathBuf {
    data_dir().join("history.jsonl")
}
