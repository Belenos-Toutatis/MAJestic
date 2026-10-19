//! Scoop, when installed (user-level, must never run elevated).
//!
//! OWNER: package-managers agent.

use crate::model::{CommandSpec, JobRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoopRow {
    pub app: String,
    pub installed: String,
    pub available: String,
}

/// Is Scoop installed for this user?
pub fn detect() -> bool {
    todo!("scoop::detect")
}

/// Refreshes buckets then lists outdated apps (`scoop status`).
pub fn status() -> anyhow::Result<Vec<ScoopRow>> {
    todo!("scoop::status")
}

/// `scoop update <app>` (or `scoop install <app>@<version>` for a version).
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    let _ = job;
    todo!("scoop::build_command")
}
