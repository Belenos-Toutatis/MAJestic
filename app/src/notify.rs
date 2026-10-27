//! Windows notifications for the scheduled check.
//!
//! An unpackaged app can show toasts under its own name by registering an
//! AppUserModelID in `HKCU\Software\Classes\AppUserModelId`. Clicking the
//! toast opens the `majestic:` URL, registered to start MAJestic. Both
//! registrations are per-user, created when the scheduled check is turned on
//! and removed when it is turned off.

use std::path::Path;

use windows::core::{HSTRING, PCWSTR};
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

pub const AUMID: &str = "MAJestic.UpdateAssistant";
const AUMID_KEY: &str = r"Software\Classes\AppUserModelId\MAJestic.UpdateAssistant";
const PROTOCOL_KEY: &str = r"Software\Classes\majestic";
const ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");

fn set_string(subkey: &str, name: Option<&str>, value: &str) -> anyhow::Result<()> {
    let mut key = HKEY::default();
    // SAFETY: valid strings and out pointer; the key is closed below.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .ok()?;
    }
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: reinterpreting a u16 buffer as bytes, same lifetime.
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    let name = name.map(HSTRING::from);
    let name_ptr = name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null());
    // SAFETY: `key` is open; `bytes` outlives the call.
    let result = unsafe { RegSetValueExW(key, name_ptr, None, REG_SZ, Some(bytes)) };
    // SAFETY: closing the key we opened.
    let _ = unsafe { RegCloseKey(key) };
    result.ok()?;
    Ok(())
}

/// Registers the notification identity and the `majestic:` protocol.
pub fn register(exe: &Path) -> anyhow::Result<()> {
    let icon = majestic_core::paths::data_dir().join("majestic.png");
    if !icon.exists() {
        std::fs::write(&icon, ICON_PNG)?;
    }
    set_string(AUMID_KEY, Some("DisplayName"), "MAJestic")?;
    set_string(AUMID_KEY, Some("IconUri"), &icon.to_string_lossy())?;
    set_string(PROTOCOL_KEY, None, "URL:MAJestic")?;
    set_string(PROTOCOL_KEY, Some("URL Protocol"), "")?;
    let command = format!("\"{}\" \"%1\"", exe.display());
    set_string(&format!(r"{PROTOCOL_KEY}\shell\open\command"), None, &command)?;
    Ok(())
}

/// The program the `majestic:` protocol starts, when registered.
pub fn registered_exe() -> Option<std::path::PathBuf> {
    let command = majestic_core::system::registry::user_string(&format!(r"{PROTOCOL_KEY}\shell\open\command"), "")?;
    let exe = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => command.split_whitespace().next()?,
    };
    Some(std::path::PathBuf::from(exe))
}

pub fn unregister() -> anyhow::Result<()> {
    for key in [AUMID_KEY, PROTOCOL_KEY] {
        // SAFETY: deleting a per-user subtree we created; missing is fine.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key)) };
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Shows a toast; clicking it (or its button) opens MAJestic.
pub fn show_toast(title: &str, body: &str, button: &str) -> anyhow::Result<()> {
    let xml = format!(
        r#"<toast activationType="protocol" launch="majestic:open">
  <visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>
  <actions><action content="{}" activationType="protocol" arguments="majestic:open"/></actions>
</toast>"#,
        xml_escape(title),
        xml_escape(body),
        xml_escape(button)
    );
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?;
    notifier.Show(&toast)?;
    Ok(())
}
