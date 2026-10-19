//! Windows integration: installed programs, files, icons, processes,
//! elevation and shell helpers.
//!
//! OWNER: system agent.

pub mod classify;
pub mod elevation;
pub mod fileinfo;
pub mod icons;
pub mod procs;
pub mod registry;
pub mod shell;

use std::ffi::OsStr;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};

/// NUL-terminated UTF-16 copy of `s`, for `PCWSTR` arguments.
pub(crate) fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// String from a UTF-16 buffer, up to the first NUL (or the whole buffer).
pub(crate) fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Keeps COM initialized (single-threaded apartment) on the current thread
/// for its lifetime, as the shell APIs (`ShellExecute*`, `SHGetFileInfoW`)
/// require. When the thread already uses another apartment model, COM is
/// left alone (and is usable anyway).
pub(crate) struct ComGuard {
    initialized: bool,
    /// COM initialization is per thread: keep the guard on its thread.
    _not_send: PhantomData<*const ()>,
}

impl ComGuard {
    pub(crate) fn new() -> ComGuard {
        // SAFETY: plain call; every success (S_OK or S_FALSE) is balanced by
        // CoUninitialize in Drop, on the same thread (the guard is !Send).
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        ComGuard {
            initialized: hr.is_ok(),
            _not_send: PhantomData,
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: balances the successful CoInitializeEx of `new`.
            unsafe { CoUninitialize() };
        }
    }
}
