//! What kind of program is this (for programs no package manager handles)?
//!
//! OWNER: system agent.

use crate::model::Category;
use crate::system::registry::InstalledProgram;

/// Classifies an installed program: game launcher titles (Steam
/// `steam://uninstall`, Epic, GOG, Ubisoft, EA, Battle.net), browser web apps
/// (Edge/Chrome PWA uninstall strings), self-updating apps (Squirrel
/// `Update.exe`, browsers, Office Click-to-Run, Teams, Zoom, Discord...),
/// drivers and vendor utilities (NVIDIA, AMD, Intel, Realtek...), system
/// components (runtimes, redistributables, SDKs, Windows components).
/// Everything else is `Category::Application`.
pub fn classify(p: &InstalledProgram) -> Category {
    let _ = p;
    todo!("classify::classify")
}
