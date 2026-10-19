//! Program icons for the UI.
//!
//! OWNER: system agent.

/// Extracts the icon designated by `hint` (a `DisplayIcon` value such as
/// `C:\app\app.exe,0`, or a plain .exe/.ico path, possibly quoted) and returns
/// it as a `data:image/png;base64,...` URL of `size`×`size` pixels (32 or 48),
/// with alpha. `None` when nothing usable is found.
pub fn icon_data_url(hint: &str, size: u32) -> Option<String> {
    let _ = (hint, size);
    todo!("icons::icon_data_url")
}
