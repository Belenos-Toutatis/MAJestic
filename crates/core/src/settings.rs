//! User preferences, stored as JSON in the data directory.

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// What to do with programs that are running when they are about to be updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloseAppsPolicy {
    /// One grouped confirmation before starting.
    #[default]
    Ask,
    /// Close and relaunch without asking.
    Always,
    /// Never close anything: running programs are skipped.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Daily,
    #[default]
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotifyLevel {
    /// Notify whenever an update is available.
    All,
    /// Notify only for critical or recommended updates.
    #[default]
    Important,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub frequency: Frequency,
    /// `HH:MM`, local time.
    pub time: String,
    pub notify: NotifyLevel,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule { frequency: Frequency::Weekly, time: "12:00".into(), notify: NotifyLevel::Important }
    }
}

/// "Do not propose this package" (any version) or "skip this version".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreRule {
    pub key: String,
    /// Name, for display in the settings page.
    pub name: String,
    /// `None` = ignore every version; `Some(v)` = ignore only version `v`.
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `fr`, `en`, or `None` to follow Windows.
    pub language: Option<String>,
    pub theme: Theme,
    /// Scan automatically when the window opens.
    pub scan_on_start: bool,
    /// Use public online sources (CISA catalog, release notes, catalogs).
    pub online_intel: bool,
    /// Look for newer versions of programs that no package manager handles.
    pub lookup_unmanaged: bool,
    /// List winget packages whose installed version is unknown.
    pub include_unknown: bool,
    pub use_chocolatey: bool,
    pub use_scoop: bool,
    pub close_apps: CloseAppsPolicy,
    pub ignored: Vec<IgnoreRule>,
    /// Scheduled background check (Windows Task Scheduler). `None` = disabled.
    pub schedule: Option<Schedule>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: None,
            theme: Theme::System,
            scan_on_start: true,
            online_intel: true,
            lookup_unmanaged: true,
            include_unknown: true,
            use_chocolatey: true,
            use_scoop: true,
            close_apps: CloseAppsPolicy::Ask,
            ignored: Vec::new(),
            schedule: None,
        }
    }
}

impl Settings {
    /// Loads settings; missing or corrupt file gives defaults.
    pub fn load() -> Settings {
        std::fs::read_to_string(paths::settings_file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let path = paths::settings_file();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Is this package (or this available version of it) ignored?
    pub fn is_ignored(&self, key: &str, available: Option<&str>) -> bool {
        self.ignored.iter().any(|r| {
            r.key == key
                && match (&r.version, available) {
                    (None, _) => true,
                    (Some(v), Some(a)) => v == a,
                    (Some(_), None) => false,
                }
        })
    }
}
