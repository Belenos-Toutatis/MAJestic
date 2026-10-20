//! The Windows "Save as" and "Open" dialogs (IFileSaveDialog,
//! IFileOpenDialog), shown on their own COM thread.

use std::path::{Path, PathBuf};

use anyhow::anyhow;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST,
    SIGDN_FILESYSPATH,
};

/// What the dialog shows and where.
pub struct Dialog<'a> {
    /// Parent window handle (`0`: none).
    pub owner: isize,
    pub title: &'a str,
    /// Folder shown first.
    pub folder: Option<&'a Path>,
    /// Proposed file name ("Save as" only).
    pub file_name: &'a str,
    /// File type shown, e.g. `("Liste MAJestic", "*.json")`.
    pub filter: (&'a str, &'a str),
    /// Extension added when the user types none, without dot.
    pub extension: &'a str,
}

/// "Save as": the chosen path, `None` when cancelled.
pub fn save(d: &Dialog) -> anyhow::Result<Option<PathBuf>> {
    on_com_thread(|| unsafe {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        prepare(&dialog, d, FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST)?;
        dialog.SetFileName(&HSTRING::from(d.file_name))?;
        show(&dialog, d.owner)
    })
}

/// "Open": the chosen file, `None` when cancelled.
pub fn open(d: &Dialog) -> anyhow::Result<Option<PathBuf>> {
    on_com_thread(|| unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        prepare(&dialog, d, FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST)?;
        show(&dialog, d.owner)
    })
}

unsafe fn prepare(
    dialog: &IFileDialog,
    d: &Dialog,
    options: windows::Win32::UI::Shell::FILEOPENDIALOGOPTIONS,
) -> anyhow::Result<()> {
    dialog.SetOptions(dialog.GetOptions()? | options | FOS_FORCEFILESYSTEM)?;
    dialog.SetTitle(&HSTRING::from(d.title))?;
    let (name, pattern) = (HSTRING::from(d.filter.0), HSTRING::from(d.filter.1));
    let spec = COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(pattern.as_ptr()) };
    dialog.SetFileTypes(&[spec])?;
    if !d.extension.is_empty() {
        dialog.SetDefaultExtension(&HSTRING::from(d.extension))?;
    }
    if let Some(folder) = d.folder.filter(|f| f.is_dir()) {
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(folder.as_os_str()), None) {
            let _ = dialog.SetFolder(&item);
        }
    }
    Ok(())
}

unsafe fn show(dialog: &IFileDialog, owner: isize) -> anyhow::Result<Option<PathBuf>> {
    let parent = (owner != 0).then_some(HWND(owner as *mut _));
    if let Err(e) = dialog.Show(parent) {
        if e.code() == ERROR_CANCELLED.to_hresult() {
            return Ok(None);
        }
        return Err(e.into());
    }
    let item = dialog.GetResult()?;
    let name = item.GetDisplayName(SIGDN_FILESYSPATH)?;
    let path = name.to_string();
    CoTaskMemFree(Some(name.0 as *const _));
    Ok(Some(PathBuf::from(path?)))
}

/// Runs `f` on a new single-threaded COM apartment, as the dialogs require.
fn on_com_thread<T: Send>(f: impl FnOnce() -> anyhow::Result<T> + Send) -> anyhow::Result<T> {
    std::thread::scope(|s| {
        s.spawn(|| unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let result = f();
            if init.is_ok() {
                CoUninitialize();
            }
            result
        })
        .join()
        .map_err(|_| anyhow!("the file dialog thread stopped"))?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

    /// Opens each dialog and closes it right away, as "Cancel" would.
    /// Shows a window for a moment: run with `--ignored`.
    #[test]
    #[ignore = "shows a dialog for a moment"]
    fn dialogs_open_and_cancel() {
        for (save_it, title) in [(false, "MAJestic test open"), (true, "MAJestic test save")] {
            let closer = std::thread::spawn(move || {
                for _ in 0..80 {
                    std::thread::sleep(Duration::from_millis(100));
                    if let Ok(hwnd) = unsafe { FindWindowW(None, &HSTRING::from(title)) } {
                        if !hwnd.is_invalid() {
                            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
                            return true;
                        }
                    }
                }
                false
            });
            let folder = std::env::temp_dir();
            let dialog = Dialog {
                owner: 0,
                title,
                folder: Some(&folder),
                file_name: "Test.json",
                filter: ("Liste", "*.json"),
                extension: "json",
            };
            let chosen = if save_it { save(&dialog) } else { open(&dialog) };
            assert!(closer.join().unwrap(), "{title}: the dialog never appeared");
            assert_eq!(chosen.unwrap(), None, "{title}");
        }
    }
}
