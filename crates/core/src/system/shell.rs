//! Shell helpers.
//!
//! OWNER: system agent.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Globalization::{
    GetUserDefaultUILanguage, GetUserPreferredUILanguages, LCIDToLocaleName, MUI_LANGUAGE_NAME,
};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::system::registry::RegKey;
use crate::system::{from_wide, wide, ComGuard};

/// Pages of the Settings app MAJestic opens (fixed addresses, never input).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    /// Windows Update, starting a search ("Check for updates").
    WindowsUpdateCheck,
    /// Windows Update › Optional updates (drivers...).
    WindowsUpdateOptional,
    /// Apps › Installed apps (uninstall).
    InstalledApps,
}

impl SettingsPage {
    pub fn uri(self) -> &'static str {
        match self {
            SettingsPage::WindowsUpdateCheck => "ms-settings:windowsupdate-action",
            SettingsPage::WindowsUpdateOptional => "ms-settings:windowsupdate-optionalupdates",
            SettingsPage::InstalledApps => "ms-settings:appsfeatures",
        }
    }
}

/// Opens a page of the Settings app.
pub fn open_settings(page: SettingsPage) -> anyhow::Result<()> {
    shell_open(page.uri())
}

/// Opens a URL with the default handler. Only `https:`, `http:` and
/// `ms-windows-store:` schemes are allowed.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    shell_open(validate_url(url)?)
}

fn shell_open(url: &str) -> anyhow::Result<()> {
    let _com = ComGuard::new();
    let operation = wide("open");
    let file = wide(url);
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a fake HINSTANCE: > 32 means success.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        anyhow::bail!("cannot open {url} (ShellExecuteW error {code})")
    }
}

/// Checks a URL before handing it to the shell (which would happily run
/// `file:` paths or other protocol handlers).
fn validate_url(url: &str) -> anyhow::Result<&str> {
    let url = url.trim();
    if url.is_empty() || url.len() > 8192 || url.chars().any(char::is_control) {
        anyhow::bail!("invalid URL");
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        anyhow::bail!("URL without a scheme: {url}");
    };
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => {
            let authority = rest
                .strip_prefix("//")
                .map(|r| r.split(['/', '?', '#']).next().unwrap_or_default())
                .unwrap_or_default();
            let host = authority.rsplit('@').next().unwrap_or_default();
            if host.is_empty() || host.starts_with(':') || host.contains(['\\', ' ', '"']) {
                anyhow::bail!("invalid web address: {url}");
            }
        }
        "ms-windows-store" => {
            if rest.trim_start_matches('/').is_empty() {
                anyhow::bail!("invalid Store link: {url}");
            }
        }
        other => anyhow::bail!("URL scheme not allowed: {other}"),
    }
    Ok(url)
}

/// Opens a folder or selects a file in Explorer.
pub fn reveal(path: &std::path::Path) -> anyhow::Result<()> {
    let args = if path.is_file() {
        explorer_args(path, true)
    } else if path.is_dir() {
        explorer_args(path, false)
    } else {
        anyhow::bail!("{} does not exist", path.display());
    };
    let windir = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    Command::new(windir.join("explorer.exe"))
        .raw_arg(args)
        .spawn()
        .map_err(|e| anyhow::anyhow!("cannot start explorer: {e}"))?;
    Ok(())
}

/// Explorer command line: `/select,"C:\dir\file"` or `"C:\dir"`.
fn explorer_args(path: &Path, select: bool) -> String {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    // A trailing backslash would escape the closing quote; a drive root
    // (`C:\`) has no blank and needs no quotes.
    let trimmed = text.trim_end_matches('\\');
    let quoted = if trimmed.len() <= 2 {
        format!("{trimmed}\\")
    } else {
        format!("\"{trimmed}\"")
    };
    if select {
        format!("/select,{quoted}")
    } else {
        quoted
    }
}

/// Windows UI language as a BCP-47 tag (`fr-FR`).
pub fn ui_language() -> String {
    preferred_ui_language()
        .or_else(default_ui_language)
        .unwrap_or_else(|| "en-US".to_string())
}

fn preferred_ui_language() -> Option<String> {
    let mut count = 0u32;
    let mut len = 0u32;
    // SAFETY: size query (no buffer).
    unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut len) }.ok()?;
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: `buf` holds `len` characters.
    unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(buf.as_mut_ptr())), &mut len) }
        .ok()?;
    // A double-NUL-terminated list: the first entry is the preferred one.
    let first = from_wide(&buf);
    (!first.is_empty()).then_some(first)
}

