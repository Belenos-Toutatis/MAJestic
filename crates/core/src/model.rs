//! Shared data model.
//!
//! Every type here is serialized to JSON and consumed by the web UI, so field
//! names are camelCase and enums are tagged with `"type"` when they carry data.
//! Changing a type here is a contract change: update `ui/js/` accordingly.

use serde::{Deserialize, Serialize};

/// Where a package comes from / which tool is able to update it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// winget, community repository (`winget` source).
    Winget,
    /// winget, Microsoft Store source (`msstore`).
    #[serde(rename = "msstore")]
    MsStore,
    Chocolatey,
    Scoop,
    /// Program found in the Windows "Apps & features" registry keys, not
    /// managed by any package manager.
    Registry,
    /// MSIX / Store / system app not matched to any source.
    Msix,
}

impl Source {
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Winget => "winget",
            Source::MsStore => "msstore",
            Source::Chocolatey => "chocolatey",
            Source::Scoop => "scoop",
            Source::Registry => "registry",
            Source::Msix => "msix",
        }
    }
}

/// Installation scope, deduced from the registry hive (HKCU = user).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    User,
    Machine,
}

/// How much we trust `installed_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionCertainty {
    /// Reported as-is by the package manager or the registry.
    Exact,
    /// winget displayed `< x` or `> x`.
    Approximate,
    /// The package manager could not tell; MAJestic read it from the
    /// program's executable file.
    FromFile,
    /// Nobody knows.
    Unknown,
}

/// What kind of row this is in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// An update can be installed automatically by a package manager.
    Update,
    /// A newer version exists (found on Chocolatey's catalog, GitHub...), but
    /// the program is not managed: the user has to install it by hand.
    Manual,
    /// Managed by a package manager and up to date.
    UpToDate,
    /// Not managed by any package manager and no update information found.
    Unmanaged,
}

/// Ordering/grouping in the UI, most important first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Security fix or software with actively exploited vulnerabilities.
    Critical,
    /// Major version jump: new features, possible breaking changes or paid upgrade.
    Major,
    /// Internet-exposed software (browser, PDF reader, runtime...): keep it current.
    Recommended,
    /// Regular update.
    Normal,
    /// Installed version unknown: the user should decide.
    Uncertain,
}

/// Families of software with a large attack surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureCategory {
    Browser,
    DocumentReader,
    Runtime,
    Archiver,
    MediaPlayer,
    Communication,
    RemoteAccess,
    Vpn,
    PasswordManager,
    Office,
    DevTool,
    CloudSync,
    Network,
}

/// Why an item got its priority. Rendered (translated) by the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Reason {
    /// Listed in CISA's Known Exploited Vulnerabilities catalog.
    KnownExploited {
        /// CVE identifiers, most recent first (at most 5).
        cves: Vec<String>,
        /// `dateAdded` of the most recent entry (YYYY-MM-DD).
        most_recent: String,
        /// Most recent entry added less than 2 years ago.
        recent: bool,
        /// At least one entry is known to be used by ransomware.
        ransomware: bool,
    },
    /// Security keywords found in the release notes of the new version.
    SecurityFix { keywords: Vec<String> },
    /// Internet-exposed family of software.
    Exposed { category: ExposureCategory },
    /// Major version jump.
    MajorUpgrade { from: String, to: String },
    /// Installed version could not be determined.
    UnknownVersion,
    /// Installed version is approximate.
    ApproximateVersion,
    /// Version was read from the executable file.
    VersionFromFile { version: String },
    /// winget requires the package to be targeted explicitly.
    RequiresExplicit,
    /// Package is pinned in winget.
    Pinned,
    /// The program is currently running.
    AppRunning { processes: Vec<String> },
    /// Same software also seems managed by another package manager.
    DuplicateManager { other: Source },
    /// The winget manifest of the available version points to the installer
    /// of the installed version: installing it changes nothing.
    StaleInstaller { installer_version: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub priority: Priority,
    /// Checked by default in the UI ("recommended selection").
    pub preselect: bool,
    pub reasons: Vec<Reason>,
}

impl Default for Assessment {
    fn default() -> Self {
        Assessment { priority: Priority::Normal, preselect: false, reasons: Vec::new() }
    }
}

