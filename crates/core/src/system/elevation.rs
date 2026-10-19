//! Administrator rights.
//!
//! OWNER: system agent.

use std::ffi::c_void;
use std::path::Path;
use std::time::Duration;

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{
    CheckTokenMembership, CreateWellKnownSid, GetTokenInformation, TokenElevation, TokenElevationType,
    TokenElevationTypeFull, TokenElevationTypeLimited, WinBuiltinAdministratorsSid, PSID, TOKEN_ELEVATION,
    TOKEN_ELEVATION_TYPE, TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, OpenProcessToken, WaitForSingleObject, INFINITE,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SW_HIDE};

use crate::system::{wide, ComGuard};

/// The current process runs elevated.
pub fn is_elevated() -> bool {
    // SAFETY: TOKEN_ELEVATION is the structure TokenElevation returns.
    unsafe { token_information::<TOKEN_ELEVATION>(TokenElevation) }.is_some_and(|e| e.TokenIsElevated != 0)
}

/// The user is a member of Administrators (possibly filtered by UAC).
pub fn is_admin_member() -> bool {
    // A split token (UAC on, administrator) is either the full or the
    // limited half of an administrator's logon.
    // SAFETY: TOKEN_ELEVATION_TYPE is what TokenElevationType returns.
    let kind = unsafe { token_information::<TOKEN_ELEVATION_TYPE>(TokenElevationType) };
    if kind.is_some_and(|k| k == TokenElevationTypeFull || k == TokenElevationTypeLimited) {
        return true;
    }
    // Default type: UAC disabled, built-in Administrator or standard user.
    // SID storage: SECURITY_MAX_SID_SIZE (68) bytes, 4-byte aligned.
    let mut sid = [0u32; 17];
    let mut size = std::mem::size_of_val(&sid) as u32;
    let psid = PSID(sid.as_mut_ptr().cast());
    // SAFETY: `psid` points to `size` writable bytes.
    if unsafe { CreateWellKnownSid(WinBuiltinAdministratorsSid, None, Some(psid), &mut size) }.is_err() {
        return false;
    }
    let mut member = BOOL(0);
    // SAFETY: `psid` holds a valid SID; None = the calling thread's token.
    unsafe { CheckTokenMembership(None, psid, &mut member) }.is_ok() && member.as_bool()
}

