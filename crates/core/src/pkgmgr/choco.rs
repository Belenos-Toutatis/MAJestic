//! Chocolatey (`choco`), when installed.
//!
//! OWNER: package-managers agent.

use std::time::Duration;

use anyhow::bail;

use crate::exec;
use crate::model::{CommandSpec, JobAction, JobRequest, Source};
use crate::pkgmgr::{check_id, check_version};
use crate::version::Version;

const PROGRAM: &str = "choco";

/// `choco outdated` queries every configured feed.
const OUTDATED_TIMEOUT: Duration = Duration::from_secs(300);
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChocoRow {
    pub id: String,
    pub installed: String,
    pub available: String,
    pub pinned: bool,
}

/// Chocolatey version, or `None` when choco is not installed.
pub fn version() -> Option<String> {
    let out = exec::run(PROGRAM, &["--version"], VERSION_TIMEOUT).ok()?;
    if out.code != 0 {
        return None;
    }
    out.stdout.lines().map(str::trim).rfind(|line| Version::parse(line).is_some()).map(str::to_string)
}

/// `choco outdated -r --ignore-unfound` (`id|installed|available|pinned`).
pub fn outdated() -> anyhow::Result<Vec<ChocoRow>> {
    let out = exec::run(PROGRAM, &["outdated", "-r", "--ignore-unfound"], OUTDATED_TIMEOUT)?;
    let rows = parse_outdated(&out.stdout);
    // With enhanced exit codes, 2 means "outdated packages found".
    if !matches!(out.code, 0 | 2) && rows.is_empty() {
        bail!("choco outdated failed (exit code {}): {}", out.code, out.excerpt(3));
    }
    Ok(rows)
}

/// Every version of a package in the configured feeds, newest first
/// (`choco search <id> --exact --all-versions -r`).
pub fn list_versions(id: &str) -> anyhow::Result<Vec<String>> {
    check_id(id)?;
    let out = exec::run(PROGRAM, &["search", id, "--exact", "--all-versions", "-r"], OUTDATED_TIMEOUT)?;
    let mut versions: Vec<String> = out
        .stdout
        .lines()
        .filter_map(|line| line.trim().split_once('|'))
        .filter(|(pkg, v)| pkg.eq_ignore_ascii_case(id) && Version::parse(v).is_some())
        .map(|(_, v)| v.trim().to_string())
        .collect();
    if versions.is_empty() {
        bail!("no version of {id} found (exit code {}): {}", out.code, out.excerpt(3));
    }
    versions.sort_by(|a, b| crate::version::compare(b, a));
    versions.dedup();
    Ok(versions)
}

/// Parses `id|installed|available|pinned` lines, ignoring anything else.
fn parse_outdated(output: &str) -> Vec<ChocoRow> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.trim().split('|').map(str::trim).collect();
            let [id, installed, available, pinned] = fields.as_slice() else {
                return None;
            };
            if check_id(id).is_err() || installed.is_empty() || available.is_empty() {
                return None;
            }
            Some(ChocoRow {
                id: id.to_string(),
                installed: installed.to_string(),
                available: available.to_string(),
                pinned: pinned.eq_ignore_ascii_case("true"),
            })
        })
        .collect()
}

/// `choco upgrade <id> -y --no-progress` (or `--version <v> --allow-downgrade`).
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    if job.source != Source::Chocolatey {
        bail!("{} packages are not handled by Chocolatey", job.source.as_str());
    }
    check_id(&job.id)?;
    let mut args: Vec<String> =
        ["upgrade", &job.id, "-y", "--no-progress", "--limit-output"].iter().map(|s| s.to_string()).collect();
    match &job.action {
        JobAction::Upgrade => {}
        JobAction::InstallVersion { version } => {
            check_version(version)?;
            args.extend(["--version".to_string(), version.clone(), "--allow-downgrade".to_string()]);
        }
        JobAction::ForceReinstall => args.push("--force".to_string()),
        // Let the installer show its own interface.
        JobAction::Interactive => args.push("--not-silent".to_string()),
        JobAction::StoreUpdateScan => bail!("the Store update scan is not a Chocolatey command"),
    }
    Ok(CommandSpec { program: PROGRAM.to_string(), args })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scope;

    #[test]
    fn outdated_lines() {
        let output = "Chocolatey v2.4.3\r\n\
                      python313|3.13.5|3.13.16|false\r\n\
                      contoso-tool|1.0.0|1.2.0|true\r\n\
                      \r\n\
                      vcredist140 | 14.40.33810 | 14.42.34433 | False\r\n\
                      broken|1.0|false\r\n\
                      bad id|1.0|2.0|false\r\n\
                      empty||2.0|false\r\n\
                      Chocolatey has determined 3 package(s) are outdated.\r\n";
        assert_eq!(
            parse_outdated(output),
            [
                ChocoRow {
                    id: "python313".into(),
                    installed: "3.13.5".into(),
                    available: "3.13.16".into(),
                    pinned: false
                },
                ChocoRow {
                    id: "contoso-tool".into(),
                    installed: "1.0.0".into(),
                    available: "1.2.0".into(),
                    pinned: true
                },
                ChocoRow {
                    id: "vcredist140".into(),
                    installed: "14.40.33810".into(),
                    available: "14.42.34433".into(),
                    pinned: false,
                },
            ]
        );
        assert!(parse_outdated("").is_empty());
    }

    fn job(action: JobAction) -> JobRequest {
        JobRequest {
            key: "choco:python313".into(),
            source: Source::Chocolatey,
            id: "python313".into(),
            name: "Python 3.13".into(),
            action,
            from_version: Some("3.13.5".into()),
            to_version: Some("3.13.16".into()),
            scope: Some(Scope::Machine),
            include_unknown: false,
            include_pinned: false,
            download_size: None,
            installer_type: None,
        }
    }

    #[test]
    fn commands() {
        let spec = build_command(&job(JobAction::Upgrade)).unwrap();
        assert_eq!(spec.program, "choco");
        assert_eq!(spec.args, ["upgrade", "python313", "-y", "--no-progress", "--limit-output"]);

        let spec = build_command(&job(JobAction::InstallVersion { version: "3.13.4".into() })).unwrap();
        assert_eq!(
            spec.args,
            [
                "upgrade",
                "python313",
                "-y",
                "--no-progress",
                "--limit-output",
                "--version",
                "3.13.4",
                "--allow-downgrade"
            ]
        );

        assert_eq!(build_command(&job(JobAction::ForceReinstall)).unwrap().args.last().unwrap(), "--force");
        assert!(build_command(&job(JobAction::InstallVersion { version: "-1".into() })).is_err());
        assert!(build_command(&job(JobAction::StoreUpdateScan)).is_err());

        let mut bad = job(JobAction::Upgrade);
        bad.id = "python313 --source=evil".into();
        assert!(build_command(&bad).is_err());
        bad.id = "python313".into();
        bad.source = Source::Winget;
        assert!(build_command(&bad).is_err());
    }

    #[test]
    #[ignore = "runs the real choco"]
    fn smoke_real_choco() {
        println!("choco version: {:?}", version());
        let rows = outdated().expect("choco outdated");
        println!("choco outdated: {} ({} pinned)", rows.len(), rows.iter().filter(|r| r.pinned).count());
    }
}