/// What a program is, beyond "an application".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Category {
    Application,
    /// Updates itself (browser with its own updater, Squirrel apps, Office C2R...).
    AutoUpdating { by: String },
    /// Installed and updated by a game launcher (Steam, Epic, GOG, Battle.net...).
    Game { launcher: String },
    /// Progressive web app installed by a browser.
    WebApp { browser: String },
    /// Runtime, SDK, redistributable, Windows component.
    SystemComponent,
    /// Hardware driver or vendor utility (best updated through Windows Update / the vendor).
    Driver,
    /// Microsoft Store / MSIX app.
    StoreApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlPurpose {
    Homepage,
    Download,
    ReleaseNotes,
    Support,
    Search,
}

/// Something the user can do when the automatic path is not available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Alternative {
    /// winget: install the latest version over the current one even though the
    /// installed version is unknown.
    ForceReinstall,
    /// winget: pick a specific version (rollback or older release).
    InstallVersion,
    /// Unmanaged program that seems to exist in winget under this id.
    WingetCandidate { id: String, name: String, version: Option<String> },
    /// A more recent version was found in a public catalog.
    LatestElsewhere { version: String, origin: String, url: Option<String> },
    /// Open a web page.
    OpenUrl { purpose: UrlPurpose, url: String },
    /// Updated by something else (Steam, Microsoft Store, Windows Update, its own updater...).
    ManagedElsewhere { by: String },
    /// Ask the Microsoft Store to look for app updates.
    StoreUpdate,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    pub homepage: Option<String>,
    pub download: Option<String>,
    pub release_notes: Option<String>,
    pub support: Option<String>,
}

/// A process belonging to a program that is about to be updated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningProcess {
    pub pid: u32,
    /// File name, e.g. `nextcloud.exe`.
    pub name: String,
    /// Full path of the executable.
    pub exe: String,
    /// Has at least one visible top-level window (may hold unsaved work).
    pub has_window: bool,
}

/// One row of the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    /// Stable unique key: `winget:<id>`, `msstore:<id>`, `choco:<id>`,
    /// `scoop:<app>`, `arp:<hive>\<key>`, `msix:<id>`.
    pub key: String,
    pub source: Source,
    /// Identifier for the package manager (winget id, choco id, scoop app,
    /// registry key name for unmanaged programs).
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub installed_version: Option<String>,
    pub version_certainty: VersionCertainty,
    pub available_version: Option<String>,
    pub kind: ItemKind,
    pub category: Category,
    pub scope: Option<Scope>,
    /// winget: only upgradable with an explicit `--id`.
    pub explicit_only: bool,
    pub pinned: bool,
    pub install_location: Option<String>,
    /// Value to pass to `get_icon` (registry DisplayIcon or an exe path).
    pub icon_hint: Option<String>,
    pub links: Links,
    /// Plain-text excerpt of the release notes of the available version.
    pub release_notes: Option<String>,
    /// Size of the installer that will be downloaded, in bytes (HTTP HEAD on
    /// the manifest's installer URL), when known.
    pub download_size: Option<u64>,
    /// winget installer type (`msi`, `wix`, `inno`, `nullsoft`, `exe`, `msix`,
    /// `zip`, `portable`, `burn`...), used for time estimates.
    pub installer_type: Option<String>,
    pub assessment: Assessment,
    pub alternatives: Vec<Alternative>,
    pub running: Vec<RunningProcess>,
    /// The user chose to ignore this package (or this exact available version).
    pub ignored: bool,
    /// Registry entry with an uninstaller (`HKLM64\<key>`...), for
    /// `run_uninstaller`.
    #[serde(default)]
    pub uninstall_key: Option<String>,
    /// Folders of the program's executables (not sent to the UI): used to
    /// find its running processes right before updating it.
    #[serde(skip)]
    pub install_dirs: Vec<std::path::PathBuf>,
}

