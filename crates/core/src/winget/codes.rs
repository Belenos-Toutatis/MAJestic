//! Exit codes of winget / Chocolatey / Scoop → outcome.
//!
//! OWNER: package-managers agent.
//!
//! winget codes come from the official list
//! (`doc/windows/package-manager/winget/returnCodes.md` in winget-cli).
//! Installers' own codes (MSI, Windows errors) are recognized for every tool.

use crate::model::{FailureKind, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success { reboot_required: bool },
    Failure(FailureKind),
}

/// Interprets an exit code (unsigned convention, see `exec::Output::code`)
/// for the tool that produced it.
pub fn classify(source: Source, code: i64) -> Outcome {
    let Some(code) = to_u32(code) else {
        return Outcome::Failure(FailureKind::Unknown);
    };
    if code == 0 {
        return Outcome::Success { reboot_required: false };
    }
    let specific = match source {
        Source::Chocolatey => chocolatey(code),
        Source::Scoop => None,
        Source::Winget | Source::MsStore | Source::Registry | Source::Msix => winget(code),
    };
    specific.or_else(|| windows(code)).unwrap_or(match source {
        // Both exit with 1 (choco also with -1) whatever went wrong in the
        // package script or the installer.
        Source::Chocolatey | Source::Scoop => Outcome::Failure(FailureKind::InstallerFailed),
        _ => Outcome::Failure(FailureKind::Unknown),
    })
}

/// Accepts the unsigned convention and, defensively, signed 32-bit codes.
fn to_u32(code: i64) -> Option<u32> {
    u32::try_from(code).ok().or_else(|| i32::try_from(code).ok().map(|c| c as u32))
}

fn fail(kind: FailureKind) -> Option<Outcome> {
    Some(Outcome::Failure(kind))
}

const REBOOT: Option<Outcome> = Some(Outcome::Success { reboot_required: true });

