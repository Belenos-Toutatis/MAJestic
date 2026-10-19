//! Running processes: find the ones belonging to a program, close them
//! before its update, relaunch them afterwards.
//!
//! OWNER: system agent.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::RunningProcess;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
}

/// Every process whose image path can be read (current user's processes at
/// least), excluding MAJestic itself.
pub fn snapshot() -> Vec<ProcInfo> {
    todo!("procs::snapshot")
}

/// Processes whose executable lives under one of `dirs` (case-insensitive,
/// path-component aware). Fills `has_window` (visible top-level window).
pub fn running_under(dirs: &[PathBuf], procs: &[ProcInfo]) -> Vec<RunningProcess> {
    let _ = (dirs, procs);
    todo!("procs::running_under")
}

/// Politely closes processes (WM_CLOSE to all their top-level windows,
/// including hidden tray windows), waits up to `grace`, then terminates the
/// survivors if `force`. Returns the pids still alive.
pub fn close(pids: &[u32], grace: Duration, force: bool) -> Vec<u32> {
    let _ = (pids, grace, force);
    todo!("procs::close")
}

/// Starts a program detached, in its own folder, **never elevated**: when
/// MAJestic runs elevated the launch goes through the shell (explorer) so the
/// program gets the user's normal rights.
pub fn launch_unelevated(exe: &Path) -> anyhow::Result<()> {
    let _ = exe;
    todo!("procs::launch_unelevated")
}
