//! Shell helpers.
//!
//! OWNER: system agent.

/// Opens a URL with the default handler. Only `https:`, `http:` and
/// `ms-windows-store:` schemes are allowed.
pub fn open_url(url: &str) -> anyhow::Result<()> {
    let _ = url;
    todo!("shell::open_url")
}

/// Opens a folder or selects a file in Explorer.
pub fn reveal(path: &std::path::Path) -> anyhow::Result<()> {
    let _ = path;
    todo!("shell::reveal")
}

/// Windows UI language as a BCP-47 tag (`fr-FR`).
pub fn ui_language() -> String {
    todo!("shell::ui_language")
}

/// Human description of the OS, e.g. `Windows 11 Home 24H2 (build 26100)`.
pub fn os_description() -> String {
    todo!("shell::os_description")
}
