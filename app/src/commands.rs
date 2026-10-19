//! Tauri commands: the UI's API (see docs/ARCHITECTURE.md).

use std::collections::HashMap;
use std::sync::Mutex;

use majestic_core::eta::EtaModel;
use majestic_core::model::*;
use majestic_core::settings::Settings;
use majestic_core::system::{elevation, icons, procs, shell};
use majestic_core::{history, paths, pkgmgr, scan, scheduler, winget};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{batch, notify};

#[derive(Default)]
pub struct AppState {
    /// Last scan result, kept up to date with `item-updated`.
    pub last_scan: Mutex<Option<ScanResult>>,
    pub icons: Mutex<HashMap<String, Option<String>>>,
    /// Some while a batch of updates runs.
    pub batch: Mutex<Option<batch::BatchControl>>,
}

/// Runs blocking work off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    app_version: String,
    ui_language: String,
    elevated: bool,
    admin_member: bool,
    data_dir: String,
}

#[tauri::command]
pub async fn app_info() -> Result<AppInfo, String> {
    blocking(|| AppInfo {
        app_version: majestic_core::APP_VERSION.to_string(),
        ui_language: shell::ui_language(),
        elevated: elevation::is_elevated(),
        admin_member: elevation::is_admin_member(),
        data_dir: paths::data_dir().to_string_lossy().into_owned(),
    })
    .await
}

#[tauri::command]
pub fn get_settings() -> Settings {
    Settings::load()
}

fn current_exe() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Saves settings and applies the scheduled check. If the scheduler refuses,
/// the schedule is turned off and the error is returned.
#[tauri::command]
pub async fn save_settings(settings: Settings) -> Result<Settings, String> {
    blocking(move || {
        let previous = Settings::load();
        let mut settings = settings;
        settings.save().map_err(|e| format!("{e:#}"))?;
        if previous.schedule != settings.schedule {
            let exe = current_exe()?;
            let applied = scheduler::apply(settings.schedule.as_ref(), &exe).and_then(|_| match &settings.schedule {
                Some(_) => notify::register(&exe),
                None => notify::unregister(),
            });
            if let Err(e) = applied {
                log::warn!("schedule: {e:#}");
                settings.schedule = None;
                let _ = settings.save();
                return Err(format!("{e:#}"));
            }
        }
        Ok(settings)
    })
    .await?
}

/// The executable may have moved since the task was created (portable app):
/// re-register the task with the current path.
pub fn refresh_schedule_path() {
    std::thread::spawn(|| {
        let settings = Settings::load();
        if let (Some(schedule), Ok(exe)) = (&settings.schedule, std::env::current_exe()) {
            if let Err(e) = scheduler::apply(Some(schedule), &exe).and_then(|_| notify::register(&exe)) {
                log::warn!("schedule refresh: {e:#}");
            }
        }
    });
}

