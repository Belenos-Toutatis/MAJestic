//! MAJestic updating itself (see `majestic_core::selfupdate`): the commands
//! used by the UI, and the start of the new version (`--after-update`).

use std::path::Path;
use std::time::{Duration, Instant};

use majestic_core::selfupdate::{self, Release};
use majestic_core::settings::Settings;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::AppState;

/// At most this many progress events per second.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
/// How long the new version waits for the previous one to exit.
const EXIT_WAIT_MS: u32 = 15_000;

/// Error of `install_app_update`: `kind` is `busy`, `not_checked`,
/// `restart_failed` or a `selfupdate::FailureKind` (`network`,
/// `hash_mismatch`, `version_mismatch`, `write_denied`, `swap_failed`).
#[derive(Debug, Serialize)]
pub struct UpdateError {
    kind: String,
    message: String,
}

impl UpdateError {
    fn new(kind: &str, message: impl Into<String>) -> Self {
        UpdateError { kind: kind.to_string(), message: message.into() }
    }
}

impl From<selfupdate::Failure> for UpdateError {
    fn from(f: selfupdate::Failure) -> Self {
        let kind = serde_json::to_value(f.kind).ok().and_then(|v| v.as_str().map(str::to_string));
        UpdateError { kind: kind.unwrap_or_else(|| "network".into()), message: f.detail }
    }
}

/// Event `app-update-progress`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    /// `download`, `install` (checked, being put in place) or `restart`.
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

/// The newer release on GitHub, if any (never an error: offline or refused
/// requests just mean "nothing new"). Respects the "online sources" setting.
#[tauri::command]
pub async fn check_app_update(state: State<'_, AppState>) -> Result<Option<Release>, String> {
    if !Settings::load().online_intel {
        return Ok(None);
    }
    let found = tauri::async_runtime::spawn_blocking(|| selfupdate::check(majestic_core::APP_VERSION))
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|e| {
            log::info!("update check: {e:#}");
            None
        });
    if let Some(r) = &found {
        log::info!("MAJestic {} is available", r.version);
    }
    if let Ok(mut slot) = state.app_update.lock() {
        slot.clone_from(&found);
    }
    Ok(found)
}

/// Downloads the release found by `check_app_update`, checks it, puts it in
/// place of this executable, starts it and exits. Emits `app-update-progress`.
#[tauri::command]
pub async fn install_app_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), UpdateError> {
    if state.batch.lock().map(|b| b.is_some()).unwrap_or(true) {
        return Err(UpdateError::new("busy", "updates are running"));
    }
    let release = state
        .app_update
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .ok_or_else(|| UpdateError::new("not_checked", "no newer version known"))?;
    let exe = std::env::current_exe().map_err(|e| UpdateError::new("write_denied", e.to_string()))?;

    let emitter = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || install(&emitter, &release, &exe))
        .await
        .map_err(|e| UpdateError::new("network", e.to_string()))?;
    if result.is_ok() {
        app.exit(0);
    }
    result
}

fn install(app: &AppHandle, release: &Release, exe: &Path) -> Result<(), UpdateError> {
    log::info!("updating MAJestic {} → {}", majestic_core::APP_VERSION, release.version);
    let staging = selfupdate::staging_path(exe);
    let mut last = Instant::now() - PROGRESS_INTERVAL;
    selfupdate::download(release, &staging, &mut |downloaded, total| {
        if last.elapsed() >= PROGRESS_INTERVAL || total == Some(downloaded) {
            last = Instant::now();
            let _ = app.emit("app-update-progress", Progress { phase: "download", downloaded, total });
        }
    })
    .inspect_err(|e| log::warn!("update download: {e}"))?;

    let _ = app.emit("app-update-progress", Progress { phase: "install", downloaded: 0, total: None });
    selfupdate::swap(exe, &staging).inspect_err(|e| log::warn!("update swap: {e}"))?;

    let _ = app.emit("app-update-progress", Progress { phase: "restart", downloaded: 0, total: None });
    std::process::Command::new(exe)
        .arg("--after-update")
        .arg(std::process::id().to_string())
        .arg(majestic_core::APP_VERSION)
        .spawn()
        .map_err(|e| {
            log::warn!("cannot start the new version: {e}");
            UpdateError::new("restart_failed", e.to_string())
        })?;
    log::info!("MAJestic {} started, exiting", release.version);
    Ok(())
}

/// `--after-update <pid> <previous version>`: waits for the previous process
/// to exit (it holds the single-instance lock) and returns the previous
/// version, to be announced by the UI.
pub fn after_update(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--after-update")?;
    if let Some(pid) = args.get(i + 1).and_then(|p| p.parse::<u32>().ok()) {
        wait_for_exit(pid);
    }
    args.get(i + 2).cloned()
}

fn wait_for_exit(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};

    // SAFETY: plain Win32 calls on a handle owned here and closed once.
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(handle, EXIT_WAIT_MS);
            let _ = CloseHandle(handle);
        }
    }
}
