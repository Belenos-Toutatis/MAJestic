//! Microsoft Store.
//!
//! OWNER: package-managers agent.

use crate::model::CommandSpec;

/// Opens the Store's "Downloads and updates" page.
pub const STORE_UPDATES_URI: &str = "ms-windows-store://downloadsandupdates";

/// PowerShell command asking the Store to scan for and install app updates
/// (MDM bridge `UpdateScanMethod`; needs elevation).
pub fn update_scan_command() -> CommandSpec {
    todo!("store::update_scan_command")
}
