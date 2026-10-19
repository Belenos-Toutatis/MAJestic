//! "Apps & features" inventory from the Uninstall registry keys.
//!
//! OWNER: system agent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::Scope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hive {
    /// HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall (64-bit view)
    Hklm64,
    /// HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall
    Hklm32,
    /// HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
    Hkcu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledProgram {
    pub hive: Hive,
    pub key_name: String,
    pub display_name: String,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    pub install_location: Option<String>,
    /// Raw `DisplayIcon` (`C:\path\app.exe,0`).
    pub display_icon: Option<String>,
    pub uninstall_string: Option<String>,
    pub url_info_about: Option<String>,
    pub url_update_info: Option<String>,
    pub help_link: Option<String>,
    /// `YYYYMMDD` when present.
    pub install_date: Option<String>,
    pub system_component: bool,
    pub windows_installer: bool,
    /// Has `ParentKeyName` / is an update or hotfix of another entry.
    pub is_update: bool,
}

impl InstalledProgram {
    pub fn scope(&self) -> Scope {
        match self.hive {
            Hive::Hkcu => Scope::User,
            _ => Scope::Machine,
        }
    }

    /// Unique id used in item keys: `HKLM64\<key>`, `HKLM32\<key>`, `HKCU\<key>`.
    pub fn reg_id(&self) -> String {
        let hive = match self.hive {
            Hive::Hklm64 => "HKLM64",
            Hive::Hklm32 => "HKLM32",
            Hive::Hkcu => "HKCU",
        };
        format!("{hive}\\{}", self.key_name)
    }

    /// The pseudo id winget prints for this entry when it is not matched to a
    /// package: `ARP\Machine\X64\<key>`, `ARP\Machine\X86\<key>`,
    /// `ARP\User\X64\<key>`.
    pub fn winget_arp_id(&self) -> String {
        todo!("registry::winget_arp_id")
    }

    /// Directories where the program's executables live: `InstallLocation`,
    /// the folder of `DisplayIcon`, the folder of `UninstallString` — skipping
    /// Windows, System32, `ProgramData\Package Cache`, `Windows\Installer` and
    /// other shared locations. Deduplicated, existing only.
    pub fn install_dirs(&self) -> Vec<PathBuf> {
        todo!("registry::install_dirs")
    }
}

/// Every entry of the three Uninstall keys that has a `DisplayName`.
/// System components and updates are included but flagged.
pub fn installed_programs() -> Vec<InstalledProgram> {
    todo!("registry::installed_programs")
}
