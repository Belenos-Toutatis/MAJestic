//! Priority and default selection of an update.
//!
//! OWNER: intel agent.

use crate::model::{Assessment, Reason, UpdateItem};

/// Security-related keywords found in release notes (English and French:
/// `security`, `vulnerability`, `CVE-xxxx-xxxx`, `exploit`, `sécurité`,
/// `faille`, `remote code execution`, ...). Returns the distinct matches as
/// found (CVE ids verbatim), at most 6.
pub fn security_keywords(text: &str) -> Vec<String> {
    let _ = text;
    todo!("assess::security_keywords")
}

/// Computes the assessment of an item whose base fields are filled (kind,
/// versions, certainty, explicit_only, pinned, running, category).
///
/// * Critical: `kev` reason with `recent`, or security keywords in the notes.
/// * Major: major version jump (`version::is_major_jump`).
/// * Recommended: exposed software family, or old (non-recent) KEV history.
/// * Uncertain: installed version unknown/approximate (unless read from file).
/// * Normal otherwise.
///
/// `preselect` is true for Critical/Recommended/Normal automatic updates, false
/// for Major, Uncertain, Manual items, ignored or pinned items.
/// Reasons list everything that applies (also informative ones like
/// `AppRunning`, `RequiresExplicit`, `Pinned`, `VersionFromFile`).
pub fn assess(item: &UpdateItem, kev: Option<Reason>, release_notes: Option<&str>) -> Assessment {
    let _ = (item, kev, release_notes);
    todo!("assess::assess")
}