/// winget's own HRESULTs (`APPINSTALLER_CLI_ERROR_*`).
fn winget(code: u32) -> Option<Outcome> {
    use FailureKind::*;
    match code {
        // INSTALL_REBOOT_REQUIRED_TO_FINISH, INSTALL_REBOOT_INITIATED
        0x8A15_0109 | 0x8A15_010B => REBOOT,
        // INSTALL_REBOOT_REQUIRED_FOR_INSTALL
        0x8A15_010A => fail(RebootRequired),
        // INSTALL_PACKAGE_IN_USE, INSTALL_FILE_IN_USE, INSTALL_PACKAGE_IN_USE_BY_APPLICATION
        0x8A15_0101 | 0x8A15_0103 | 0x8A15_0111 => fail(AppRunning),
        // INSTALL_INSTALL_IN_PROGRESS
        0x8A15_0102 => fail(InstallInProgress),
        // INSTALL_NO_NETWORK, DOWNLOAD_FAILED, DOWNLOAD_SIZE_MISMATCH, SOURCE_OPEN_FAILED,
        // FAILED_TO_OPEN_ALL_SOURCES, SERVICE_UNAVAILABLE, DISPLAYCATALOG_API_FAILED,
        // SFSCLIENT_API_FAILED, INSTALLER_ZERO_BYTE_FILE
        0x8A15_0107 | 0x8A15_0008 | 0x8A15_002E | 0x8A15_0045 | 0x8A15_004B | 0x8A15_006D | 0x8A15_007F
        | 0x8A15_0081 | 0x8A15_0086 => fail(NoNetwork),
        // INSTALL_DISK_FULL
        0x8A15_0105 => fail(DiskFull),
        // CTRL_SIGNAL_RECEIVED, APPTERMINATION_RECEIVED, INSTALL_CANCELLED_BY_USER,
        // AUTHENTICATION_CANCELLED_BY_USER
        0x8A15_0005 | 0x8A15_006A | 0x8A15_010C | 0x8A15_0077 => fail(CancelledByUser),
        // MSSTORE_BLOCKED_BY_POLICY, MSSTORE_APP_BLOCKED_BY_POLICY, INSTALLER_SECURITY_CHECK_FAILED,
        // BLOCKED_BY_POLICY, ARCHIVE_SCAN_FAILED, LICENSING_API_FAILED_FORBIDDEN,
        // INSTALL_BLOCKED_BY_POLICY
        0x8A15_001B | 0x8A15_001C | 0x8A15_002D | 0x8A15_003A | 0x8A15_0060 | 0x8A15_0085 | 0x8A15_010F => {
            fail(BlockedByPolicy)
        }
        // INSTALLER_HASH_MISMATCH, PINNED_CERTIFICATE_MISMATCH
        0x8A15_0011 | 0x8A15_005E => fail(HashMismatch),
        // NO_APPLICABLE_INSTALLER, NO_APPLICABLE_DISPLAYCATALOG_PACKAGE,
        // NO_APPLICABLE_SFSCLIENT_PACKAGE, SFSCLIENT_PACKAGE_NOT_SUPPORTED, INSTALL_SYSTEM_NOT_SUPPORTED
        0x8A15_0010 | 0x8A15_0080 | 0x8A15_0082 | 0x8A15_0084 | 0x8A15_0113 => fail(NoApplicableInstaller),
        // UPDATE_NOT_APPLICABLE, UPGRADE_VERSION_NOT_NEWER, UPGRADE_VERSION_UNKNOWN, PACKAGE_IS_PINNED,
        // UPDATE_INSTALL_TECHNOLOGY_MISMATCH, INSTALL_UPGRADE_NOT_SUPPORTED
        0x8A15_002B | 0x8A15_004F | 0x8A15_0050 | 0x8A15_0068 | 0x8A15_008E | 0x8A15_0114 => fail(UpgradeNotApplicable),
        // INSTALL_DOWNGRADE
        0x8A15_010E => fail(DowngradeRefused),
        // PORTABLE_PACKAGE_ALREADY_EXISTS, PACKAGE_ALREADY_INSTALLED, FONT_ALREADY_INSTALLED,
        // INSTALL_ALREADY_INSTALLED
        0x8A15_0054 | 0x8A15_0061 | 0x8A15_0089 | 0x8A15_010D => fail(AlreadyInstalled),
        // DEPENDENCIES_VALIDATION_FAILED, DOWNLOAD_DEPENDENCIES, INSTALL_MISSING_DEPENDENCY,
        // INSTALL_DEPENDENCIES
        0x8A15_004C | 0x8A15_006B | 0x8A15_0104 | 0x8A15_0110 => fail(MissingDependency),
        // SOURCE_NAME_DOES_NOT_EXIST, NO_APPLICATIONS_FOUND, MULTIPLE_APPLICATIONS_FOUND,
        // NO_MANIFEST_FOUND, NOT_ALL_PACKAGES_FOUND, MISSING_PACKAGE, NOT_ALL_QUERIES_FOUND_SINGLE
        0x8A15_0012 | 0x8A15_0014 | 0x8A15_0016 | 0x8A15_0017 | 0x8A15_0035 | 0x8A15_004D | 0x8A15_0067 => {
            fail(PackageNotFound)
        }
        // COMMAND_REQUIRES_ADMIN
        0x8A15_0019 => fail(ElevationRequired),
        // SHELLEXEC_INSTALL_FAILED, MSSTORE_INSTALL_FAILED, MSI_INSTALL_FAILED, PORTABLE_INSTALL_FAILED,
        // INSTALLER_PROHIBITS_ELEVATION, NESTEDINSTALLER_NOT_FOUND, EXTRACT_ARCHIVE_FAILED,
        // NESTEDINSTALLER_INVALID_PATH, MULTIPLE_INSTALL_FAILED, ADMIN_CONTEXT_ACTION_PROHIBITED,
        // FONT_INSTALL_FAILED, INSTALL_INSUFFICIENT_MEMORY, INSTALL_CONTACT_SUPPORT,
        // INSTALL_INVALID_PARAMETER, INSTALL_CUSTOM_ERROR
        0x8A15_0006 | 0x8A15_001E | 0x8A15_0049 | 0x8A15_0052 | 0x8A15_0056 | 0x8A15_005B | 0x8A15_005C
        | 0x8A15_005D | 0x8A15_0065 | 0x8A15_007D | 0x8A15_0087 | 0x8A15_0106 | 0x8A15_0108 | 0x8A15_0112
        | 0x8A15_0115 => fail(InstallerFailed),
        _ => None,
    }
}

