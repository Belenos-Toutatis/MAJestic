//! Update history (`history.jsonl`, one JSON object per line, append-only).

use std::collections::{HashMap, HashSet};
use std::io::Write;

use crate::model::{FailureKind, HistoryEntry, JobAction};
use crate::paths;

/// Appends an entry. Errors are logged, never fatal.
pub fn append(entry: &HistoryEntry) {
    let result = (|| -> anyhow::Result<()> {
        let line = serde_json::to_string(entry)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths::history_file())?;
        writeln!(file, "{line}")?;
        Ok(())
    })();
    if let Err(e) = result {
        log::warn!("history: cannot append entry: {e:#}");
    }
}

/// Reads the whole history, most recent first. Corrupt lines are skipped.
pub fn load() -> Vec<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(paths::history_file()) else {
        return Vec::new();
    };
    let mut entries: Vec<HistoryEntry> =
        text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    entries.reverse();
    entries
}

/// After this many failed attempts at the same version, the update is no
/// longer preselected and MAJestic suggests ignoring that version.
pub const FAILURE_LIMIT: u32 = 3;

/// Failed attempts per `(key, target version)` since the last success of
/// that package. `entries`: most recent first, as [`load`] returns them.
pub fn failure_counts(entries: &[HistoryEntry]) -> HashMap<(String, String), u32> {
    let mut counts = HashMap::new();
    let mut settled: HashSet<&str> = HashSet::new();
    for e in entries {
        if settled.contains(e.key.as_str()) {
            continue;
        }
        match e.result.as_str() {
            "succeeded" => {
                settled.insert(&e.key);
            }
            "failed" => {
                let targets_version =
                    matches!(e.action, JobAction::Upgrade | JobAction::Interactive | JobAction::InstallVersion { .. });
                if let Some(v) = e.to_version.as_deref().filter(|v| targets_version && !v.trim().is_empty()) {
                    *counts.entry((e.key.clone(), v.to_string())).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }
    counts
}

/// Packages whose most recent attempt failed because winget will not switch
/// installation technologies. `entries`: most recent first.
pub fn technology_mismatches(entries: &[HistoryEntry]) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for e in entries {
        if !seen.insert(e.key.as_str()) {
            continue;
        }
        if e.result == "failed" && e.failure == Some(FailureKind::TechnologyMismatch) {
            found.insert(e.key.clone());
        }
    }
    found
}

/// Failed attempts at installing `version` of `key`, from the history file.
pub fn failed_attempts(key: &str, version: &str) -> u32 {
    failure_counts(&load()).get(&(key.to_string(), version.to_string())).copied().unwrap_or(0)
}

/// Deletes the history file.
pub fn clear() -> anyhow::Result<()> {
    let path = paths::history_file();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn entry(key: &str, action: JobAction, to: &str, result: &str) -> HistoryEntry {
        HistoryEntry {
            date: String::new(),
            key: key.into(),
            source: Source::Winget,
            id: key.into(),
            name: key.into(),
            action,
            from_version: Some("1.0".into()),
            to_version: Some(to.into()),
            result: result.into(),
            failure: None,
            code: None,
            reboot_required: false,
            duration_secs: None,
            download_secs: None,
            install_secs: None,
            download_bytes: None,
            installer_type: None,
        }
    }

    #[test]
    fn failures_since_the_last_success() {
        let up = || JobAction::Upgrade;
        // Most recent first.
        let entries = vec![
            entry("winget:A", up(), "2.0", "failed"),
            entry("winget:A", JobAction::Interactive, "2.0", "failed"),
            entry("winget:B", up(), "5.0", "failed"),
            entry("winget:A", up(), "1.9", "failed"),
            entry("winget:A", up(), "2.0", "failed"),
            entry("winget:B", up(), "5.0", "succeeded"),
            entry("winget:B", up(), "5.0", "failed"),
            entry("winget:C", JobAction::Uninstall, "", "failed"),
            entry("winget:C", up(), "3.0", "cancelled"),
        ];
        let counts = failure_counts(&entries);
        let get = |k: &str, v: &str| counts.get(&(k.to_string(), v.to_string())).copied().unwrap_or(0);
        assert_eq!(get("winget:A", "2.0"), 3);
        assert_eq!(get("winget:A", "1.9"), 1);
        // Only the failure after B's last success counts.
        assert_eq!(get("winget:B", "5.0"), 1);
        assert_eq!(get("winget:C", ""), 0);
        assert_eq!(get("winget:C", "3.0"), 0);
    }

    #[test]
    fn remembered_technology_mismatches() {
        let failed = |key: &str, kind| HistoryEntry { failure: Some(kind), ..entry(key, JobAction::Upgrade, "2.0", "failed") };
        // Most recent first.
        let entries = vec![
            failed("winget:Microsoft.Edge", FailureKind::TechnologyMismatch),
            failed("winget:A", FailureKind::DownloadFailed),
            failed("winget:A", FailureKind::TechnologyMismatch),
            entry("winget:B", JobAction::Upgrade, "2.0", "succeeded"),
            failed("winget:B", FailureKind::TechnologyMismatch),
        ];
        let found = technology_mismatches(&entries);
        assert!(found.contains("winget:Microsoft.Edge"));
        // Only the most recent attempt counts, and a success clears it.
        assert!(!found.contains("winget:A"));
        assert!(!found.contains("winget:B"));
    }
}
