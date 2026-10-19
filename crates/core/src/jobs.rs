//! Executes one installation job and records it in the history.
//!
//! OWNER: lead (integration).

use crate::model::{JobEvent, JobRequest, JobState};

/// Runs `job` to completion, emitting progress events. Never panics.
pub fn run_job(job: &JobRequest, emit: &mut dyn FnMut(JobEvent)) -> JobState {
    let _ = (job, emit);
    todo!("jobs::run_job")
}