/// Chocolatey (`https://docs.chocolatey.org/en-us/choco/commands/upgrade#exit-codes`);
/// its reboot codes (350, 1604, 1641, 3010) are the Windows ones.
fn chocolatey(code: u32) -> Option<Outcome> {
    match code {
        // Enhanced exit codes: nothing to do (already up to date).
        2 => Some(Outcome::Success { reboot_required: false }),
        _ => None,
    }
}

/// MSI and Windows codes passed through by installers, and their HRESULTs.
fn windows(code: u32) -> Option<Outcome> {
    use FailureKind::*;
    match code {
        // ERROR_SUCCESS_REBOOT_INITIATED, ERROR_SUCCESS_REBOOT_REQUIRED
        1641 | 3010 | 0x8007_0669 | 0x8007_0BC2 => REBOOT,
        // ERROR_FAIL_NOACTION_REBOOT, ERROR_INSTALL_SUSPEND
        350 | 1604 => fail(RebootRequired),
        // ERROR_INSTALL_USEREXIT
        1602 | 0x8007_0642 => fail(CancelledByUser),
        // ERROR_CANCELLED: typically the UAC prompt of the installer was declined.
        1223 | 0x8007_04C7 => fail(ElevationRefused),
        // ERROR_INSTALL_ALREADY_RUNNING
        1618 | 0x8007_0652 => fail(InstallInProgress),
        // ERROR_PRODUCT_VERSION (another version already installed)
        1638 | 0x8007_0666 => fail(AlreadyInstalled),
        // ERROR_INSTALL_PACKAGE_REJECTED, ERROR_PATCH_PACKAGE_REJECTED,
        // ERROR_INSTALL_TRANSFORM_REJECTED (software restriction policies)
        1625 | 1643 | 1644 | 0x8007_0659 | 0x8007_066B | 0x8007_066C => fail(BlockedByPolicy),
        // ERROR_ACCESS_DENIED, ERROR_ELEVATION_REQUIRED
        5 | 740 | 0x8007_0005 | 0x8007_02E4 => fail(ElevationRequired),
        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL
        39 | 112 | 0x8007_0027 | 0x8007_0070 => fail(DiskFull),
        // ERROR_SHARING_VIOLATION
        32 | 0x8007_0020 => fail(AppRunning),
        // ERROR_NETWORK_UNREACHABLE, WinINet timeout / name not resolved /
        // cannot connect / connection aborted / connection reset
        0x8007_04CF | 0x8007_2EE2 | 0x8007_2EE7 | 0x8007_2EFD | 0x8007_2EFE | 0x8007_2EFF => fail(NoNetwork),
        // ERROR_INSTALL_FAILURE
        1603 | 0x8007_0643 => fail(InstallerFailed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FailureKind::*;

    fn failure(source: Source, code: i64) -> Option<FailureKind> {
        match classify(source, code) {
            Outcome::Failure(kind) => Some(kind),
            Outcome::Success { .. } => None,
        }
    }

    #[test]
    fn success_and_reboot() {
        assert_eq!(classify(Source::Winget, 0), Outcome::Success { reboot_required: false });
        assert_eq!(classify(Source::Winget, 0x8A15_0109), Outcome::Success { reboot_required: true });
        assert_eq!(classify(Source::MsStore, 0x8A15_010B), Outcome::Success { reboot_required: true });
        assert_eq!(classify(Source::Winget, 3010), Outcome::Success { reboot_required: true });
        assert_eq!(classify(Source::Chocolatey, 1641), Outcome::Success { reboot_required: true });
        assert_eq!(classify(Source::Chocolatey, 3010), Outcome::Success { reboot_required: true });
        assert_eq!(classify(Source::Chocolatey, 2), Outcome::Success { reboot_required: false });
        assert_eq!(classify(Source::Scoop, 0), Outcome::Success { reboot_required: false });
    }

    #[test]
    fn winget_failures() {
        assert_eq!(failure(Source::Winget, 2_316_632_321), Some(AppRunning)); // 0x8A150101
        assert_eq!(failure(Source::Winget, 0x8A15_0102), Some(InstallInProgress));
        assert_eq!(failure(Source::Winget, 0x8A15_010A), Some(RebootRequired));
        assert_eq!(failure(Source::Winget, 0x8A15_0107), Some(NoNetwork));
        assert_eq!(failure(Source::Winget, 0x8A15_0008), Some(NoNetwork));
        assert_eq!(failure(Source::Winget, 0x8A15_0105), Some(DiskFull));
        assert_eq!(failure(Source::Winget, 0x8A15_010C), Some(CancelledByUser));
        assert_eq!(failure(Source::Winget, 0x8A15_010F), Some(BlockedByPolicy));
        assert_eq!(failure(Source::Winget, 0x8A15_003A), Some(BlockedByPolicy));
        assert_eq!(failure(Source::Winget, 0x8A15_0011), Some(HashMismatch));
        assert_eq!(failure(Source::Winget, 0x8A15_0010), Some(NoApplicableInstaller));
        assert_eq!(failure(Source::Winget, 0x8A15_002B), Some(UpgradeNotApplicable));
        assert_eq!(failure(Source::Winget, 0x8A15_010E), Some(DowngradeRefused));
        assert_eq!(failure(Source::Winget, 0x8A15_0061), Some(AlreadyInstalled));
        assert_eq!(failure(Source::Winget, 0x8A15_0104), Some(MissingDependency));
        assert_eq!(failure(Source::Winget, 0x8A15_0014), Some(PackageNotFound));
        assert_eq!(failure(Source::Winget, 0x8A15_0019), Some(ElevationRequired));
        assert_eq!(failure(Source::Winget, 0x8A15_0006), Some(InstallerFailed));
        assert_eq!(failure(Source::Registry, 0x8A15_0115), Some(InstallerFailed));
        assert_eq!(failure(Source::Winget, 0x8A15_0001), Some(Unknown));
        assert_eq!(failure(Source::Winget, 1), Some(Unknown));
    }

    #[test]
    fn signed_codes_are_tolerated() {
        assert_eq!(failure(Source::Winget, -1_978_334_975), Some(AppRunning));
        assert_eq!(failure(Source::Winget, i64::MAX), Some(Unknown));
    }

    #[test]
    fn installer_codes() {
        assert_eq!(failure(Source::Winget, 1618), Some(InstallInProgress));
        assert_eq!(failure(Source::Chocolatey, 1618), Some(InstallInProgress));
        assert_eq!(failure(Source::Chocolatey, 1602), Some(CancelledByUser));
        assert_eq!(failure(Source::Winget, 1223), Some(ElevationRefused));
        assert_eq!(failure(Source::Winget, 0x8007_0070), Some(DiskFull));
        assert_eq!(failure(Source::Chocolatey, 1603), Some(InstallerFailed));
    }

    #[test]
    fn chocolatey_and_scoop() {
        assert_eq!(failure(Source::Chocolatey, 350), Some(RebootRequired));
        assert_eq!(failure(Source::Chocolatey, 1604), Some(RebootRequired));
        assert_eq!(failure(Source::Chocolatey, 1), Some(InstallerFailed));
        assert_eq!(failure(Source::Chocolatey, 0xFFFF_FFFF), Some(InstallerFailed));
        assert_eq!(failure(Source::Scoop, 1), Some(InstallerFailed));
    }
}
