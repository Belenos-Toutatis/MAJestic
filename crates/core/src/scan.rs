//! Scan orchestration: builds the full [`ScanResult`].
//!
//! OWNER: lead (integration).

use crate::model::{ScanProgress, ScanResult, UpdateItem};
use crate::settings::Settings;

/// Full scan. Reports progress through `progress`.
pub fn scan(settings: &Settings, progress: &dyn Fn(ScanProgress)) -> ScanResult {
    let _ = (settings, progress);
    todo!("scan::scan")
}

/// Background phase: looks for newer versions of unmanaged programs and
/// calls `on_item` with every item that changed.
pub fn lookup_unmanaged(items: &[UpdateItem], progress: &dyn Fn(ScanProgress), on_item: &dyn Fn(UpdateItem)) {
    let _ = (items, progress, on_item);
    todo!("scan::lookup_unmanaged")
}
