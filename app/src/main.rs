//! MAJestic executable.
//!
//! * no argument: the window (Tauri);
//! * `--worker <pipe> <parent-pid>`: elevated helper started by the window
//!   through UAC, runs the installations that need administrator rights;
//! * `--check`: scheduled background check, shows a notification;
//! * `--after-update <pid> <version>`: the window, started by the previous
//!   version after it replaced itself (see `appupdate`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appupdate;
mod batch;
mod check;
mod commands;
mod logging;
mod notify;
mod worker;

use tauri::Manager;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.get(1).map(String::as_str) == Some("--worker") {
        logging::init("worker");
        std::process::exit(worker::run_worker(&args[2..]));
    }
    if args.iter().any(|a| a == "--check") {
        logging::init("check");
        std::process::exit(check::run());
    }

    logging::init("app");
    let updated_from = appupdate::after_update(&args);
    match &updated_from {
        Some(previous) => log::info!("MAJestic {} starting (updated from {previous})", majestic_core::APP_VERSION),
        None => log::info!("MAJestic {} starting", majestic_core::APP_VERSION),
    }
    // Leftovers of an update: the previous executable, a partial download.
    std::thread::spawn(|| {
        if let Ok(exe) = std::env::current_exe() {
            majestic_core::selfupdate::cleanup(&exe);
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(commands::AppState { updated_from, ..Default::default() })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::scan,
            commands::get_icon,
            commands::list_versions,
            commands::search_winget,
            commands::running_apps,
            commands::estimate_jobs,
            commands::start_updates,
            commands::cancel_updates,
            commands::get_history,
            commands::clear_history,
            commands::open_url,
            commands::open_store_updates,
            commands::open_logs,
            appupdate::check_app_update,
            appupdate::install_app_update,
        ])
        .setup(|_app| {
            commands::refresh_schedule_path();
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log::error!("cannot start the window: {e}");
            std::process::exit(1);
        });
}