#[tauri::command]
pub async fn scan(app: AppHandle) -> Result<ScanResult, String> {
    let handle = app.clone();
    let result = blocking(move || {
        let settings = Settings::load();
        let emit = |p: ScanProgress| {
            let _ = handle.emit("scan-progress", p);
        };
        scan::scan(&settings, &emit)
    })
    .await?;

    let state = app.state::<AppState>();
    if let Ok(mut last) = state.last_scan.lock() {
        *last = Some(result.clone());
    }

    let settings = Settings::load();
    if settings.lookup_unmanaged && result.environment.online {
        let items = result.items.clone();
        let handle = app.clone();
        std::thread::spawn(move || {
            let progress = |p: ScanProgress| {
                let _ = handle.emit("scan-progress", p);
            };
            let on_item = |mut item: UpdateItem| {
                let settings = Settings::load();
                item.ignored = settings.is_ignored(&item.key, item.available_version.as_deref());
                if let Ok(mut last) = handle.state::<AppState>().last_scan.lock() {
                    if let Some(scan) = last.as_mut() {
                        if let Some(slot) = scan.items.iter_mut().find(|i| i.key == item.key) {
                            *slot = item.clone();
                        }
                    }
                }
                let _ = handle.emit("item-updated", item);
            };
            scan::lookup_unmanaged(&items, &progress, &on_item);
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_icon(hint: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    if let Some(cached) = state.icons.lock().ok().and_then(|c| c.get(&hint).cloned()) {
        return Ok(cached);
    }
    let key = hint.clone();
    let icon = blocking(move || icons::icon_data_url(&hint, 32)).await?;
    if let Ok(mut cache) = state.icons.lock() {
        cache.insert(key, icon.clone());
    }
    Ok(icon)
}

#[tauri::command]
pub async fn list_versions(id: String, source: String) -> Result<Vec<String>, String> {
    let source = match source.as_str() {
        "winget" => "winget",
        "msstore" | "ms_store" => "msstore",
        "chocolatey" => "chocolatey",
        other => return Err(format!("versions are not available for source {other}")),
    };
    blocking(move || {
        let versions = if source == "chocolatey" {
            pkgmgr::choco::list_versions(&id)
        } else {
            winget::list_versions(&id, Some(source))
        };
        versions.map_err(|e| format!("{e:#}"))
    })
    .await?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    name: String,
    id: String,
    version: String,
    source: Option<String>,
}

#[tauri::command]
pub async fn search_winget(query: String) -> Result<Vec<SearchHit>, String> {
    blocking(move || {
        winget::search(&query)
            .map(|rows| {
                rows.into_iter()
                    .map(|r| SearchHit { name: r.name, id: r.id, version: r.version, source: r.source })
                    .collect()
            })
            .map_err(|e| format!("{e:#}"))
    })
    .await?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    key: String,
    name: String,
    processes: Vec<RunningProcess>,
}

/// Processes currently running for the given items (fresh check).
#[tauri::command]
pub async fn running_apps(keys: Vec<String>, state: State<'_, AppState>) -> Result<Vec<RunningApp>, String> {
    let targets: Vec<(String, String, Vec<std::path::PathBuf>)> = state
        .last_scan
        .lock()
        .ok()
        .and_then(|s| {
            s.as_ref().map(|scan| {
                scan.items
                    .iter()
                    .filter(|i| keys.contains(&i.key) && !i.install_dirs.is_empty())
                    .map(|i| (i.key.clone(), i.name.clone(), i.install_dirs.clone()))
                    .collect()
            })
        })
        .unwrap_or_default();
    blocking(move || {
        let snapshot = procs::snapshot();
        targets
            .into_iter()
            .filter_map(|(key, name, dirs)| {
                let processes = procs::running_under(&dirs, &snapshot);
                (!processes.is_empty()).then_some(RunningApp { key, name, processes })
            })
            .collect()
    })
    .await
}

#[tauri::command]
pub async fn estimate_jobs(jobs: Vec<JobRequest>) -> Result<Vec<JobEstimate>, String> {
    blocking(move || {
        let model = EtaModel::from_history(&history::load());
        jobs.iter().map(|j| model.estimate(j)).collect()
    })
    .await
}

#[tauri::command]
pub fn start_updates(
    app: AppHandle,
    jobs: Vec<JobRequest>,
    close_apps: Vec<String>,
    allow_elevation: bool,
) -> Result<(), String> {
    batch::start(app, jobs, close_apps, allow_elevation)
}

#[tauri::command]
pub fn cancel_updates(state: State<'_, AppState>) {
    if let Ok(batch) = state.batch.lock() {
        if let Some(b) = batch.as_ref() {
            b.cancel();
        }
    }
}

#[tauri::command]
pub async fn get_history() -> Result<Vec<HistoryEntry>, String> {
    blocking(history::load).await
}

#[tauri::command]
pub fn clear_history() -> Result<(), String> {
    history::clear().map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    shell::open_url(&url).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn open_store_updates() -> Result<(), String> {
    shell::open_url(pkgmgr::store::STORE_UPDATES_URI).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn open_logs() -> Result<(), String> {
    shell::reveal(&paths::logs_dir()).map_err(|e| format!("{e:#}"))
}
