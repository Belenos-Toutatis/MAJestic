//! Other package managers: Chocolatey, Scoop, Microsoft Store.
//!
//! OWNER: package-managers agent.

pub mod choco;
pub mod scoop;
pub mod store;

use crate::model::{CommandSpec, JobRequest, Source};

/// Builds the command for any job, dispatching on its source.
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    if matches!(job.action, crate::model::JobAction::StoreUpdateScan) {
        return Ok(store::update_scan_command());
    }
    match job.source {
        Source::Winget | Source::MsStore | Source::Registry => crate::winget::build_command(job),
        Source::Chocolatey => choco::build_command(job),
        Source::Scoop => scoop::build_command(job),
        Source::Msix => anyhow::bail!("MSIX apps are updated by the Microsoft Store"),
    }
}