fn default_ui_language() -> Option<String> {
    // SAFETY: plain call.
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
                              // SAFETY: `buf` is a valid mutable slice; a LANGID is a valid LCID with
                              // the default sort order.
    let written = unsafe { LCIDToLocaleName(u32::from(lang_id), Some(&mut buf), 0) };
    let name = from_wide(&buf);
    (written > 0 && !name.is_empty()).then_some(name)
}

/// Human description of the OS, e.g. `Windows 11 Home 24H2 (build 26100)`.
pub fn os_description() -> String {
    let Some(key) = RegKey::open(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        KEY_WOW64_64KEY,
    ) else {
        return "Windows".to_string();
    };
    let product = key.string("ProductName");
    let build = key.string("CurrentBuild").or_else(|| key.string("CurrentBuildNumber"));
    let display_version = key.string("DisplayVersion").or_else(|| key.string("ReleaseId"));
    describe_os(product.as_deref(), build.as_deref(), display_version.as_deref())
}

fn describe_os(product: Option<&str>, build: Option<&str>, display_version: Option<&str>) -> String {
    let build = build.map(str::trim).filter(|b| !b.is_empty());
    let mut name = product
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or("Windows")
        .to_string();
    // ProductName still says "Windows 10" on Windows 11.
    if build.and_then(|b| b.parse::<u32>().ok()).is_some_and(|b| b >= 22000) {
        if let Some(rest) = name.strip_prefix("Windows 10") {
            name = format!("Windows 11{rest}");
        }
    }
    if let Some(version) = display_version.map(str::trim).filter(|v| !v.is_empty()) {
        name.push(' ');
        name.push_str(version);
    }
    if let Some(build) = build {
        name.push_str(&format!(" (build {build})"));
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    // `open_url` and `reveal` are not called: they would open windows.

    #[test]
    fn url_validation() {
        for ok in [
            "https://example.com",
            "https://example.com/path?q=1#x",
            "HTTP://example.com:8080/",
            "https://user@example.com/",
            "ms-windows-store://downloadsandupdates",
            "ms-windows-store://pdp/?productid=9NBLGGH4NNS1",
        ] {
            assert!(validate_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "example.com",
            "file:///C:/Windows/System32/calc.exe",
            r"C:\Windows\System32\calc.exe",
            "javascript:alert(1)",
            "ms-settings:windowsupdate",
            "https:/example.com",
            "https://",
            "https:///path",
            "https://exa mple.com",
            "https://example.com/\nnext",
            "ms-windows-store:",
        ] {
            assert!(validate_url(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            validate_url("  https://example.com  ").ok(),
            Some("https://example.com")
        );
    }

    #[test]
    fn explorer_command_lines() {
        assert_eq!(
            explorer_args(Path::new(r"C:\Program Files\App\app.exe"), true),
            r#"/select,"C:\Program Files\App\app.exe""#
        );
        assert_eq!(
            explorer_args(Path::new(r"C:\Program Files\App\"), false),
            r#""C:\Program Files\App""#
        );
        assert_eq!(explorer_args(Path::new(r"\\?\C:\Data"), false), r#""C:\Data""#);
        assert_eq!(explorer_args(Path::new(r"C:\"), false), r"C:\");
    }

    #[test]
    fn reveal_rejects_missing_paths() {
        assert!(reveal(Path::new(r"C:\does\not\exist")).is_err());
    }

    #[test]
    fn os_descriptions() {
        assert_eq!(
            describe_os(Some("Windows 10 Home"), Some("26100"), Some("24H2")),
            "Windows 11 Home 24H2 (build 26100)"
        );
        assert_eq!(
            describe_os(Some("Windows 10 Pro"), Some("19045"), Some("22H2")),
            "Windows 10 Pro 22H2 (build 19045)"
        );
        assert_eq!(
            describe_os(Some("Windows 11 Pro"), Some("22631"), None),
            "Windows 11 Pro (build 22631)"
        );
        assert_eq!(describe_os(None, None, None), "Windows");
        let real = os_description();
        assert!(real.starts_with("Windows"), "{real}");
    }

    #[test]
    fn language_tag() {
        let tag = ui_language();
        let mut parts = tag.split('-');
        let language = parts.next().unwrap_or_default();
        assert!(
            (2..=3).contains(&language.len()) && language.chars().all(|c| c.is_ascii_lowercase()),
            "{tag}"
        );
        assert!(
            parts.all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric())),
            "{tag}"
        );
        assert!(default_ui_language().is_some());
    }
}
