//! winget command-line integration.
//!
//! OWNER: package-managers agent.
//!
//! winget has no machine-readable output for `list`/`upgrade`, so we parse
//! its tables (see [`table`]). Headers are localized (`Nom`, `Disponible`...)
//! and must never be matched by text: columns are identified by position and
//! content.

pub mod codes;
pub mod table;

use crate::model::{CommandSpec, JobRequest};

/// Flags added to every winget call.
pub const COMMON_FLAGS: &[&str] = &["--accept-source-agreements", "--disable-interactivity"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WingetRow {
    /// Display name as installed (Apps & features name).
    pub name: String,
    /// Package id (`Microsoft.VisualStudioCode`) or a pseudo id for
    /// unmatched entries (`ARP\Machine\X64\{GUID}`, `MSIX\...`).
    pub id: String,
    /// Installed version as printed (`1.2.3`, `Unknown`, `< 4.0`).
    pub version: String,
    pub available: Option<String>,
    /// `winget`, `msstore`, another source name, or `None` when unmatched.
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeList {
    /// Main table of `winget upgrade --include-unknown --include-pinned`.
    pub upgrades: Vec<WingetRow>,
    /// Packages that "require explicit targeting" (second table).
    pub explicit: Vec<WingetRow>,
}

/// `winget --version` without the leading `v`, or `None` if winget is missing.
pub fn version() -> Option<String> {
    todo!("winget::version")
}

/// `winget upgrade --include-unknown --include-pinned` (unknown versions only
/// when `include_unknown`).
pub fn list_upgrades(include_unknown: bool) -> anyhow::Result<UpgradeList> {
    let _ = include_unknown;
    todo!("winget::list_upgrades")
}

/// `winget list`: every installed program winget can see, matched or not.
pub fn list_installed() -> anyhow::Result<Vec<WingetRow>> {
    todo!("winget::list_installed")
}

/// Ids of pinned packages (`winget pin list`).
pub fn list_pins() -> anyhow::Result<Vec<String>> {
    todo!("winget::list_pins")
}

/// Every version of a package available in its source, newest first
/// (`winget show --id <id> --exact --versions`).
pub fn list_versions(id: &str, source: Option<&str>) -> anyhow::Result<Vec<String>> {
    let _ = (id, source);
    todo!("winget::list_versions")
}

/// Name search in the `winget` source, at most 10 rows (`winget search`).
pub fn search(query: &str) -> anyhow::Result<Vec<WingetRow>> {
    let _ = query;
    todo!("winget::search")
}

/// Configured source names (`winget source list`), e.g. `["msstore", "winget"]`.
pub fn sources() -> anyhow::Result<Vec<String>> {
    todo!("winget::sources")
}

/// Builds the winget command for a job (upgrade, specific version, forced
/// reinstall, interactive retry). Always `--exact --id`, `--source` when known,
/// `--accept-package-agreements`, `COMMON_FLAGS`, `--silent` (except
/// interactive). Rejects ids with characters outside `[A-Za-z0-9._+-]`.
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    let _ = job;
    todo!("winget::build_command")
}
