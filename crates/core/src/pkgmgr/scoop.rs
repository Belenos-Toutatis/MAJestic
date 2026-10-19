//! Scoop, when installed (user-level, must never run elevated).
//!
//! OWNER: package-managers agent.

use std::env;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{anyhow, bail};
use regex::Regex;

use crate::exec;
use crate::model::{CommandSpec, JobAction, JobRequest, Source};
use crate::pkgmgr::{check_id, check_version};
use crate::winget::table;

/// `scoop update` fetches every bucket with git.
const UPDATE_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoopRow {
    pub app: String,
    pub installed: String,
    pub available: String,
}

/// Is Scoop installed for this user?
pub fn detect() -> bool {
    launcher().is_some()
}

/// Refreshes buckets then lists outdated apps (`scoop status`).
pub fn status() -> anyhow::Result<Vec<ScoopRow>> {
    let launcher = launcher().ok_or_else(|| anyhow!("Scoop is not installed"))?;

    let update = launcher.command(&["update"]);
    match run(&update, UPDATE_TIMEOUT) {
        Ok(out) if out.code != 0 => log::warn!("scoop update exited with code {}", out.code),
        Ok(_) => {}
        Err(err) => log::warn!("scoop update failed: {err:#}"),
    }

    let out = run(&launcher.command(&["status"]), STATUS_TIMEOUT)?;
    let rows = parse_status(&out.stdout);
    if out.code != 0 && rows.is_empty() {
        bail!("scoop status failed (exit code {}): {}", out.code, out.excerpt(3));
    }
    Ok(rows)
}

/// `scoop update <app>` (or `scoop install <app>@<version>` for a version).
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    if job.source != Source::Scoop {
        bail!("{} packages are not handled by Scoop", job.source.as_str());
    }
    check_id(&job.id)?;
    let args = match &job.action {
        JobAction::Upgrade | JobAction::Interactive => vec!["update".to_string(), job.id.clone()],
        JobAction::ForceReinstall => vec!["update".to_string(), job.id.clone(), "--force".to_string()],
        JobAction::InstallVersion { version } => {
            check_version(version)?;
            if version.contains(char::is_whitespace) {
                bail!("invalid package version: {version:?}");
            }
            vec!["install".to_string(), format!("{}@{version}", job.id)]
        }
        JobAction::StoreUpdateScan => bail!("the Store update scan is not a Scoop command"),
    };
    let launcher = launcher().ok_or_else(|| anyhow!("Scoop is not installed"))?;
    Ok(launcher.command(&args.iter().map(String::as_str).collect::<Vec<_>>()))
}

/// How to start Scoop.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Launcher {
    /// `scoop.ps1`, run by Windows PowerShell.
    Script(PathBuf),
    /// `scoop.cmd` shim, run directly.
    Shim(PathBuf),
}

impl Launcher {
    fn command(&self, args: &[&str]) -> CommandSpec {
        match self {
            Launcher::Script(script) => {
                let mut all: Vec<String> =
                    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"].iter().map(|s| s.to_string()).collect();
                all.push(script.to_string_lossy().into_owned());
                all.extend(args.iter().map(|s| s.to_string()));
                CommandSpec { program: "powershell".to_string(), args: all }
            }
            Launcher::Shim(shim) => CommandSpec {
                program: shim.to_string_lossy().into_owned(),
                args: args.iter().map(|s| s.to_string()).collect(),
            },
        }
    }
}

/// Finds Scoop in `%SCOOP%`, else `%USERPROFILE%\scoop`.
fn launcher() -> Option<Launcher> {
    let root = match env::var_os("SCOOP").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env::var_os("USERPROFILE")?).join("scoop"),
    };
    find_launcher(&root)
}

fn find_launcher(root: &std::path::Path) -> Option<Launcher> {
    let shims = root.join("shims");
    [shims.join("scoop.ps1"), root.join("apps").join("scoop").join("current").join("bin").join("scoop.ps1")]
        .into_iter()
        .find(|path| path.is_file())
        .map(Launcher::Script)
        .or_else(|| Some(shims.join("scoop.cmd")).filter(|path| path.is_file()).map(Launcher::Shim))
}

fn run(spec: &CommandSpec, timeout: Duration) -> anyhow::Result<exec::Output> {
    let args: Vec<&str> = spec.args.iter().map(String::as_str).collect();
    exec::run(&spec.program, &args, timeout)
}

/// `git: 2.40.0 -> 2.41.0` (Scoop before 0.3).
static LEGACY_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([\w.+-]+):\s+(\S+)\s+->\s+(\S+)(.*)$").expect("valid regex"));

