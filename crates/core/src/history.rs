//! Update history (`history.jsonl`, one JSON object per line, append-only).

use std::io::Write;

use crate::model::HistoryEntry;
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

/// Deletes the history file.
pub fn clear() -> anyhow::Result<()> {
    let path = paths::history_file();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
