//! Updates waiting in Windows Update, read through the Windows Update Agent
//! API (wuapi.dll), which needs no administrator rights to search. MAJestic
//! only reports them: installing stays with Windows Update (Settings), which
//! handles ordering, failures and restarts far better.

use windows::core::{Interface, BSTR};
use windows::Win32::Foundation::{DECIMAL, VARIANT_BOOL, VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::UpdateAgent::{
    irbNeverReboots, orcSucceeded, orcSucceededWithErrors, utDriver, AutomaticUpdates, IAutomaticUpdates2,
    IStringCollection, ISystemInformation, IUpdate, IUpdate3, IUpdateSession, SystemInformation, UpdateSession,
};
use windows::Win32::System::Variant::{VARIANT, VT_DATE};

use crate::model::{WindowsUpdate, WindowsUpdateKind, WindowsUpdates};
use crate::system::ComGuard;

/// Everything applicable and not hidden by the user (drivers included).
const CRITERIA: &str = "IsInstalled=0 and IsHidden=0";

// Update classification categories (stable ids, the names are localized).
const SECURITY: &str = "0fa1201d-4330-4fa8-8ae9-b877473b6441";
const CRITICAL: &str = "e6cf1350-c01b-414d-a61f-263d14d133b4";
const DEFINITION: &str = "e0789628-ce08-4437-be74-2495b842f43b";
const DRIVERS: &str = "ebfc1fc5-71a4-4f7b-9aca-3b9a503104a0";
const FEATURE_PACKS: &str = "b54e7d24-7add-428f-8b75-90a396fa584f";
const UPGRADES: &str = "3689bdc8-b205-4af4-8d4a-a63924c5e9d5";
const SERVICE_PACKS: &str = "68c5b0a3-d1a6-4553-ae49-01d3a7827828";

/// What Windows Update has waiting. `online`: search Microsoft's servers
/// (like "Check for updates"); when that fails, Windows' cached results of
/// its last search are used. Blocking: an online search can take minutes.
pub fn pending(online: bool) -> WindowsUpdates {
    let _com = ComGuard::new();
    let mut out = WindowsUpdates { checked_at: chrono::Local::now().to_rfc3339(), ..Default::default() };
    out.reboot_pending = reboot_required().unwrap_or_else(|e| {
        log::debug!("windows update reboot state: {e}");
        false
    });
    match last_dates() {
        Ok((search, install)) => {
            out.last_search = search;
            out.last_install = install;
        }
        Err(e) => log::debug!("windows update dates: {e}"),
    }
    let result = match search(online) {
        Err(e) if online => {
            log::info!("windows update online search failed ({}), using cached results", describe(&e));
            search(false).map(|list| (list, false))
        }
        other => other.map(|list| (list, online)),
    };
    match result {
        Ok((list, online)) => {
            log::info!("windows update: {} pending (online: {online})", list.len());
            out.updates = list;
            out.online = online;
        }
        Err(e) => {
            log::warn!("windows update search failed: {}", describe(&e));
            out.error = Some(describe(&e));
        }
    }
    out
}

fn describe(e: &windows::core::Error) -> String {
    format!("{:#010X} {}", e.code().0, e.message().trim())
}

fn search(online: bool) -> windows::core::Result<Vec<WindowsUpdate>> {
    // SAFETY: COM is initialized on this thread (`pending`); every interface
    // comes from wuapi and is used on this thread only.
    unsafe {
        let session: IUpdateSession = CoCreateInstance(&UpdateSession, None, CLSCTX_ALL)?;
        let _ = session.SetClientApplicationID(&BSTR::from(crate::APP_NAME));
        let searcher = session.CreateUpdateSearcher()?;
        searcher.SetOnline(if online { VARIANT_TRUE } else { VARIANT_FALSE })?;
        let result = searcher.Search(&BSTR::from(CRITERIA))?;
        let code = result.ResultCode()?;
        if code != orcSucceeded && code != orcSucceededWithErrors {
            return Err(windows::core::Error::new(windows::Win32::Foundation::E_FAIL, format!("search result {}", code.0)));
        }
        let updates = result.Updates()?;
        let mut list = Vec::new();
        for i in 0..updates.Count()? {
            match updates.get_Item(i).and_then(|u| read_update(&u)) {
                Ok(update) => list.push(update),
                Err(e) => log::debug!("windows update #{i}: {}", describe(&e)),
            }
        }
        list.sort_by(|a, b| (a.optional, a.kind).cmp(&(b.optional, b.kind)).then_with(|| a.title.cmp(&b.title)));
        Ok(list)
    }
}

/// # Safety
/// `u` must be used on its COM thread.
unsafe fn read_update(u: &IUpdate) -> windows::core::Result<WindowsUpdate> {
    let mut classes = Vec::new();
    let categories = u.Categories()?;
    for j in 0..categories.Count()? {
        let category = categories.get_Item(j)?;
        if category.Type().map(|t| t.to_string()).unwrap_or_default() == "UpdateClassification" {
            classes.push(category.CategoryID()?.to_string());
        }
    }
    let is_driver = u.Type()? == utDriver;
    Ok(WindowsUpdate {
        title: u.Title()?.to_string(),
        kb: strings(&u.KBArticleIDs()?).into_iter().map(|k| format!("KB{k}")).collect(),
        kind: classify(&classes, is_driver),
        severity: u.MsrcSeverity().ok().map(|s| s.to_string()).filter(|s| !s.is_empty()),
        optional: u.cast::<IUpdate3>().and_then(|u3| u3.BrowseOnly()).map(truth).unwrap_or(false),
        downloaded: u.IsDownloaded().map(truth).unwrap_or(false),
        size: u.MaxDownloadSize().ok().and_then(decimal_u64).filter(|&n| n > 0),
        reboot: u.InstallationBehavior().and_then(|b| b.RebootBehavior()).map(|r| r != irbNeverReboots).unwrap_or(false),
        date: u.LastDeploymentChangeTime().ok().and_then(ole_date).map(|d| d.date().to_string()),
        support_url: u
            .SupportUrl()
            .ok()
            .map(|s| s.to_string())
            .filter(|s| s.starts_with("https://") || s.starts_with("http://")),
    })
}

/// # Safety
/// `c` must be used on its COM thread.
unsafe fn strings(c: &IStringCollection) -> Vec<String> {
    let n = c.Count().unwrap_or(0);
    (0..n).filter_map(|i| c.get_Item(i).ok()).map(|s| s.to_string()).filter(|s| !s.is_empty()).collect()
}

fn reboot_required() -> windows::core::Result<bool> {
    // SAFETY: COM is initialized on this thread (`pending`).
    unsafe {
        let info: ISystemInformation = CoCreateInstance(&SystemInformation, None, CLSCTX_ALL)?;
        info.RebootRequired().map(truth)
    }
}

/// Windows' own last successful search and installation (RFC 3339, UTC).
fn last_dates() -> windows::core::Result<(Option<String>, Option<String>)> {
    // SAFETY: COM is initialized on this thread (`pending`).
    unsafe {
        let au: IAutomaticUpdates2 = CoCreateInstance(&AutomaticUpdates, None, CLSCTX_ALL)?;
        let results = au.Results()?;
        let utc = |v: windows::core::Result<VARIANT>| {
            v.ok().and_then(|v| variant_date(&v)).and_then(ole_date).map(|d| d.and_utc().to_rfc3339())
        };
        Ok((utc(results.LastSearchSuccessDate()), utc(results.LastInstallationSuccessDate())))
    }
}

fn variant_date(v: &VARIANT) -> Option<f64> {
    // SAFETY: the union is read according to its type tag.
    unsafe {
        let inner = &v.Anonymous.Anonymous;
        (inner.vt == VT_DATE).then(|| inner.Anonymous.date)
    }
}

fn truth(b: VARIANT_BOOL) -> bool {
    b.0 != 0
}

/// COM `DECIMAL` holding a byte count.
fn decimal_u64(d: DECIMAL) -> Option<u64> {
    // SAFETY: both unions are plain integers whatever the variant.
    let (lo, scale) = unsafe { (d.Anonymous2.Lo64, d.Anonymous1.Anonymous.scale) };
    if d.Hi32 != 0 || scale > 19 {
        return None;
    }
    Some(lo / 10u64.pow(u32::from(scale)))
}

/// OLE automation date (days since 1899-12-30).
fn ole_date(days: f64) -> Option<chrono::NaiveDateTime> {
    if !days.is_finite() || days <= 0.0 {
        return None;
    }
    let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    base.checked_add_signed(chrono::Duration::milliseconds((days * 86_400_000.0).round() as i64))
}

fn classify(classes: &[String], is_driver: bool) -> WindowsUpdateKind {
    let has = |id: &str| classes.iter().any(|c| c.eq_ignore_ascii_case(id));
    if has(SECURITY) {
        WindowsUpdateKind::Security
    } else if has(DEFINITION) {
        WindowsUpdateKind::Definition
    } else if is_driver || has(DRIVERS) {
        WindowsUpdateKind::Driver
    } else if has(CRITICAL) {
        WindowsUpdateKind::Critical
    } else if has(UPGRADES) || has(FEATURE_PACKS) || has(SERVICE_PACKS) {
        WindowsUpdateKind::Feature
    } else {
        WindowsUpdateKind::Update
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{DECIMAL_0, DECIMAL_0_0, DECIMAL_1};

    #[test]
    fn classification() {
        let v = |ids: &[&str]| ids.iter().map(|s| s.to_uppercase()).collect::<Vec<_>>();
        assert_eq!(classify(&v(&[SECURITY]), false), WindowsUpdateKind::Security);
        assert_eq!(classify(&v(&[DEFINITION]), false), WindowsUpdateKind::Definition);
        assert_eq!(classify(&[], true), WindowsUpdateKind::Driver);
        assert_eq!(classify(&v(&[CRITICAL]), false), WindowsUpdateKind::Critical);
        assert_eq!(classify(&v(&[UPGRADES]), false), WindowsUpdateKind::Feature);
        assert_eq!(classify(&[], false), WindowsUpdateKind::Update);
    }

    #[test]
    fn conversions() {
        let d = |lo: u64, scale: u8| DECIMAL {
            wReserved: 0,
            Anonymous1: DECIMAL_0 { Anonymous: DECIMAL_0_0 { scale, sign: 0 } },
            Hi32: 0,
            Anonymous2: DECIMAL_1 { Lo64: lo },
        };
        assert_eq!(decimal_u64(d(123_456_789, 0)), Some(123_456_789));
        assert_eq!(decimal_u64(d(1_000, 2)), Some(10));
        assert_eq!(ole_date(45000.5).unwrap().to_string(), "2023-03-15 12:00:00");
        assert_eq!(ole_date(0.0), None);
    }

    #[test]
    #[ignore = "queries the Windows Update service"]
    fn cached_results() {
        let w = pending(false);
        println!("{w:#?}");
        assert!(w.error.is_none());
    }
}
