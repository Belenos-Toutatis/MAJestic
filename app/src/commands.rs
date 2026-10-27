//! Tauri commands: the UI's API (see docs/ARCHITECTURE.md).

use std::collections::HashMap;
use std::sync::Mutex;

use majestic_core::eta::EtaModel;
use majestic_core::intel::about::{self, About, AboutRequest};
use majestic_core::model::*;
use majestic_core::settings::Settings;
use majestic_core::system::{elevation, filedialog, icons, procs, registry, shell};
use majestic_core::{cleanup, history, inventory, paths, pkgmgr, scan, scheduler, winget};
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
    /// Newer MAJestic found by `check_app_update`.
    pub app_update: Mutex<Option<majestic_core::selfupdate::Release>>,
    /// Previous version when this process was started by an update.
    pub updated_from: Option<String>,
    /// Last answer of Windows Update (searched after each scan).
    pub windows_updates: Mutex<Option<WindowsUpdates>>,
    /// File written by the last `export_inventory`.
    pub last_export: Mutex<Option<std::path::PathBuf>>,
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
    /// Set right after MAJestic updated itself (previous version).
    updated_from: Option<String>,
    /// `missing` (scheduled check turned off: its task was removed) or
    /// `moved` (the task runs another copy of MAJestic).
    schedule_notice: Option<String>,
    /// May update itself automatically: not a development build (debug, or
    /// run from cargo's `target` folder).
    auto_updatable: bool,
}

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> Result<AppInfo, String> {
    let updated_from = state.updated_from.clone();
    blocking(move || AppInfo {
        app_version: majestic_core::APP_VERSION.to_string(),
        auto_updatable: !cfg!(debug_assertions)
            && !std::env::current_exe().is_ok_and(|e| e.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target"))),
        ui_language: shell::ui_language(),
        elevated: elevation::is_elevated(),
        admin_member: elevation::is_admin_member(),
        data_dir: paths::data_dir().to_string_lossy().into_owned(),
        updated_from,
        schedule_notice: check_schedule().map(str::to_string),
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
        // The schedule is only changed by `set_schedule` (its buttons), so a
        // stale copy sent here can never write the task.
        let mut settings = settings;
        settings.schedule = Settings::load().schedule;
        settings.save().map_err(|e| format!("{e:#}"))?;
        Ok(settings)
    })
    .await?
}

/// State of the scheduled check, for the settings' summary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    schedule: Option<majestic_core::settings::Schedule>,
    /// `ok`, `missing` (option on, no task), `other` (the task runs another
    /// copy of MAJestic), `unreadable`, or `none` (option off).
    task: &'static str,
    /// Program run by the task, when it exists.
    task_command: Option<String>,
    /// This copy of MAJestic.
    exe: String,
    /// `downloads` / `temp`: this copy sits in a folder that is often emptied.
    location_warning: Option<&'static str>,
    /// Next run (local time, `YYYY-MM-DDTHH:MM:SS`).
    next_run: Option<String>,
    last_check: Option<scheduler::LastCheck>,
}

fn schedule_status_now() -> Result<ScheduleStatus, String> {
    let settings = Settings::load();
    let exe = current_exe()?;
    let (task, task_command) = match scheduler::registered_task() {
        Ok(scheduler::TaskState::Runs(cmd)) => {
            let state = if scheduler::same_command(&cmd, &exe) { "ok" } else { "other" };
            (state, Some(cmd.to_string_lossy().into_owned()))
        }
        Ok(scheduler::TaskState::Missing) => (if settings.schedule.is_some() { "missing" } else { "none" }, None),
        Ok(scheduler::TaskState::Unreadable) => ("unreadable", None),
        Err(e) => {
            log::warn!("schedule status: {e:#}");
            ("unreadable", None)
        }
    };
    let next_run = settings
        .schedule
        .as_ref()
        .filter(|_| task == "ok")
        .and_then(|s| scheduler::next_run(s, chrono::Local::now().naive_local()))
        .map(|at| at.format("%Y-%m-%dT%H:%M:%S").to_string());
    Ok(ScheduleStatus {
        schedule: settings.schedule.clone(),
        task,
        task_command,
        location_warning: paths::volatile_location(&exe),
        exe: exe.to_string_lossy().into_owned(),
        next_run,
        last_check: scheduler::load_last_check(),
    })
}

