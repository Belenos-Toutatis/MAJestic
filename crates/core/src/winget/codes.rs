//! Exit codes of winget / Chocolatey / Scoop → outcome.
//!
//! OWNER: package-managers agent.

use crate::model::{FailureKind, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success { reboot_required: bool },
    Failure(FailureKind),
}

/// Interprets an exit code (unsigned convention, see `exec::Output::code`)
/// for the tool that produced it.
pub fn classify(source: Source, code: i64) -> Outcome {
    let _ = (source, code);
    todo!("codes::classify")
}
