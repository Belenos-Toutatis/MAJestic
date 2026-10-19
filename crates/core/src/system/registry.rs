//! "Apps & features" inventory from the Uninstall registry keys.
//!
//! OWNER: system agent.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
    KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ, REG_QWORD, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
};

use crate::model::Scope;
use crate::system::{from_wide, wide};

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
        // Checked against `winget list` on Windows 11 x64: the 64-bit HKLM
        // view is `Machine\X64`, WOW6432Node is `Machine\X86`, and HKCU (not
        // redirected by WOW64) is always `User\X64`.
        let prefix = match self.hive {
            Hive::Hklm64 => "ARP\\Machine\\X64",
            Hive::Hklm32 => "ARP\\Machine\\X86",
            Hive::Hkcu => "ARP\\User\\X64",
        };
        format!("{prefix}\\{}", self.key_name)
    }

    /// Directories where the program's executables live: `InstallLocation`,
    /// the folder of `DisplayIcon`, the folder of `UninstallString` — skipping
    /// Windows, System32, `ProgramData\Package Cache`, `Windows\Installer` and
    /// other shared locations. Deduplicated, existing only.
    pub fn install_dirs(&self) -> Vec<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        if let Some(location) = &self.install_location {
            let path = PathBuf::from(expand_env(strip_quotes(location)));
            // Some installers store the main executable instead of its folder.
            if path.is_file() {
                candidates.extend(path.parent().map(Path::to_path_buf));
            } else {
                candidates.push(path);
            }
        }
        if let Some((icon, _)) = self.display_icon.as_deref().and_then(parse_icon_location) {
            candidates.extend(icon.parent().map(Path::to_path_buf));
        }
        if let Some(exe) = self.uninstall_string.as_deref().and_then(command_exe) {
            let file = exe
                .file_name()
                .map(|f| f.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !matches!(file.as_str(), "msiexec.exe" | "msiexec" | "rundll32.exe" | "rundll32") {
                candidates.extend(exe.parent().map(Path::to_path_buf));
            }
        }

        let shared = SharedLocations::get();
        let mut seen = HashSet::new();
        let mut dirs = Vec::new();
        for dir in candidates {
            let text = dir.to_string_lossy();
            let trimmed = trim_trailing_separators(&text);
            let key = norm_path(trimmed);
            if key.is_empty() || !Path::new(trimmed).is_absolute() || shared.contains(&key) {
                continue;
            }
            if seen.insert(key) && Path::new(trimmed).is_dir() {
                dirs.push(PathBuf::from(trimmed));
            }
        }
        dirs
    }
}

/// Every entry of the three Uninstall keys that has a `DisplayName`.
/// System components and updates are included but flagged.
pub fn installed_programs() -> Vec<InstalledProgram> {
    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    let views = [
        (Hive::Hklm64, HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        (Hive::Hklm32, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        (Hive::Hkcu, HKEY_CURRENT_USER, REG_SAM_FLAGS(0)),
    ];
    let mut programs = Vec::new();
    for (hive, root, view) in views {
        let Some(uninstall) = RegKey::open(root, UNINSTALL, view) else {
            log::debug!("registry: no Uninstall key for {hive:?}");
            continue;
        };
        for name in uninstall.subkey_names() {
            let Some(key) = RegKey::open(uninstall.0, &name, view) else {
                continue;
            };
            if let Some(program) = read_program(hive, name, &key) {
                programs.push(program);
            }
        }
    }
    programs
}

fn read_program(hive: Hive, key_name: String, key: &RegKey) -> Option<InstalledProgram> {
    let display_name = key.string("DisplayName")?;
    let release_type = key.string("ReleaseType").unwrap_or_default().to_ascii_lowercase();
    let is_update = key.string("ParentKeyName").is_some()
        || matches!(
            release_type.as_str(),
            "update" | "hotfix" | "security update" | "service pack"
        );
    Some(InstalledProgram {
        hive,
        key_name,
        display_name,
        display_version: key.string("DisplayVersion"),
        publisher: key.string("Publisher"),
        install_location: key.string("InstallLocation"),
        display_icon: key.string("DisplayIcon"),
        uninstall_string: key.string("UninstallString"),
        url_info_about: key.string("URLInfoAbout"),
        url_update_info: key.string("URLUpdateInfo"),
        help_link: key.string("HelpLink"),
        install_date: key.string("InstallDate").as_deref().and_then(normalize_date),
        system_component: key.dword("SystemComponent") == Some(1),
        windows_installer: key.dword("WindowsInstaller") == Some(1),
        is_update,
    })
}

/// `YYYYMMDD` from `20240131`, `2024-01-31`, `2024/01/31`...; `None` for
/// anything else (day-first or month-first dates are ambiguous).
fn normalize_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if !raw.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '/' | '.')) {
        return None;
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 8 {
        return None;
    }
    let year: u32 = digits[0..4].parse().ok()?;
    let month: u32 = digits[4..6].parse().ok()?;
    let day: u32 = digits[6..8].parse().ok()?;
    ((1980..=2100).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day)).then_some(digits)
}

