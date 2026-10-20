//! Installers left on disk by the package managers (failed or interrupted
//! installs, Scoop's download cache): measured, then removed on request.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;

/// Entries changed more recently may belong to a download in progress.
const MIN_AGE: Duration = Duration::from_secs(10 * 60);

/// Programs that may be using these folders: nothing is removed while one runs.
pub const BUSY_PROCESSES: &[&str] = &["winget.exe", "choco.exe", "msiexec.exe"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Leftovers {
    pub bytes: u64,
    /// Package folders or files.
    pub entries: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cleaned {
    pub freed: u64,
    pub removed: usize,
    /// Entries that could not be removed (in use, denied).
    pub failed: usize,
}

/// A folder where installers are downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadDir {
    pub path: PathBuf,
    /// Removes the files directly inside (Scoop's archives) rather than the
    /// subfolders (one per package for winget and Chocolatey; their loose
    /// files are install logs, kept).
    pub files: bool,
}

/// Subfolders that are not downloads: winget's own `cache`.
const KEEP: &[&str] = &["cache"];

/// Where installers are downloaded: `%TEMP%\WinGet\<id>.<version>`,
/// `%TEMP%\chocolatey\<id>`, Scoop's `cache`.
pub fn download_dirs() -> Vec<DownloadDir> {
    let temp = std::env::temp_dir();
    let mut dirs = vec![
        DownloadDir { path: temp.join("WinGet"), files: false },
        DownloadDir { path: temp.join("chocolatey"), files: false },
    ];
    if let Some(root) = crate::pkgmgr::scoop::root() {
        dirs.push(DownloadDir { path: root.join("cache"), files: true });
    }
    dirs
}

/// Entries of `dirs` old enough to remove, with their size.
fn entries(dirs: &[DownloadDir], now: SystemTime) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(list) = std::fs::read_dir(&dir.path) else { continue };
        for entry in list.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let kept = KEEP.iter().any(|k| entry.file_name().to_string_lossy().eq_ignore_ascii_case(k));
            if meta.is_dir() == dir.files || kept {
                continue;
            }
            let recent = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).is_none_or(|age| age < MIN_AGE);
            if recent {
                continue;
            }
            let path = entry.path();
            let size = if meta.is_dir() { crate::jobs::dir_size(&path) } else { meta.len() };
            found.push((path, size));
        }
    }
    found
}

pub fn measure(dirs: &[DownloadDir]) -> Leftovers {
    measure_at(dirs, SystemTime::now())
}

/// Removes what [`measure`] counts. The caller makes sure no installation runs.
pub fn clean(dirs: &[DownloadDir]) -> Cleaned {
    clean_at(dirs, SystemTime::now())
}

fn measure_at(dirs: &[DownloadDir], now: SystemTime) -> Leftovers {
    let found = entries(dirs, now);
    Leftovers { bytes: found.iter().map(|(_, s)| s).sum(), entries: found.len() }
}

fn clean_at(dirs: &[DownloadDir], now: SystemTime) -> Cleaned {
    let mut result = Cleaned::default();
    for (path, size) in entries(dirs, now) {
        let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        match removed {
            Ok(()) => {
                result.freed += size;
                result.removed += 1;
            }
            Err(e) => {
                log::info!("cleanup: cannot remove {}: {e}", path.display());
                // Partly removed: count what is gone.
                result.freed += size.saturating_sub(if path.exists() { size_of(&path) } else { 0 });
                result.failed += 1;
            }
        }
    }
    log::info!("cleanup: {} entries removed, {} bytes freed, {} failed", result.removed, result.freed, result.failed);
    result
}

fn size_of(path: &Path) -> u64 {
    if path.is_dir() {
        crate::jobs::dir_size(path)
    } else {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_removes_old_entries_only() {
        let base = std::env::temp_dir().join(format!("majestic-cleanup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let winget = base.join("WinGet");
        let old = winget.join("Kieirra.Murmure.2.0.3");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("Murmure_x64.msi"), vec![0u8; 3000]).unwrap();
        std::fs::write(winget.join("install.log"), vec![0u8; 500]).unwrap();
        std::fs::create_dir_all(winget.join("cache")).unwrap();
        std::fs::write(winget.join("cache").join("index"), vec![0u8; 700]).unwrap();
        let scoop = base.join("scoop-cache");
        std::fs::create_dir_all(scoop.join("folder")).unwrap();
        std::fs::write(scoop.join("app#1.0#https_x.zip"), vec![0u8; 400]).unwrap();
        let dirs = vec![
            DownloadDir { path: winget.clone(), files: false },
            DownloadDir { path: scoop.clone(), files: true },
            DownloadDir { path: base.join("missing"), files: false },
        ];

        // Just written: maybe a download in progress, left alone.
        assert_eq!(measure(&dirs), Leftovers::default());
        assert_eq!(clean(&dirs), Cleaned::default());
        // Seen 20 minutes later.
        let later = SystemTime::now() + Duration::from_secs(20 * 60);
        // The package folder and Scoop's archive; not the log, winget's cache
        // or a folder in Scoop's cache.
        assert_eq!(measure_at(&dirs, later), Leftovers { bytes: 3400, entries: 2 });
        assert_eq!(clean_at(&dirs, later), Cleaned { freed: 3400, removed: 2, failed: 0 });
        assert!(winget.join("install.log").is_file() && winget.join("cache").join("index").is_file());
        assert!(scoop.join("folder").is_dir());
        assert_eq!(measure_at(&dirs, later), Leftovers::default());
        let _ = std::fs::remove_dir_all(&base);
    }
}
