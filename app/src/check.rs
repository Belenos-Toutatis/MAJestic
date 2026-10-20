//! `--check`: scheduled background check. Scans quietly and shows a
//! notification when there is something worth installing.

use majestic_core::model::{ItemKind, Priority, WindowsUpdateKind};
use majestic_core::settings::{NotifyLevel, Settings};
use majestic_core::system::{shell, windows_update};

use crate::notify;

pub fn run() -> i32 {
    let settings = Settings::load();
    let Some(schedule) = settings.schedule.clone() else {
        log::info!("check: schedule disabled in {}, nothing to do", majestic_core::paths::settings_file().display());
        return 0;
    };
    let result = majestic_core::scan::scan(&settings, &|_| {});
    let updates: Vec<_> = result.items.iter().filter(|i| i.kind == ItemKind::Update && !i.ignored).collect();
    let critical = updates.iter().filter(|i| i.assessment.priority == Priority::Critical).count();
    let important = updates
        .iter()
        .filter(|i| matches!(i.assessment.priority, Priority::Critical | Priority::Recommended))
        .count();
    // Security updates waiting in Windows Update count as important.
    let wu_security = if settings.windows_update {
        let wu = windows_update::pending(true);
        wu.updates.iter().filter(|u| !u.optional && u.kind == WindowsUpdateKind::Security).count()
    } else {
        0
    };
    log::info!(
        "check: {} updates, {important} important, {critical} critical, {wu_security} Windows security updates",
        updates.len()
    );

    let worth = match schedule.notify {
        NotifyLevel::All => !updates.is_empty() || wu_security > 0,
        NotifyLevel::Important => important > 0 || wu_security > 0,
    };
    if !worth {
        return 0;
    }

    let lang = settings.language.clone().unwrap_or_else(shell::ui_language);
    let n = updates.len();
    let (title, body, button) = if lang.starts_with("fr") {
        let wu_line = match wu_security {
            0 => String::new(),
            1 => " Windows Update : 1 mise à jour de sécurité en attente.".into(),
            c => format!(" Windows Update : {c} mises à jour de sécurité en attente."),
        };
        let (title, body) = if n == 0 {
            ("Mises à jour de sécurité Windows en attente".to_string(), format!("{}Ouvrez MAJestic pour les voir.", wu_line.trim_start()))
        } else {
            let title = if n > 1 { format!("{n} mises à jour disponibles") } else { "1 mise à jour disponible".into() };
            let body = match critical {
                0 => "Ouvrez MAJestic pour les installer en un clic.".to_string(),
                1 => "Dont 1 critique pour la sécurité. Ouvrez MAJestic pour l'installer.".to_string(),
                c => format!("Dont {c} critiques pour la sécurité. Ouvrez MAJestic pour les installer."),
            };
            (title, format!("{body}{wu_line}"))
        };
        (title, body, "Ouvrir MAJestic")
    } else {
        let wu_line = match wu_security {
            0 => String::new(),
            1 => " Windows Update: 1 security update waiting.".into(),
            c => format!(" Windows Update: {c} security updates waiting."),
        };
        let (title, body) = if n == 0 {
            ("Windows security updates waiting".to_string(), format!("{}Open MAJestic to see them.", wu_line.trim_start()))
        } else {
            let title = if n > 1 { format!("{n} updates available") } else { "1 update available".into() };
            let body = match critical {
                0 => "Open MAJestic to install them in one click.".to_string(),
                1 => "Including 1 critical security update. Open MAJestic to install it.".to_string(),
                c => format!("Including {c} critical security updates. Open MAJestic to install them."),
            };
            (title, format!("{body}{wu_line}"))
        };
        (title, body, "Open MAJestic")
    };

    if let Ok(exe) = std::env::current_exe() {
        let _ = notify::register(&exe);
    }
    if let Err(e) = notify::show_toast(&title, &body, button) {
        log::warn!("check: cannot show notification: {e:#}");
        return 1;
    }
    // Let the notification platform take the toast before exiting.
    std::thread::sleep(std::time::Duration::from_secs(2));
    0
}
