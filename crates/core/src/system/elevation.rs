//! Administrator rights.
//!
//! OWNER: system agent.

use std::path::Path;
use std::time::Duration;

/// The current process runs elevated.
pub fn is_elevated() -> bool {
    todo!("elevation::is_elevated")
}

/// The user is a member of Administrators (possibly filtered by UAC).
pub fn is_admin_member() -> bool {
    todo!("elevation::is_admin_member")
}

#[derive(Debug)]
pub enum ElevationError {
    /// The user answered "No" to the UAC prompt.
    Refused,
    Other(String),
}

impl std::fmt::Display for ElevationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElevationError::Refused => write!(f, "elevation refused by the user"),
            ElevationError::Other(e) => write!(f, "elevation failed: {e}"),
        }
    }
}

impl std::error::Error for ElevationError {}

/// Handle on a process started elevated.
pub struct ElevatedProcess {
    /// Raw process HANDLE value (closed on drop).
    pub handle: isize,
    pub pid: u32,
}

impl ElevatedProcess {
    /// Waits for the process; `None` on timeout, else its exit code.
    pub fn wait(&self, timeout: Duration) -> Option<u32> {
        let _ = timeout;
        todo!("ElevatedProcess::wait")
    }

    pub fn is_running(&self) -> bool {
        self.wait(Duration::ZERO).is_none()
    }
}

impl Drop for ElevatedProcess {
    fn drop(&mut self) {
        // system agent: CloseHandle(self.handle)
    }
}

/// Starts `exe args` with the UAC prompt (`ShellExecuteExW`, verb `runas`,
/// hidden window, `SEE_MASK_NOCLOSEPROCESS`).
pub fn spawn_elevated(exe: &Path, args: &str) -> Result<ElevatedProcess, ElevationError> {
    let _ = (exe, args);
    todo!("elevation::spawn_elevated")
}
