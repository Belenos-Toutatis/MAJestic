//! Other package managers: Chocolatey, Scoop, Microsoft Store.
//!
//! OWNER: package-managers agent.

pub mod choco;
pub mod scoop;
pub mod store;

use crate::model::{CommandSpec, JobRequest, Source};

/// Builds the command for any job, dispatching on its source.
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    if matches!(job.action, crate::model::JobAction::StoreUpdateScan) {
        return Ok(store::update_scan_command());
    }
    match job.source {
        Source::Winget | Source::MsStore | Source::Registry => crate::winget::build_command(job),
        Source::Chocolatey => choco::build_command(job),
        Source::Scoop => scoop::build_command(job),
        Source::Msix => anyhow::bail!("MSIX apps are updated by the Microsoft Store"),
    }
}

/// Accepts package ids made of `[A-Za-z0-9._+-]` that cannot be mistaken for
/// an option (no leading `-`).
pub(crate) fn check_id(id: &str) -> anyhow::Result<()> {
    let valid = !id.is_empty()
        && id.len() <= 256
        && !id.starts_with('-')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    anyhow::ensure!(valid, "invalid package id: {id:?}");
    Ok(())
}

/// Accepts versions that cannot be mistaken for an option and contain none of
/// the characters winget manifests forbid (`\ / : * ? " < > |`, controls).
pub(crate) fn check_version(version: &str) -> anyhow::Result<()> {
    let valid = !version.trim().is_empty()
        && version.len() <= 128
        && !version.starts_with('-')
        && !version
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
    anyhow::ensure!(valid, "invalid package version: {version:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{JobAction, Scope};

    #[test]
    fn id_validation() {
        for good in ["Git.Git", "Notepad++.Notepad++", "python313", "7zip", "Foo_Bar-1.2"] {
            assert!(check_id(good).is_ok(), "{good:?}");
        }
        for bad in ["", "-y", "--force", "a b", "a;b", "a&b", "a|b", "a\"b", "a\\b", "a/b", "éa", "a@1.0"] {
            assert!(check_id(bad).is_err(), "{bad:?}");
        }
        assert!(check_id(&"a".repeat(257)).is_err());
    }

    #[test]
    fn version_validation() {
        for good in ["1.2.3", "2.11.0-insider", "1.0 beta", "14.38.33135.0", "1.2.3+build"] {
            assert!(check_version(good).is_ok(), "{good:?}");
        }
        for bad in ["", " ", "-1", "--force", "1.0|calc", "1.0\"", "1/2", "1\n2"] {
            assert!(check_version(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn dispatch() {
        let mut job = JobRequest {
            key: "msix:Contoso.App".into(),
            source: Source::Msix,
            id: "Contoso.App".into(),
            name: "Contoso App".into(),
            action: JobAction::Upgrade,
            from_version: None,
            to_version: None,
            scope: Some(Scope::User),
            include_unknown: false,
            include_pinned: false,
            download_size: None,
            installer_type: None,
        };
        assert!(build_command(&job).is_err());
        job.action = JobAction::StoreUpdateScan;
        assert_eq!(build_command(&job).unwrap(), store::update_scan_command());
        job.source = Source::Chocolatey;
        job.action = JobAction::Upgrade;
        assert_eq!(build_command(&job).unwrap().program, "choco");
        job.source = Source::Winget;
        assert_eq!(build_command(&job).unwrap().program, "winget");
    }
}
