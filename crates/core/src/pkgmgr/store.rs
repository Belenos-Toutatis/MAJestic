//! Microsoft Store.
//!
//! OWNER: package-managers agent.

use crate::model::CommandSpec;

/// Opens the Store's "Downloads and updates" page.
pub const STORE_UPDATES_URI: &str = "ms-windows-store://downloadsandupdates";

/// Asks the Store, through the MDM bridge, to scan for and install app updates.
const UPDATE_SCAN_SCRIPT: &str = "Get-CimInstance -Namespace 'Root\\cimv2\\mdm\\dmmap' \
     -ClassName 'MDM_EnterpriseModernAppManagement_AppManagement01' \
     | Invoke-CimMethod -MethodName UpdateScanMethod";

/// PowerShell command asking the Store to scan for and install app updates
/// (MDM bridge `UpdateScanMethod`; needs elevation).
pub fn update_scan_command() -> CommandSpec {
    CommandSpec {
        program: "powershell".to_string(),
        args: ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", UPDATE_SCAN_SCRIPT]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_scan() {
        let spec = update_scan_command();
        assert_eq!(spec.program, "powershell");
        assert_eq!(spec.args[..4], ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"]);
        assert_eq!(
            spec.args[4],
            "Get-CimInstance -Namespace 'Root\\cimv2\\mdm\\dmmap' -ClassName \
             'MDM_EnterpriseModernAppManagement_AppManagement01' | Invoke-CimMethod -MethodName UpdateScanMethod"
        );
    }
}