/// Texts of a file dialog, in the interface's language.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogText {
    title: String,
    filter_name: String,
    /// Proposed file name (save only).
    #[serde(default)]
    file_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    path: String,
    packages: usize,
    manual: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    file: String,
    plan: inventory::ImportPlan,
}

fn scanned_items(state: &AppState) -> Result<Vec<UpdateItem>, String> {
    state
        .last_scan
        .lock()
        .ok()
        .and_then(|s| s.as_ref().map(|scan| scan.items.clone()))
        .ok_or_else(|| "no_scan".to_string())
}

fn owner_window(window: &tauri::WebviewWindow) -> isize {
    window.hwnd().map(|h| h.0 as isize).unwrap_or(0)
}

/// Saves the list of programs found by the last scan to a file chosen in
/// "Save as" (`null` when cancelled).
#[tauri::command]
pub async fn export_inventory(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    text: DialogText,
) -> Result<Option<Exported>, String> {
    let items = scanned_items(&state)?;
    let owner = owner_window(&window);
    let exported = blocking(move || -> Result<Option<(std::path::PathBuf, Exported)>, String> {
        let mut inv = inventory::build(&items);
        // The scan lists only their outdated packages.
        let settings = Settings::load();
        inventory::add_installed(
            &mut inv,
            if settings.use_chocolatey { pkgmgr::choco::installed() } else { Vec::new() },
            if settings.use_scoop { pkgmgr::scoop::installed() } else { Vec::new() },
        );
        let folder = inventory::default_folder();
        let dialog = filedialog::Dialog {
            owner,
            title: &text.title,
            folder: folder.as_deref(),
            file_name: &text.file_name,
            filter: (&text.filter_name, "*.json"),
            extension: "json",
        };
        let Some(path) = filedialog::save(&dialog).map_err(|e| format!("{e:#}"))? else {
            return Ok(None);
        };
        let json = serde_json::to_vec_pretty(&inv).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
        log::info!("inventory: {} packages, {} other programs saved", inv.packages.len(), inv.manual.len());
        let summary = Exported { path: path.display().to_string(), packages: inv.packages.len(), manual: inv.manual.len() };
        Ok(Some((path, summary)))
    })
    .await??;
    Ok(exported.map(|(path, summary)| {
        if let Ok(mut last) = state.last_export.lock() {
            *last = Some(path);
        }
        summary
    }))
}

/// Opens the page where a self-updating program (a browser) updates itself.
/// Error `elevated` when MAJestic runs as administrator.
#[tauri::command]
pub async fn open_self_update(key: String, state: State<'_, AppState>) -> Result<(), String> {
    let item = scanned_items(&state)?.into_iter().find(|i| i.key == key).ok_or("unknown program")?;
    blocking(move || {
        let updater = majestic_core::self_updaters::for_id(&item.id).ok_or("no update page")?;
        majestic_core::self_updaters::open(updater, item.install_location.as_deref()).map_err(|e| format!("{e:#}"))
    })
    .await?
}

/// Shows the last exported list in Explorer.
#[tauri::command]
pub fn reveal_export(state: State<'_, AppState>) -> Result<(), String> {
    let path = state.last_export.lock().ok().and_then(|p| p.clone()).ok_or("nothing exported")?;
    shell::reveal(&path).map_err(|e| format!("{e:#}"))
}

