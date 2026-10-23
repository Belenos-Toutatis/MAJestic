//! Running processes: find the ones belonging to a program, close them
//! before its update, relaunch them afterwards.
//!
//! OWNER: system agent.

use std::collections::HashSet;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP,
    DETACHED_PROCESS, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};

use crate::model::RunningProcess;
use crate::system::elevation;
use crate::system::from_wide;
use crate::system::registry::{is_within, norm_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
}

/// Every process whose image path can be read (current user's processes at
/// least), excluding MAJestic itself.
pub fn snapshot() -> Vec<ProcInfo> {
    let own_pid = std::process::id();
    process_entries()
        .into_iter()
        .filter(|(pid, _)| *pid != 0 && *pid != own_pid)
        .filter_map(|(pid, name)| {
            image_path(pid).map(|exe| ProcInfo {
                pid,
                name,
                exe: Some(exe),
            })
        })
        .collect()
}

/// Processes whose executable lives under one of `dirs` (case-insensitive,
/// path-component aware). Fills `has_window` (visible top-level window).
pub fn running_under(dirs: &[PathBuf], procs: &[ProcInfo]) -> Vec<RunningProcess> {
    // A drive root would match every process on the drive: never a program folder.
    let dirs: Vec<String> = dirs
        .iter()
        .map(|d| norm_path(&d.to_string_lossy()))
        .filter(|d| d.len() > 2 && d.contains('\\'))
        .collect();
    let matching: Vec<(&ProcInfo, &PathBuf)> = procs
        .iter()
        .filter_map(|p| p.exe.as_ref().map(|exe| (p, exe)))
        .filter(|(_, exe)| {
            let exe = norm_path(&exe.to_string_lossy());
            dirs.iter().any(|dir| exe.len() > dir.len() && is_within(&exe, dir))
        })
        .collect();
    if matching.is_empty() {
        return Vec::new();
    }
    let with_window: HashSet<u32> = top_level_windows()
        .into_iter()
        .filter(|w| w.visible)
        .map(|w| w.pid)
        .collect();
    matching
        .into_iter()
        .map(|(p, exe)| RunningProcess {
            pid: p.pid,
            name: p.name.clone(),
            exe: exe.to_string_lossy().into_owned(),
            has_window: with_window.contains(&p.pid),
        })
        .collect()
}

