//! Scheduled background check through the Windows Task Scheduler.
//!
//! OWNER: lead (integration).

use std::path::Path;

use crate::settings::Schedule;

/// Creates/updates (or deletes when `None`) the scheduled task running
/// `exe --check`.
pub fn apply(schedule: Option<&Schedule>, exe: &Path) -> anyhow::Result<()> {
    let _ = (schedule, exe);
    todo!("scheduler::apply")
}
