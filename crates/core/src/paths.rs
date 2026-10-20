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

/// `downloads` or `temp` when `exe` sits in a folder that is often emptied
/// (a scheduled task pointing there would soon break).
pub fn volatile_location(exe: &std::path::Path) -> Option<&'static str> {
    let lower = |p: &std::path::Path| p.to_string_lossy().to_lowercase();
    let exe = lower(exe);
    if dirs::download_dir().is_some_and(|d| exe.starts_with(&lower(&d))) {
        return Some("downloads");
    }
    if exe.starts_with(&lower(&std::env::temp_dir())) {
        return Some("temp");
    }
    None
}

pub fn history_file() -> PathBuf {
    data_dir().join("history.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volatile_locations() {
        if let Some(downloads) = dirs::download_dir() {
            assert_eq!(volatile_location(&downloads.join("MAJestic.exe")), Some("downloads"));
        }
        assert_eq!(volatile_location(&std::env::temp_dir().join("x").join("MAJestic.exe")), Some("temp"));
        assert_eq!(volatile_location(std::path::Path::new(r"C:\Tools\MAJestic\MAJestic.exe")), None);
    }
}