// ---------------------------------------------------------------------------
// Registry access
// ---------------------------------------------------------------------------

/// An open registry key, closed on drop.
pub(crate) struct RegKey(HKEY);

impl RegKey {
    /// Opens `root\path` for reading; `view` is `KEY_WOW64_64KEY`,
    /// `KEY_WOW64_32KEY` or nothing.
    pub(crate) fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<RegKey> {
        let path = wide(path);
        let mut key = HKEY::default();
        // SAFETY: `path` is NUL-terminated and outlives the call.
        let err = unsafe { RegOpenKeyExW(root, PCWSTR(path.as_ptr()), Some(0), KEY_READ | view, &mut key) };
        (err == ERROR_SUCCESS).then_some(RegKey(key))
    }

    pub(crate) fn subkey_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        // Key names are limited to 255 characters.
        let mut buf = vec![0u16; 256];
        let mut index = 0u32;
        loop {
            let mut len = buf.len() as u32;
            // SAFETY: `buf` holds `len` u16 and outlives the call.
            let err = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    Some(PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if err == ERROR_MORE_DATA && buf.len() < 32 * 1024 {
                buf.resize(buf.len() * 2, 0);
                continue;
            }
            if err != ERROR_SUCCESS {
                if err != ERROR_NO_MORE_ITEMS {
                    log::debug!("registry: RegEnumKeyExW stopped: {err:?}");
                }
                break;
            }
            names.push(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]));
            index += 1;
        }
        names
    }

    fn raw_value(&self, name: &str) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
        let name = wide(name);
        let mut kind = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        // SAFETY: `name` is NUL-terminated; this call only reports the size.
        let err = unsafe {
            RegQueryValueExW(
                self.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };
        if err != ERROR_SUCCESS {
            return None;
        }
        // The value may grow between the two calls: retry a few times.
        for _ in 0..4 {
            // Two spare bytes so strings stored without their NUL still end with one.
            let mut data = vec![0u8; size as usize + 2];
            let mut len = data.len() as u32;
            // SAFETY: `data` holds `len` bytes and outlives the call.
            let err = unsafe {
                RegQueryValueExW(
                    self.0,
                    PCWSTR(name.as_ptr()),
                    None,
                    Some(&mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&mut len),
                )
            };
            if err == ERROR_MORE_DATA {
                size = len;
                continue;
            }
            if err != ERROR_SUCCESS {
                return None;
            }
            data.truncate(len as usize);
            return Some((kind, data));
        }
        None
    }

    /// `REG_SZ` / `REG_EXPAND_SZ` value (expanded), trimmed; `None` when
    /// absent or empty.
    pub(crate) fn string(&self, name: &str) -> Option<String> {
        let (kind, data) = self.raw_value(name)?;
        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return None;
        }
        let units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        let mut value = from_wide(&units);
        if kind == REG_EXPAND_SZ {
            value = expand_env(&value);
        }
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }

    /// `REG_DWORD` value (also accepts `REG_QWORD` and numeric strings).
    pub(crate) fn dword(&self, name: &str) -> Option<u32> {
        let (kind, data) = self.raw_value(name)?;
        match kind {
            REG_DWORD if data.len() >= 4 => Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]])),
            REG_QWORD if data.len() >= 8 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&data[..8]);
                u32::try_from(u64::from_le_bytes(bytes)).ok()
            }
            REG_SZ => self.string(name)?.parse().ok(),
            _ => None,
        }
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

/// Expands `%VARIABLE%` references; returns the input unchanged on failure.
pub(crate) fn expand_env(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let src = wide(s);
    // SAFETY: `src` is NUL-terminated; without a buffer the call returns the
    // required size in characters (NUL included).
    let needed = unsafe { ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), None) };
    if needed == 0 {
        return s.to_string();
    }
    let mut buf = vec![0u16; needed as usize];
    // SAFETY: `buf` is a valid mutable slice; the API writes at most its length.
    let written = unsafe { ExpandEnvironmentStringsW(PCWSTR(src.as_ptr()), Some(&mut buf)) };
    if written == 0 || written as usize > buf.len() {
        return s.to_string();
    }
    from_wide(&buf)
}

