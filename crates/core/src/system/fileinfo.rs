//! Version information of executable files.
//!
//! OWNER: system agent.

use std::path::{Path, PathBuf};

use crate::system::registry::InstalledProgram;

/// `ProductVersion` (falling back to `FileVersion`) of a PE file, cleaned
/// (`1, 2, 3, 0` → `1.2.3.0`).
pub fn file_version(path: &Path) -> Option<String> {
    let _ = path;
    todo!("fileinfo::file_version")
}

/// Best guess of the program's main executable: `DisplayIcon` when it is an
/// .exe (not an uninstaller), else an .exe in `InstallLocation` whose name
/// resembles the display name.
pub fn main_executable(p: &InstalledProgram) -> Option<PathBuf> {
    let _ = p;
    todo!("fileinfo::main_executable")
}

/// Installed version from the main executable, when the registry has none.
pub fn detect_version(p: &InstalledProgram) -> Option<String> {
    main_executable(p).and_then(|exe| file_version(&exe))
}