impl UpdateItem {
    pub fn new(key: String, source: Source, id: String, name: String) -> Self {
        UpdateItem {
            key,
            source,
            id,
            name,
            publisher: None,
            installed_version: None,
            version_certainty: VersionCertainty::Exact,
            available_version: None,
            kind: ItemKind::UpToDate,
            category: Category::Application,
            scope: None,
            explicit_only: false,
            pinned: false,
            install_location: None,
            icon_hint: None,
            links: Links::default(),
            release_notes: None,
            download_size: None,
            installer_type: None,
            assessment: Assessment::default(),
            alternatives: Vec::new(),
            running: Vec::new(),
            ignored: false,
            uninstall_key: None,
            install_dirs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub updates: usize,
    pub critical: usize,
    pub major: usize,
    pub recommended: usize,
    pub uncertain: usize,
    pub manual: usize,
    pub unmanaged: usize,
    pub up_to_date: usize,
    pub ignored: usize,
    /// Store/MSIX apps that are not listed individually.
    pub store_apps: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanWarning {
    /// `winget_missing`, `winget_failed`, `choco_failed`, `scoop_failed`,
    /// `offline`, `kev_failed`, `registry_failed`.
    pub code: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    pub app_version: String,
    pub winget: Option<String>,
    pub chocolatey: Option<String>,
    pub scoop: bool,
    /// MAJestic itself runs elevated.
    pub elevated: bool,
    /// The user belongs to the Administrators group (UAC can elevate silently-ish).
    pub admin_member: bool,
    pub os: String,
    pub online: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub items: Vec<UpdateItem>,
    pub stats: ScanStats,
    pub warnings: Vec<ScanWarning>,
    pub environment: EnvInfo,
    /// RFC 3339.
    pub scanned_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStep {
    Environment,
    WingetUpgrade,
    WingetList,
    Registry,
    Chocolatey,
    Scoop,
    SecurityCatalog,
    ReleaseNotes,
    Processes,
    /// Background phase: latest-version lookups for unmanaged programs.
    Lookup,
    /// Background phase: updates waiting in Windows Update.
    WindowsUpdate,
}

/// Classification of a pending Windows update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsUpdateKind {
    /// Security update (monthly cumulative update, .NET security fix...).
    Security,
    /// Non-security fix that Microsoft rates critical.
    Critical,
    /// New Windows version or feature pack.
    Feature,
    /// Other updates and update rollups.
    Update,
    /// Hardware driver.
    Driver,
    /// Microsoft Defender security intelligence.
    Definition,
}

/// One update waiting in Windows Update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsUpdate {
    pub title: String,
    /// `KB5031234`...
    pub kb: Vec<String>,
    pub kind: WindowsUpdateKind,
    /// Microsoft's security rating: `Critical`, `Important`, `Moderate`, `Low`.
    pub severity: Option<String>,
    /// Offered on demand only (Settings › Windows Update › Optional updates).
    pub optional: bool,
    /// Already downloaded, ready to install.
    pub downloaded: bool,
    /// Download size in bytes (the largest possible), when known.
    pub size: Option<u64>,
    /// Installing it may require a restart.
    pub reboot: bool,
    /// Publication date (YYYY-MM-DD).
    pub date: Option<String>,
    pub support_url: Option<String>,
}

/// Event `windows-updates` / command `get_windows_updates`: what Windows
/// Update has waiting.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsUpdates {
    pub updates: Vec<WindowsUpdate>,
    /// Updates are installed and Windows waits for a restart to finish.
    pub reboot_pending: bool,
    /// Last successful search by Windows itself (RFC 3339).
    pub last_search: Option<String>,
    /// Last successful installation by Windows itself (RFC 3339).
    pub last_install: Option<String>,
    /// The list comes from a fresh online search (`false`: Windows' cached
    /// results, the online search failed).
    pub online: bool,
    /// Windows Update could not be queried at all (service disabled...).
    pub error: Option<String>,
    /// When MAJestic asked (RFC 3339).
    pub checked_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Started,
    Done,
    Failed,
    Skipped,
}

/// Event `scan-progress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub step: ScanStep,
    pub state: StepState,
    pub detail: Option<String>,
    /// 0.0..=1.0 within the step when known.
    pub fraction: Option<f32>,
}

// ---------------------------------------------------------------------------
// Jobs (installation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum JobAction {
    /// Upgrade to the latest version.
    Upgrade,
    /// Install a specific version (rollback / downgrade / pick a release).
    InstallVersion { version: String },
    /// winget: install latest over the existing install (unknown version or
    /// unmanaged program matched to a winget id).
    ForceReinstall,
    /// Retry with the installer's own interface (silent mode failed).
    Interactive,
    /// Ask the Microsoft Store to update its apps.
    StoreUpdateScan,
    /// Remove the program with its package manager.
    Uninstall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRequest {
    pub key: String,
    pub source: Source,
    pub id: String,
    pub name: String,
    pub action: JobAction,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub scope: Option<Scope>,
    /// winget: pass `--include-unknown` (installed version unknown).
    #[serde(default)]
    pub include_unknown: bool,
    /// winget: pass `--include-pinned`.
    #[serde(default)]
    pub include_pinned: bool,
    /// From the scan, for time estimates.
    #[serde(default)]
    pub download_size: Option<u64>,
    #[serde(default)]
    pub installer_type: Option<String>,
}