/// Reads a list chosen in "Open" and compares it with the last scan:
/// what can be installed, what is already here (`null` when cancelled;
/// errors `no_scan`, `invalid_file`).
#[tauri::command]
pub async fn import_inventory(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    text: DialogText,
) -> Result<Option<Imported>, String> {
    let items = scanned_items(&state)?;
    let owner = owner_window(&window);
    blocking(move || {
        let folder = inventory::default_folder();
        let dialog = filedialog::Dialog {
            owner,
            title: &text.title,
            folder: folder.as_deref(),
            file_name: "",
            filter: (&text.filter_name, "*.json"),
            extension: "json",
        };
        let Some(path) = filedialog::open(&dialog).map_err(|e| format!("{e:#}"))? else {
            return Ok(None);
        };
        let invalid = |e: String| {
            log::info!("inventory: {}: {e}", path.display());
            "invalid_file".to_string()
        };
        let size = std::fs::metadata(&path).map_err(|e| invalid(e.to_string()))?.len();
        if size > inventory::MAX_FILE {
            return Err(invalid(format!("{size} bytes")));
        }
        let content = std::fs::read_to_string(&path).map_err(|e| invalid(e.to_string()))?;
        let inv = inventory::parse(&content).map_err(|e| invalid(format!("{e:#}")))?;
        Ok(Some(Imported { file: path.display().to_string(), plan: inventory::plan(&inv, &items) }))
    })
    .await?
}

/// Installers left on disk by winget, Chocolatey and Scoop.
#[tauri::command]
pub async fn leftover_downloads() -> Result<cleanup::Leftovers, String> {
    blocking(|| cleanup::measure(&cleanup::download_dirs())).await
}

/// Removes them; refused (`busy`) while MAJestic or a package manager
/// is installing something.
#[tauri::command]
pub async fn clean_downloads(state: State<'_, AppState>) -> Result<cleanup::Cleaned, String> {
    if state.batch.lock().map(|b| b.is_some()).unwrap_or(true) {
        return Err("busy".into());
    }
    blocking(|| {
        let busy = procs::snapshot()
            .iter()
            .any(|p| cleanup::BUSY_PROCESSES.iter().any(|b| p.name.eq_ignore_ascii_case(b)));
        if busy {
            return Err("busy".to_string());
        }
        Ok(cleanup::clean(&cleanup::download_dirs()))
    })
    .await?
}

/// What a program does, for the details panel (`null` when nobody says).
/// Online catalogs only with "online sources" on, and for programs no
/// package manager knows, only with "look up unmanaged programs" on too.
#[tauri::command]
pub async fn describe_item(request: AboutRequest) -> Result<Option<About>, String> {
    blocking(move || {
        let s = Settings::load();
        let online = s.online_intel && (request.source != Some(Source::Registry) || s.lookup_unmanaged);
        about::about(&request, &paths::cache_dir(), online)
    })
    .await
}

#[tauri::command]
pub async fn schedule_status() -> Result<ScheduleStatus, String> {
    blocking(schedule_status_now).await?
}

/// Turns the scheduled check on (or updates it) for this copy of MAJestic, or
/// off when `schedule` is null: the only place where the task is written,
/// always on the user's request.
#[tauri::command]
pub async fn set_schedule(schedule: Option<majestic_core::settings::Schedule>) -> Result<ScheduleStatus, String> {
    blocking(move || {
        let exe = current_exe()?;
        scheduler::apply(schedule.as_ref(), &exe).map_err(|e| format!("{e:#}"))?;
        let registered = match schedule {
            Some(_) => notify::register(&exe),
            None => notify::unregister(),
        };
        if let Err(e) = registered {
            log::warn!("schedule: notification registration: {e:#}");
        }
        let mut settings = Settings::load();
        settings.schedule = schedule;
        settings.save().map_err(|e| format!("{e:#}"))?;
        log::info!("schedule: {}", if settings.schedule.is_some() { "turned on" } else { "turned off" });
        schedule_status_now()
    })
    .await?
}