// ---------------------------------------------------------------------------
// Path helpers (shared with fileinfo, icons and procs)
// ---------------------------------------------------------------------------

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    s.strip_suffix('"').unwrap_or(s).trim()
}

fn trim_trailing_separators(s: &str) -> &str {
    let trimmed = s.trim_end_matches(['\\', '/']);
    // Keep the separator of a drive root (`C:\`).
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        &s[..3.min(s.len())]
    } else {
        trimmed
    }
}

/// Lowercase comparison form of a path: backslashes, no `\\?\` prefix, no
/// trailing separator.
pub(crate) fn norm_path(p: &str) -> String {
    let p = p.trim().replace('/', "\\");
    let p = p.strip_prefix(r"\\?\").unwrap_or(&p);
    p.trim_end_matches('\\').to_lowercase()
}

/// `path` lies inside `dir` (both in [`norm_path`] form), or is `dir` itself.
pub(crate) fn is_within(path: &str, dir: &str) -> bool {
    !dir.is_empty()
        && path.len() >= dir.len()
        && path.starts_with(dir)
        && (path.len() == dir.len() || path.as_bytes()[dir.len()] == b'\\')
}

/// Splits a `DisplayIcon`-like value (`"C:\x\app.exe",0`, `C:\x\app.exe,-101`,
/// `C:\x\app.ico`) into the file path (environment expanded) and the icon
/// index (0 when absent; negative = resource id).
pub(crate) fn parse_icon_location(raw: &str) -> Option<(PathBuf, i32)> {
    fn split_index(s: &str) -> (&str, i32) {
        match s.rfind(',') {
            Some(pos) => match s[pos + 1..].trim().parse::<i32>() {
                Ok(index) => (&s[..pos], index),
                Err(_) => (s, 0),
            },
            None => (s, 0),
        }
    }

    let s = raw.trim();
    let (path, index) = match s.strip_prefix('"') {
        Some(quoted) => {
            let end = quoted.find('"').unwrap_or(quoted.len());
            let rest = quoted[end..].trim_start_matches('"').trim();
            match rest.strip_prefix(',') {
                Some(index) => (&quoted[..end], index.trim().parse().unwrap_or(0)),
                // `"C:\x\app.exe,0"`: the index is inside the quotes.
                None => split_index(&quoted[..end]),
            }
        }
        None => split_index(s),
    };
    let path = expand_env(path.trim());
    (!path.is_empty()).then(|| (PathBuf::from(path), index))
}

/// Executable of a command line such as an `UninstallString`: the quoted
/// first token, or (unquoted) everything up to the first `.exe` followed by a
/// blank, else the first blank-separated token.
pub(crate) fn command_exe(cmd: &str) -> Option<PathBuf> {
    let s = cmd.trim();
    if let Some(quoted) = s.strip_prefix('"') {
        let end = quoted.find('"').unwrap_or(quoted.len());
        let path = quoted[..end].trim();
        return (!path.is_empty()).then(|| PathBuf::from(expand_env(path)));
    }
    // ASCII lowercasing keeps byte offsets identical.
    let lower = s.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(".exe") {
        let end = from + pos + 4;
        let next = s[end..].chars().next();
        if next.is_none_or(|c| c.is_whitespace() || c == '"' || c == ',') {
            return Some(PathBuf::from(expand_env(s[..end].trim())));
        }
        from = end;
    }
    s.split_whitespace()
        .next()
        .map(|token| PathBuf::from(expand_env(token)))
}

/// Shared or overly broad folders that never identify one program.
pub(crate) struct SharedLocations {
    /// Excluded with everything below them ([`norm_path`] form).
    trees: Vec<String>,
    /// Excluded themselves only (their subfolders are program folders).
    exact: Vec<String>,
}

impl SharedLocations {
    pub(crate) fn get() -> &'static SharedLocations {
        static LOCATIONS: OnceLock<SharedLocations> = OnceLock::new();
        LOCATIONS.get_or_init(SharedLocations::from_env)
    }

    fn from_env() -> SharedLocations {
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| norm_path(&v))
                .filter(|v| !v.is_empty())
        };
        let windir = var("SystemRoot")
            .or_else(|| var("windir"))
            .unwrap_or_else(|| r"c:\windows".into());
        let program_data = var("ProgramData").unwrap_or_else(|| r"c:\programdata".into());
        let program_files: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(|v| var(v))
            .collect();

        let mut trees = vec![windir, format!(r"{program_data}\package cache")];
        trees.extend(
            [
                "CommonProgramFiles",
                "CommonProgramFiles(x86)",
                "CommonProgramW6432",
                "TEMP",
                "TMP",
            ]
            .iter()
            .filter_map(|v| var(v)),
        );
        trees.extend(program_files.iter().map(|p| format!(r"{p}\common files")));
        trees.extend(
            program_files
                .iter()
                .map(|p| format!(r"{p}\installshield installation information")),
        );

        let mut exact = program_files;
        exact.push(program_data);
        exact.extend(
            ["USERPROFILE", "LOCALAPPDATA", "APPDATA", "PUBLIC"]
                .iter()
                .filter_map(|v| var(v)),
        );
        if let Some(profile) = var("USERPROFILE") {
            exact.push(format!(r"{profile}\appdata"));
            exact.push(format!(r"{profile}\appdata\locallow"));
            if let Some(users) = Path::new(&profile).parent() {
                exact.push(norm_path(&users.to_string_lossy()));
            }
        }
        if let Some(local) = var("LOCALAPPDATA") {
            exact.push(format!(r"{local}\programs"));
        }
        SharedLocations { trees, exact }
    }

    /// `dir` ([`norm_path`] form) is a shared location.
    pub(crate) fn contains(&self, dir: &str) -> bool {
        // Drive roots and UNC share roots.
        if dir.len() <= 2 || !dir.contains('\\') {
            return true;
        }
        const ANYWHERE: [&str; 4] = [
            r"\windows\installer",
            r"\package cache",
            r"\installshield installation information",
            r"\common files",
        ];
        if ANYWHERE
            .iter()
            .any(|part| dir.contains(&format!("{part}\\")) || dir.ends_with(part))
        {
            return true;
        }
        self.trees.iter().any(|tree| is_within(dir, tree)) || self.exact.iter().any(|e| e == dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(hive: Hive, key: &str) -> InstalledProgram {
        InstalledProgram {
            hive,
            key_name: key.into(),
            display_name: "Test".into(),
            display_version: None,
            publisher: None,
            install_location: None,
            display_icon: None,
            uninstall_string: None,
            url_info_about: None,
            url_update_info: None,
            help_link: None,
            install_date: None,
            system_component: false,
            windows_installer: false,
            is_update: false,
        }
    }

    #[test]
    fn winget_arp_ids_match_winget_list() {
        // Verified against `winget list` output on a Windows 11 x64 machine.
        assert_eq!(
            program(Hive::Hklm64, "RustDesk").winget_arp_id(),
            r"ARP\Machine\X64\RustDesk"
        );
        assert_eq!(
            program(Hive::Hklm32, "FileZilla Client").winget_arp_id(),
            r"ARP\Machine\X86\FileZilla Client"
        );
        assert_eq!(
            program(Hive::Hkcu, "9c41d802bcbbc024e1916717565ad00d").winget_arp_id(),
            r"ARP\User\X64\9c41d802bcbbc024e1916717565ad00d"
        );
        assert_eq!(program(Hive::Hklm32, "{ABC}").reg_id(), r"HKLM32\{ABC}");
        assert_eq!(program(Hive::Hkcu, "x").scope(), Scope::User);
    }

    #[test]
    fn icon_locations() {
        let p = |s: &str| parse_icon_location(s).map(|(p, i)| (p.to_string_lossy().into_owned(), i));
        assert_eq!(p(r#""C:\x y\app.exe",0"#), Some((r"C:\x y\app.exe".into(), 0)));
        assert_eq!(p(r#""C:\x y\app.exe", 2"#), Some((r"C:\x y\app.exe".into(), 2)));
        assert_eq!(p(r#""C:\x y\app.exe,3""#), Some((r"C:\x y\app.exe".into(), 3)));
        assert_eq!(p(r"C:\x y\app.exe,-101"), Some((r"C:\x y\app.exe".into(), -101)));
        assert_eq!(p(r"C:\x y\app.ico"), Some((r"C:\x y\app.ico".into(), 0)));
        assert_eq!(p(r"C:\a,b\app.exe"), Some((r"C:\a,b\app.exe".into(), 0)));
        assert_eq!(p("  "), None);
        let windir = std::env::var("SystemRoot").unwrap_or_default();
        assert_eq!(
            p(r"%SystemRoot%\explorer.exe,0"),
            Some((format!(r"{windir}\explorer.exe"), 0))
        );
    }

    #[test]
    fn command_executables() {
        let c = |s: &str| command_exe(s).map(|p| p.to_string_lossy().into_owned());
        assert_eq!(
            c(r#""C:\Program Files\App\unins000.exe" /SILENT"#).as_deref(),
            Some(r"C:\Program Files\App\unins000.exe")
        );
        assert_eq!(
            c(r"C:\Program Files\App\uninst.exe /S").as_deref(),
            Some(r"C:\Program Files\App\uninst.exe")
        );
        assert_eq!(
            c(r"C:\Program Files\App\uninst.EXE").as_deref(),
            Some(r"C:\Program Files\App\uninst.EXE")
        );
        assert_eq!(c(r"C:\x.exed\real.exe --x").as_deref(), Some(r"C:\x.exed\real.exe"));
        assert_eq!(c("MsiExec.exe /X{1234}").as_deref(), Some("MsiExec.exe"));
        assert_eq!(c(r"C:\tools\remove.bat now").as_deref(), Some(r"C:\tools\remove.bat"));
        assert_eq!(c(""), None);
    }

    #[test]
    fn dates() {
        assert_eq!(normalize_date("20240131").as_deref(), Some("20240131"));
        assert_eq!(normalize_date("2024-01-31").as_deref(), Some("20240131"));
        assert_eq!(normalize_date("2024/1/31"), None);
        assert_eq!(normalize_date("31/01/2024"), None);
        assert_eq!(normalize_date("20241331"), None);
        assert_eq!(normalize_date("Jan 2024"), None);
    }

    #[test]
    fn paths() {
        assert_eq!(norm_path(r"\\?\C:/Program Files/App\"), r"c:\program files\app");
        assert!(is_within(r"c:\app\bin\x.exe", r"c:\app"));
        assert!(is_within(r"c:\app", r"c:\app"));
        assert!(!is_within(r"c:\application\x.exe", r"c:\app"));
        assert!(!is_within(r"c:\app\x.exe", ""));
        assert_eq!(trim_trailing_separators(r"C:\App\\"), r"C:\App");
        assert_eq!(trim_trailing_separators(r"C:\"), r"C:\");
    }

    #[test]
    fn shared_locations() {
        let shared = SharedLocations {
            trees: vec![r"c:\windows".into(), r"c:\users\me\appdata\local\temp".into()],
            exact: vec![r"c:\program files".into(), r"c:\users\me\appdata\local".into()],
        };
        for dir in [
            r"c:",
            r"c:\windows\system32",
            r"c:\windows\installer\{guid}",
            r"c:\programdata\package cache\{guid}",
            r"d:\programdata\package cache\{guid}\sub",
            r"c:\program files (x86)\common files\vendor",
            r"c:\program files (x86)\installshield installation information\{guid}",
            r"c:\program files",
            r"c:\users\me\appdata\local",
            r"c:\users\me\appdata\local\temp\is-123",
        ] {
            assert!(shared.contains(dir), "{dir} should be shared");
        }
        for dir in [
            r"c:\program files\app",
            r"c:\users\me\appdata\local\discord",
            r"d:\games\thing",
        ] {
            assert!(!shared.contains(dir), "{dir} should not be shared");
        }
    }

    #[test]
    fn install_dirs_filters_and_dedupes() {
        let crate_dir = env!("CARGO_MANIFEST_DIR");
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let mut p = program(Hive::Hklm64, "x");
        p.install_location = Some(format!("\"{}\\\"", crate_dir.to_uppercase()));
        p.display_icon = Some(format!("\"{crate_dir}\\Cargo.toml\",0"));
        p.uninstall_string = Some(format!(r"{windir}\System32\rundll32.exe shell32.dll,Control_RunDLL"));
        let dirs = p.install_dirs();
        assert_eq!(dirs.len(), 1, "{dirs:?}");
        assert_eq!(norm_path(&dirs[0].to_string_lossy()), norm_path(crate_dir));

        let mut q = program(Hive::Hklm64, "y");
        q.install_location = Some(format!(r"{crate_dir}\does-not-exist"));
        q.display_icon = Some(format!(r"{windir}\explorer.exe,0"));
        q.uninstall_string = Some(format!(r#""{crate_dir}\src\unins000.exe" /SILENT"#));
        let dirs = q.install_dirs();
        assert_eq!(dirs, vec![PathBuf::from(format!(r"{crate_dir}\src"))]);

        let mut r = program(Hive::Hklm64, "z");
        r.uninstall_string = Some("MsiExec.exe /X{00000000-0000-0000-0000-000000000000}".into());
        assert!(r.install_dirs().is_empty());
    }

    #[test]
    fn reads_the_real_registry() {
        let programs = installed_programs();
        assert!(!programs.is_empty(), "no program found in the Uninstall keys");
        for p in &programs {
            assert!(!p.display_name.trim().is_empty());
            assert!(!p.key_name.is_empty());
            if let Some(date) = &p.install_date {
                assert_eq!(date.len(), 8);
            }
        }
    }
}
