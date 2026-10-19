//! `--check`: scheduled background check. Scans quietly and shows a
//! notification when there is something worth installing.

use majestic_core::model::{ItemKind, Priority};
use majestic_core::settings::{NotifyLevel, Settings};
use majestic_core::system::shell;

use crate::notify;

pub fn run() -> i32 {
    let settings = Settings::load();
    let Some(schedule) = settings.schedule.clone() else {
        log::info!("check: schedule disabled, nothing to do");
        return 0;
    };
    let result = majestic_core::scan::scan(&settings, &|_| {});
    let updates: Vec<_> = result.items.iter().filter(|i| i.kind == ItemKind::Update && !i.ignored).collect();
    let critical = updates.iter().filter(|i| i.assessment.priority == Priority::Critical).count();
    let important = updates
        .iter()
        .filter(|i| matches!(i.assessment.priority, Priority::Critical | Priority::Recommended))
        .count();
    log::info!("check: {} updates, {important} important, {critical} critical", updates.len());

    let worth = match schedule.notify {
        NotifyLevel::All => !updates.is_empty(),
        NotifyLevel::Important => important > 0,
    };
    if !worth {
        return 0;
    }

    let lang = settings.language.clone().unwrap_or_else(shell::ui_language);
    let n = updates.len();
    let (title, body, button) = if lang.starts_with("fr") {
        let title = if n > 1 { format!("{n} mises à jour disponibles") } else { "1 mise à jour disponible".into() };
        let body = match critical {
            0 => "Ouvrez MAJestic pour les installer en un clic.".to_string(),
            1 => "Dont 1 critique pour la sécurité. Ouvrez MAJestic pour l'installer.".to_string(),
            c => format!("Dont {c} critiques pour la sécurité. Ouvrez MAJestic pour les installer."),
        };
        (title, body, "Ouvrir MAJestic")
    } else {
        let title = if n > 1 { format!("{n} updates available") } else { "1 update available".into() };
        let body = match critical {
            0 => "Open MAJestic to install them in one click.".to_string(),
            1 => "Including 1 critical security update. Open MAJestic to install it.".to_string(),
            c => format!("Including {c} critical security updates. Open MAJestic to install them."),
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