/// Outdated apps from `scoop status`: the `Format-Table` output of current
/// versions (Name, Installed Version, Latest Version, Missing Dependencies,
/// Info), or the `app: installed -> latest` lines of older ones. Apps without
/// a newer version (missing dependencies, removed manifest) and held apps are
/// left out.
fn parse_status(output: &str) -> Vec<ScoopRow> {
    let tables = table::parse_tables(output);
    if let Some(table) = tables.iter().find(|t| t.headers.len() >= 3) {
        return table
            .rows
            .iter()
            .filter(|row| !row.get(4).is_some_and(|info| info.to_lowercase().contains("held")))
            .filter_map(|row| match row.as_slice() {
                [app, installed, available, ..] if !app.is_empty() && !available.is_empty() => {
                    Some(ScoopRow { app: app.clone(), installed: installed.clone(), available: available.clone() })
                }
                _ => None,
            })
            .collect();
    }
    output
        .lines()
        .map(exec::visible_text)
        .filter_map(|line| {
            let caps = LEGACY_LINE.captures(line.trim_end())?;
            let held = caps[4].to_lowercase().contains("held");
            (!held).then(|| ScoopRow {
                app: caps[1].to_string(),
                installed: caps[2].to_string(),
                available: caps[3].to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scope;

    #[test]
    fn status_table() {
        let output = "WARN  Scoop out of date. Run 'scoop update' to get the latest changes.\r\n\
                      \r\n\
                      Name    Installed Version Latest Version   Missing Dependencies Info\r\n\
                      ----    ----------------- --------------   -------------------- ----\r\n\
                      7zip    23.01             24.08\r\n\
                      git     2.45.1.windows.1  2.46.0.windows.1\r\n\
                      python  3.12.4            3.12.5                                Held package\r\n\
                      oldtool 1.0                                                     Manifest removed\r\n\
                      nodejs  20.15.0           20.16.0          dark\r\n\
                      \r\n";
        assert_eq!(
            parse_status(output),
            [
                ScoopRow { app: "7zip".into(), installed: "23.01".into(), available: "24.08".into() },
                ScoopRow {
                    app: "git".into(),
                    installed: "2.45.1.windows.1".into(),
                    available: "2.46.0.windows.1".into()
                },
                ScoopRow { app: "nodejs".into(), installed: "20.15.0".into(), available: "20.16.0".into() },
            ]
        );
    }

    #[test]
    fn status_up_to_date() {
        assert!(parse_status("Scoop is up to date.\r\nEverything is ok!\r\n").is_empty());
    }

    #[test]
    fn legacy_status_lines() {
        let output = "Scoop is up to date.\nUpdates are available for:\n    7zip: 19.00 -> 23.01\n    git: 2.40.0 -> 2.41.0 (held)\n    \
                      nodejs-lts: 18.16.0 -> 18.17.0\nMissing runtime dependencies:\n    'foo' requires 'bar'\n";
        assert_eq!(
            parse_status(output),
            [
                ScoopRow { app: "7zip".into(), installed: "19.00".into(), available: "23.01".into() },
                ScoopRow { app: "nodejs-lts".into(), installed: "18.16.0".into(), available: "18.17.0".into() },
            ]
        );
    }

    #[test]
    fn launcher_detection() {
        let root = std::env::temp_dir().join(format!("majestic-scoop-test-{}", std::process::id()));
        let shims = root.join("shims");
        std::fs::create_dir_all(&shims).unwrap();
        assert_eq!(find_launcher(&root), None);

        std::fs::write(shims.join("scoop.cmd"), "@echo off\r\n").unwrap();
        assert_eq!(find_launcher(&root), Some(Launcher::Shim(shims.join("scoop.cmd"))));

        std::fs::write(shims.join("scoop.ps1"), "").unwrap();
        let launcher = find_launcher(&root).unwrap();
        assert_eq!(launcher, Launcher::Script(shims.join("scoop.ps1")));
        let spec = launcher.command(&["update", "git"]);
        assert_eq!(spec.program, "powershell");
        assert_eq!(
            spec.args,
            [
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-File".to_string(),
                shims.join("scoop.ps1").to_string_lossy().into_owned(),
                "update".to_string(),
                "git".to_string(),
            ]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn job(action: JobAction) -> JobRequest {
        JobRequest {
            key: "scoop:git".into(),
            source: Source::Scoop,
            id: "git".into(),
            name: "git".into(),
            action,
            from_version: Some("2.45.1".into()),
            to_version: Some("2.46.0".into()),
            scope: Some(Scope::User),
            include_unknown: false,
            include_pinned: false,
            download_size: None,
            installer_type: None,
        }
    }

    #[test]
    fn commands_validate_their_input() {
        let mut bad = job(JobAction::Upgrade);
        bad.id = "git; rm".into();
        assert!(build_command(&bad).is_err());
        assert!(build_command(&job(JobAction::InstallVersion { version: "2.0 beta".into() })).is_err());
        assert!(build_command(&job(JobAction::StoreUpdateScan)).is_err());
        let mut other = job(JobAction::Upgrade);
        other.source = Source::Chocolatey;
        assert!(build_command(&other).is_err());
        // The rest depends on Scoop being installed.
        match launcher() {
            None => assert!(build_command(&job(JobAction::Upgrade)).is_err()),
            Some(_) => {
                let spec = build_command(&job(JobAction::InstallVersion { version: "2.45.1".into() })).unwrap();
                assert_eq!(spec.args[spec.args.len() - 2..], ["install".to_string(), "git@2.45.1".to_string()]);
            }
        }
    }

    #[test]
    #[ignore = "runs the real scoop"]
    fn smoke_real_scoop() {
        println!("scoop installed: {}", detect());
        if detect() {
            println!("scoop status: {}", status().expect("scoop status").len());
        }
    }
}
