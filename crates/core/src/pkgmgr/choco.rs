//! Chocolatey (`choco`), when installed.
//!
//! OWNER: package-managers agent.

use crate::model::{CommandSpec, JobRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChocoRow {
    pub id: String,
    pub installed: String,
    pub available: String,
    pub pinned: bool,
}

/// Chocolatey version, or `None` when choco is not installed.
pub fn version() -> Option<String> {
    todo!("choco::version")
}

/// `choco outdated -r --ignore-unfound` (`id|installed|available|pinned`).
pub fn outdated() -> anyhow::Result<Vec<ChocoRow>> {
    todo!("choco::outdated")
}

/// `choco upgrade <id> -y --no-progress` (or `--version <v> --allow-downgrade`).
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    let _ = job;
    todo!("choco::build_command")
}