/// Politely closes processes (WM_CLOSE to all their top-level windows,
/// including hidden tray windows), waits up to `grace`, then terminates the
/// survivors if `force`. Returns the pids still alive.
pub fn close(pids: &[u32], grace: Duration, force: bool) -> Vec<u32> {
    let own_pid = std::process::id();
    let mut unique = Vec::new();
    for &pid in pids {
        if !unique.contains(&pid) {
            unique.push(pid);
        }
    }

    let mut untouchable = Vec::new();
    let mut targets: Vec<(u32, Option<ProcessHandle>)> = Vec::new();
    for pid in unique {
        if pid == 0 || pid == 4 || pid == own_pid {
            untouchable.push(pid);
            continue;
        }
        let handle = ProcessHandle::open(
            pid,
            PROCESS_SYNCHRONIZE | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
        )
        .or_else(|_| ProcessHandle::open(pid, PROCESS_SYNCHRONIZE));
        match handle {
            Ok(h) if h.has_exited(Duration::ZERO) => {}
            Ok(h) => targets.push((pid, Some(h))),
            Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => {} // no such process
            // Access denied: WM_CLOSE may still work, then poll for the pid.
            Err(_) => targets.push((pid, None)),
        }
    }

    // Ask nicely, like `taskkill` without /F.
    let target_pids: HashSet<u32> = targets.iter().map(|(pid, _)| *pid).collect();
    for window in top_level_windows().into_iter().filter(|w| target_pids.contains(&w.pid)) {
        // SAFETY: posting a message to a window handle that may have died
        // meanwhile is harmless (the call just fails).
        let _ = unsafe { PostMessageW(Some(window.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }

    // Wait for them, all within the same `grace` period.
    let deadline = Instant::now() + grace;
    for (_, handle) in &targets {
        if let Some(h) = handle {
            h.has_exited(deadline.saturating_duration_since(Instant::now()));
        }
    }
    let unwaitable: Vec<u32> = targets
        .iter()
        .filter(|(_, h)| h.is_none())
        .map(|(pid, _)| *pid)
        .collect();
    while !unwaitable.is_empty() && Instant::now() < deadline && unwaitable.iter().any(|&pid| pid_exists(pid)) {
        std::thread::sleep(Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())));
    }

    let mut alive: HashSet<u32> = untouchable.into_iter().collect();
    for (pid, handle) in targets {
        let running = match &handle {
            Some(h) => !h.has_exited(Duration::ZERO),
            None => pid_exists(pid),
        };
        if !running {
            continue;
        }
        if force {
            if let Some(h) = &handle {
                if h.terminate() && h.has_exited(Duration::from_secs(5)) {
                    continue;
                }
            }
        }
        alive.insert(pid);
    }
    let mut result: Vec<u32> = Vec::new();
    for &pid in pids {
        if alive.contains(&pid) && !result.contains(&pid) {
            result.push(pid);
        }
    }
    result
}

/// Starts a program detached, in its own folder, **never elevated**: when
/// MAJestic runs elevated the launch goes through the shell (explorer) so the
/// program gets the user's normal rights.
pub fn launch_unelevated(exe: &Path) -> anyhow::Result<()> {
    if !exe.is_file() {
        anyhow::bail!("cannot launch {}: file not found", exe.display());
    }
    if elevation::is_elevated() {
        // A new explorer.exe hands the request to the running (unelevated)
        // shell, which starts the program with the user's normal token.
        let windir = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        Command::new(windir.join("explorer.exe"))
            .raw_arg(format!("\"{}\"", exe.display()))
            .spawn()
            .map_err(|e| anyhow::anyhow!("cannot launch {} through explorer: {e}", exe.display()))?;
        return Ok(());
    }
    let mut command = Command::new(exe);
    if let Some(dir) = exe.parent().filter(|d| !d.as_os_str().is_empty()) {
        command.current_dir(dir);
    }
    command
        .creation_flags(DETACHED_PROCESS.0 | CREATE_NEW_PROCESS_GROUP.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("cannot launch {}: {e}", exe.display()))?;
    Ok(())
}

/// Starts `exe arg` detached. Refused when MAJestic runs elevated: the
/// program would get administrator rights (the shell route of
/// [`launch_unelevated`] cannot pass an argument).
pub fn launch_with_arg(exe: &Path, arg: &str) -> anyhow::Result<()> {
    if !exe.is_file() {
        anyhow::bail!("cannot launch {}: file not found", exe.display());
    }
    if elevation::is_elevated() {
        anyhow::bail!("elevated");
    }
    let mut command = Command::new(exe);
    if let Some(dir) = exe.parent().filter(|d| !d.as_os_str().is_empty()) {
        command.current_dir(dir);
    }
    command
        .arg(arg)
        .creation_flags(DETACHED_PROCESS.0 | CREATE_NEW_PROCESS_GROUP.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("cannot launch {}: {e}", exe.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A process handle closed on drop.
struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> windows::core::Result<ProcessHandle> {
        // SAFETY: plain call; the handle is closed in Drop.
        unsafe { OpenProcess(access, false, pid) }.map(ProcessHandle)
    }

    /// Waits up to `timeout` for the process to end.
    fn has_exited(&self, timeout: Duration) -> bool {
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: the handle is valid for our lifetime and has SYNCHRONIZE.
        let event = unsafe { WaitForSingleObject(self.0, ms) };
        event == WAIT_OBJECT_0
    }

    fn terminate(&self) -> bool {
        // SAFETY: valid handle; fails harmlessly without PROCESS_TERMINATE.
        unsafe { TerminateProcess(self.0, 1) }.is_ok()
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// (pid, executable file name) of every process.
fn process_entries() -> Vec<(u32, String)> {
    // SAFETY: plain call; the snapshot handle is closed below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while more {
        entries.push((entry.th32ProcessID, from_wide(&entry.szExeFile)));
        // SAFETY: same snapshot and entry.
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    // SAFETY: the snapshot handle is ours and closed once.
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    entries
}

fn pid_exists(pid: u32) -> bool {
    process_entries().iter().any(|(p, _)| *p == pid)
}

fn image_path(pid: u32) -> Option<PathBuf> {
    let process = ProcessHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    for capacity in [1024usize, 32 * 1024] {
        let mut buf = vec![0u16; capacity];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` u16; `len` receives the length written.
        let ok =
            unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
        if ok.is_ok() {
            let path = String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]);
            return (!path.is_empty()).then(|| PathBuf::from(path));
        }
    }
    None
}

struct TopLevelWindow {
    hwnd: HWND,
    pid: u32,
    visible: bool,
}

/// Every top-level window (visible or not) with its process id.
fn top_level_windows() -> Vec<TopLevelWindow> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the address of the Vec passed by
        // `top_level_windows`, alive and exclusively borrowed during EnumWindows.
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<TopLevelWindow>) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` comes from EnumWindows; `pid` is a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        // SAFETY: plain query on a window handle.
        let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        windows.push(TopLevelWindow { hwnd, pid, visible });
        BOOL(1)
    }

    let mut windows: Vec<TopLevelWindow> = Vec::new();
    // SAFETY: the callback only touches `windows` through `lparam`, and
    // EnumWindows returns before `windows` goes out of scope.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut windows as *mut Vec<TopLevelWindow> as isize)) };
    windows
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    /// A copy of `ping.exe` sleeping for a minute, in its own folder. Only
    /// this process is ever closed by the tests; it is killed on drop.
    struct Dummy {
        child: std::process::Child,
        dir: PathBuf,
        exe: PathBuf,
    }

    /// A private folder holding a copy of ping.exe: (folder, exe).
    fn dummy_exe(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("majestic-procs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dummy dir");
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let exe = dir.join("majestic-dummy.exe");
        std::fs::copy(Path::new(&windir).join(r"System32\PING.EXE"), &exe).expect("copy ping.exe");
        (dir, exe)
    }

    impl Dummy {
        fn spawn(tag: &str) -> Dummy {
            let (dir, exe) = dummy_exe(tag);
            let child = Command::new(&exe)
                .args(["-n", "60", "127.0.0.1"])
                .creation_flags(CREATE_NO_WINDOW.0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn dummy");
            std::thread::sleep(Duration::from_millis(200));
            Dummy { child, dir, exe }
        }

        fn pid(&self) -> u32 {
            self.child.id()
        }

        fn is_running(&mut self) -> bool {
            matches!(self.child.try_wait(), Ok(None))
        }
    }

    impl Drop for Dummy {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn snapshot_sees_our_dummy_but_not_us() {
        let mut dummy = Dummy::spawn("snapshot");
        assert!(dummy.is_running(), "dummy process exited early");
        let procs = snapshot();
        assert!(!procs.iter().any(|p| p.pid == std::process::id()));
        let found = procs.iter().find(|p| p.pid == dummy.pid()).expect("dummy in snapshot");
        assert_eq!(found.name.to_lowercase(), "majestic-dummy.exe");
        let exe = found.exe.as_ref().expect("exe path");
        assert_eq!(
            norm_path(&exe.to_string_lossy()),
            norm_path(&dummy.exe.to_string_lossy())
        );
    }

    #[test]
    fn running_under_matches_path_components() {
        let dummy = Dummy::spawn("under");
        let procs = snapshot();
        let dir_text = dummy.dir.to_string_lossy().into_owned();

        let found = running_under(&[PathBuf::from(dir_text.to_uppercase())], &procs);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].pid, dummy.pid());
        assert!(!found[0].has_window);

        // Same prefix, different folder.
        let truncated = PathBuf::from(&dir_text[..dir_text.len() - 1]);
        assert!(running_under(&[truncated], &procs).is_empty());
        assert!(running_under(&[dummy.dir.join("sub")], &procs).is_empty());
        assert!(running_under(&[PathBuf::from(r"C:\")], &procs).is_empty());
        assert!(running_under(&[], &procs).is_empty());
    }

    #[test]
    fn close_terminates_only_when_forced() {
        let mut dummy = Dummy::spawn("close");
        let pid = dummy.pid();
        // No window to receive WM_CLOSE: still alive after the grace period.
        assert_eq!(close(&[pid, pid], Duration::from_millis(300), false), vec![pid]);
        assert!(dummy.is_running());
        assert!(close(&[pid], Duration::from_millis(300), true).is_empty());
        assert!(!dummy.is_running());
        // Our own process is never touched.
        assert_eq!(
            close(&[std::process::id()], Duration::ZERO, true),
            vec![std::process::id()]
        );
    }

    #[test]
    fn launch_checks_the_file() {
        assert!(launch_unelevated(Path::new(r"C:\does\not\exist.exe")).is_err());
        if !elevation::is_elevated() {
            // ping without arguments prints its usage and exits at once.
            let (dir, exe) = dummy_exe("launch");
            assert!(launch_unelevated(&exe).is_ok());
            std::thread::sleep(Duration::from_millis(500));
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