/// Checks the scheduled check at start, without ever (re)creating its task:
/// Microsoft Defender treats an unknown program that installs a task
/// restarting it as malware persistence (Behavior:Win32/Persistence.A!ml).
/// The task is only written when the user changes the option.
/// * task gone (removed by hand or by an antivirus): the option is turned off;
/// * task running another copy of MAJestic: nothing changes (settings are
///   shared by every copy), the UI just says so.
fn check_schedule() -> Option<&'static str> {
    let mut settings = Settings::load();
    settings.schedule.as_ref()?;
    let exe = std::env::current_exe().ok()?;
    match scheduler::registered_task() {
        Ok(scheduler::TaskState::Runs(cmd)) if scheduler::same_command(&cmd, &exe) => {
            // This copy is the scheduled one: its notifications must open it
            // (the link may point to a copy that was moved or deleted).
            if !notify::registered_exe().is_some_and(|r| scheduler::same_command(&r, &exe)) {
                log::info!("schedule: the notifications opened another program, now this copy");
                if let Err(e) = notify::register(&exe) {
                    log::warn!("schedule: cannot register the notifications: {e:#}");
                }
            }
            None
        }
        Ok(scheduler::TaskState::Runs(cmd)) => {
            log::info!("schedule: the task runs {} (not this copy)", cmd.display());
            Some("moved")
        }
        Ok(scheduler::TaskState::Missing) => {
            log::warn!("schedule: the scheduled task no longer exists, turning the scheduled check off");
            settings.schedule = None;
            if let Err(e) = settings.save() {
                log::warn!("schedule: cannot save settings: {e:#}");
            }
            let _ = notify::unregister();
            Some("missing")
        }
        Ok(scheduler::TaskState::Unreadable) => None,
        Err(e) => {
            log::warn!("schedule: cannot read the scheduled task: {e:#}");
            None
        }
    }
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
    if settings.windows_update {
        let online = result.environment.online;
        let handle = app.clone();
        std::thread::spawn(move || search_windows_updates(&handle, online));
    }
    Ok(result)
}

/// Background phase of a scan: what Windows Update has waiting (an online
/// search can take minutes). Emits `scan-progress` and `windows-updates`.
fn search_windows_updates(app: &AppHandle, online: bool) {
    let progress = |state: StepState| {
        let _ = app.emit("scan-progress", ScanProgress { step: ScanStep::WindowsUpdate, state, detail: None, fraction: None });
    };
    progress(StepState::Started);
    let found = majestic_core::system::windows_update::pending(online);
    progress(if found.error.is_some() { StepState::Failed } else { StepState::Done });
    if let Ok(mut last) = app.state::<AppState>().windows_updates.lock() {
        *last = Some(found.clone());
    }
    let _ = app.emit("windows-updates", found);
}

/// Last answer of Windows Update, `null` before the first search ends.
#[tauri::command]
pub fn get_windows_updates(state: State<'_, AppState>) -> Option<WindowsUpdates> {
    state.windows_updates.lock().ok().and_then(|w| w.clone())
}

/// Starts the uninstaller of the registry entry `key` (`UpdateItem.uninstallKey`,
/// read again from the registry: the UI never passes a command line), the
/// program's own uninstaller as Settings › Apps would.
#[tauri::command]
pub async fn run_uninstaller(key: String) -> Result<(), String> {
    blocking(move || {
        let program = registry::installed_programs()
            .into_iter()
            .find(|p| p.reg_id() == key)
            .ok_or_else(|| "not_installed".to_string())?;
        let command = program.uninstall_string.clone().filter(|s| !s.trim().is_empty()).ok_or_else(|| "no_uninstaller".to_string())?;
        shell::run_uninstaller(&command, program.windows_installer).map_err(|e| format!("{e:#}"))
    })
    .await?
}

/// Opens Settings › Apps › Installed apps, or the classic Control Panel page
/// "Programs and Features" when `classic`.
#[tauri::command]
pub fn open_installed_apps(classic: bool) -> Result<(), String> {
    let opened = if classic { shell::open_programs_and_features() } else { shell::open_settings(shell::SettingsPage::InstalledApps) };
    opened.map_err(|e| format!("{e:#}"))
}

/// Opens Settings › Windows Update: `check` starts a search, `optional`
/// shows the optional updates (drivers).
#[tauri::command]
pub fn open_windows_update(page: String) -> Result<(), String> {
    let page = match page.as_str() {
        "optional" => shell::SettingsPage::WindowsUpdateOptional,
        _ => shell::SettingsPage::WindowsUpdateCheck,
    };
    shell::open_settings(page).map_err(|e| format!("{e:#}"))
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