impl JobRequest {
    /// Must this job run elevated? Per-user installs, Store apps and Scoop
    /// must NOT run elevated; Chocolatey always needs it; machine-wide or
    /// unknown-scope winget packages get it.
    pub fn needs_elevation(&self) -> bool {
        if self.action == JobAction::StoreUpdateScan {
            return true;
        }
        match self.source {
            Source::Scoop | Source::MsStore | Source::Msix => false,
            Source::Chocolatey => true,
            Source::Winget | Source::Registry => self.scope != Some(Scope::User),
        }
    }
}

/// Why an installation failed, deduced from the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    AppRunning,
    InstallInProgress,
    RebootRequired,
    NoNetwork,
    /// The publisher's server refused or failed the download (HTTP 403,
    /// 404, 5xx...), or the file arrived broken.
    DownloadFailed,
    DiskFull,
    CancelledByUser,
    BlockedByPolicy,
    HashMismatch,
    NoApplicableInstaller,
    UpgradeNotApplicable,
    DowngradeRefused,
    AlreadyInstalled,
    MissingDependency,
    PackageNotFound,
    ElevationRequired,
    ElevationRefused,
    InstallerFailed,
    /// The installer reported success but the installed version did not
    /// change (often a broken winget manifest).
    NoEffect,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum JobState {
    Queued,
    /// Closing the program's running processes.
    ClosingApps,
    Starting,
    Downloading,
    Installing,
    Succeeded { reboot_required: bool },
    Failed { code: Option<i64>, kind: FailureKind, log_excerpt: String },
    Cancelled,
    Skipped { reason: String },
}

impl JobState {
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            JobState::Succeeded { .. } | JobState::Failed { .. } | JobState::Cancelled | JobState::Skipped { .. }
        )
    }
}

/// Event `job-progress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub key: String,
    pub state: JobState,
    /// 0.0..=1.0 of the *whole job* (download + install, weighted by the
    /// estimate), when known.
    pub progress: Option<f32>,
    /// Bytes downloaded so far / total, during the download phase.
    pub downloaded: Option<u64>,
    pub total_bytes: Option<u64>,
    /// Smoothed download speed, bytes per second.
    pub speed: Option<f64>,
    /// Estimated seconds left for this job.
    pub eta_secs: Option<f64>,
    /// Last meaningful output line (already cleaned), for display.
    pub message: Option<String>,
}

impl JobEvent {
    pub fn state(key: &str, state: JobState) -> JobEvent {
        JobEvent {
            key: key.to_string(),
            state,
            progress: None,
            downloaded: None,
            total_bytes: None,
            speed: None,
            eta_secs: None,
            message: None,
        }
    }
}

/// Event `queue-progress`: the whole batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueProgress {
    pub done: usize,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    /// 0.0..=1.0, weighted by estimated durations (not by count).
    pub fraction: f32,
    pub elapsed_secs: f64,
    /// Estimated seconds left for the whole batch.
    pub eta_secs: Option<f64>,
    /// Key of the job being processed.
    pub current: Option<String>,
}

/// Up-front estimate for one job, shown before starting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEstimate {
    pub key: String,
    pub download_secs: f64,
    pub install_secs: f64,
    /// Where the install time comes from: `history` (this PC already
    /// installed it), `type` (installer type and size), `default`.
    pub basis: String,
}

/// One line of `history.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// RFC 3339.
    pub date: String,
    pub key: String,
    pub source: Source,
    pub id: String,
    pub name: String,
    pub action: JobAction,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    /// `succeeded`, `failed`, `cancelled`, `skipped`.
    pub result: String,
    pub failure: Option<FailureKind>,
    pub code: Option<i64>,
    pub reboot_required: bool,
    /// Whole job, from start to end (closing apps excluded).
    #[serde(default)]
    pub duration_secs: Option<f64>,
    /// Measured durations, used to estimate future runs.
    #[serde(default)]
    pub download_secs: Option<f64>,
    #[serde(default)]
    pub install_secs: Option<f64>,
    #[serde(default)]
    pub download_bytes: Option<u64>,
    #[serde(default)]
    pub installer_type: Option<String>,
}

/// Command to run for a job (built by the package-manager modules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}