/// Reads a fixed-size token information value of the current process.
///
/// # Safety
/// `T` must be the plain C structure that `class` returns.
unsafe fn token_information<T: Default>(class: TOKEN_INFORMATION_CLASS) -> Option<T> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo handle of the current process needs no closing.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    let mut value = T::default();
    let mut returned = 0u32;
    // SAFETY: `value` provides size_of::<T>() writable bytes (caller contract).
    let result = unsafe {
        GetTokenInformation(
            token,
            class,
            Some((&mut value as *mut T).cast::<c_void>()),
            std::mem::size_of::<T>() as u32,
            &mut returned,
        )
    };
    // SAFETY: the token handle was opened above.
    unsafe {
        let _ = CloseHandle(token);
    }
    result.ok().map(|_| value)
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
    ///
    /// When the wait itself fails (invalid handle), the process is reported
    /// as ended with exit code `u32::MAX`, so that callers polling
    /// [`is_running`](Self::is_running) do not loop forever.
    pub fn wait(&self, timeout: Duration) -> Option<u32> {
        let handle = self.raw();
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(INFINITE);
        // SAFETY: waiting on a process handle we own (or a dead value, which fails).
        match unsafe { WaitForSingleObject(handle, ms) } {
            WAIT_OBJECT_0 => {
                let mut code = 0u32;
                // SAFETY: valid handle, `code` is a valid out pointer.
                match unsafe { GetExitCodeProcess(handle, &mut code) } {
                    Ok(()) => Some(code),
                    Err(e) => {
                        log::warn!("GetExitCodeProcess({}) failed: {e}", self.pid);
                        Some(u32::MAX)
                    }
                }
            }
            WAIT_TIMEOUT => None,
            other => {
                log::warn!("waiting for elevated process {} failed: {other:?}", self.pid);
                Some(u32::MAX)
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.wait(Duration::ZERO).is_none()
    }

    fn raw(&self) -> HANDLE {
        HANDLE(self.handle as *mut c_void)
    }
}

impl Drop for ElevatedProcess {
    fn drop(&mut self) {
        if self.handle != 0 {
            // SAFETY: the handle was returned by ShellExecuteExW for us and is
            // closed exactly once.
            unsafe {
                let _ = CloseHandle(self.raw());
            }
        }
    }
}

/// Starts `exe args` with the UAC prompt (`ShellExecuteExW`, verb `runas`,
/// hidden window, `SEE_MASK_NOCLOSEPROCESS`).
pub fn spawn_elevated(exe: &Path, args: &str) -> Result<ElevatedProcess, ElevationError> {
    let _com = ComGuard::new();
    let verb = wide("runas");
    let file = wide(exe);
    let parameters = wide(args);
    let directory = exe.parent().filter(|d| !d.as_os_str().is_empty()).map(wide);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        // Owning the prompt with our foreground window brings it to the front.
        hwnd: own_foreground_window().unwrap_or_default(),
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        lpDirectory: directory.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: every string pointer in `info` stays alive until the call returns.
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
            ElevationError::Refused
        } else {
            ElevationError::Other(e.message())
        });
    }
    if info.hProcess.is_invalid() {
        return Err(ElevationError::Other("no process handle returned".into()));
    }
    // SAFETY: valid process handle returned by ShellExecuteExW.
    let pid = unsafe { GetProcessId(info.hProcess) };
    Ok(ElevatedProcess {
        handle: info.hProcess.0 as isize,
        pid,
    })
}

/// The foreground window when it belongs to this process.
fn own_foreground_window() -> Option<HWND> {
    // SAFETY: plain queries on window handles.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid == std::process::id()).then_some(hwnd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows::Win32::System::Threading::{
        OpenProcess, CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };

    // `spawn_elevated` is deliberately not tested: it would show a UAC prompt.

    #[test]
    fn token_queries() {
        let elevated = is_elevated();
        let admin = is_admin_member();
        if elevated {
            assert!(admin, "an elevated process belongs to Administrators");
        }
    }

    /// Wraps a handle on one of our own (unelevated) child processes.
    fn wrap(child: &std::process::Child) -> ElevatedProcess {
        // SAFETY: plain call; the handle is owned by the returned value.
        let handle = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                child.id(),
            )
        }
        .expect("open child process");
        ElevatedProcess {
            handle: handle.0 as isize,
            pid: child.id(),
        }
    }

    /// Starts `System32\<program> args` hidden.
    fn spawn(program: &str, args: &[&str]) -> std::process::Child {
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        Command::new(Path::new(&windir).join("System32").join(program))
            .args(args)
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cmd.exe")
    }

    #[test]
    fn wait_returns_the_exit_code() {
        let mut child = spawn("cmd.exe", &["/c", "exit 7"]);
        let process = wrap(&child);
        assert_eq!(process.wait(Duration::from_secs(10)), Some(7));
        assert!(!process.is_running());
        let _ = child.wait();
    }

    #[test]
    fn wait_times_out_while_running() {
        let mut child = spawn("PING.EXE", &["-n", "30", "127.0.0.1"]);
        let process = wrap(&child);
        assert_eq!(process.wait(Duration::from_millis(100)), None);
        assert!(process.is_running());
        let _ = child.kill();
        assert_eq!(process.wait(Duration::from_secs(10)), Some(1));
        let _ = child.wait();
    }

    #[test]
    fn invalid_handle_does_not_hang() {
        let process = ElevatedProcess { handle: 0, pid: 0 };
        assert_eq!(process.wait(Duration::from_millis(10)), Some(u32::MAX));
    }
}
