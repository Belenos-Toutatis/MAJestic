//! winget command-line integration.
//!
//! OWNER: package-managers agent.
//!
//! winget has no machine-readable output for `list`/`upgrade`, so we parse
//! its tables (see [`table`]). Headers are localized (`Nom`, `Disponible`...)
//! and must never be matched by text: columns are identified by position and
//! content.

pub mod codes;
pub mod table;

use std::time::Duration;

use anyhow::{anyhow, bail};

use crate::exec::{self, Output};
use crate::model::{CommandSpec, JobAction, JobRequest, Source};
use crate::pkgmgr::{check_id, check_version};
use crate::version;
use table::Table;

/// Flags added to every winget call.
pub const COMMON_FLAGS: &[&str] = &["--accept-source-agreements", "--disable-interactivity"];

const PROGRAM: &str = "winget";

/// Sources every winget installation has; others come from `winget source list`.
const DEFAULT_SOURCES: &[&str] = &["winget", "msstore", "winget-font"];

/// `APPINSTALLER_CLI_ERROR_NO_APPLICATIONS_FOUND`: empty result, not a failure.
const NO_APPLICATIONS_FOUND: i64 = 0x8A15_0014;
/// `APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE`.
const UPDATE_NOT_APPLICABLE: i64 = 0x8A15_002B;

/// `list` and `upgrade` refresh the sources first, which can be slow.
const LIST_TIMEOUT: Duration = Duration::from_secs(300);
const QUERY_TIMEOUT: Duration = Duration::from_secs(120);
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WingetRow {
    /// Display name as installed (Apps & features name).
    pub name: String,
    /// Package id (`Microsoft.VisualStudioCode`) or a pseudo id for
    /// unmatched entries (`ARP\Machine\X64\{GUID}`, `MSIX\...`).
    pub id: String,
    /// Installed version as printed (`1.2.3`, `Unknown`, `< 4.0`).
    pub version: String,
    pub available: Option<String>,
    /// `winget`, `msstore`, another source name, or `None` when unmatched.
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeList {
    /// Main table of `winget upgrade --include-unknown --include-pinned`.
    pub upgrades: Vec<WingetRow>,
    /// Packages that "require explicit targeting" (second table).
    pub explicit: Vec<WingetRow>,
}

/// `winget --version` without the leading `v`, or `None` if winget is missing.
pub fn version() -> Option<String> {
    let out = exec::run(PROGRAM, &["--version"], VERSION_TIMEOUT).ok()?;
    if out.code != 0 {
        return None;
    }
    out.stdout
        .lines()
        .map(str::trim)
        .map(|line| line.strip_prefix(['v', 'V']).unwrap_or(line))
        .find(|line| version::Version::parse(line).is_some())
        .map(str::to_string)
}

/// `winget upgrade --include-unknown --include-pinned` (unknown versions only
/// when `include_unknown`).
pub fn list_upgrades(include_unknown: bool) -> anyhow::Result<UpgradeList> {
    let mut args = vec!["upgrade", "--include-pinned"];
    if include_unknown {
        args.push("--include-unknown");
    }
    let out = run(&args, LIST_TIMEOUT)?;
    let tables = table::parse_tables(&out.stdout);
    if tables.is_empty() {
        ensure_empty_result(&out, "upgrade")?;
    }
    Ok(upgrade_list(&tables))
}

/// The first table lists the upgrades; any later one, the packages that
/// require explicit targeting.
fn upgrade_list(tables: &[Table]) -> UpgradeList {
    let mut tables = tables.iter();
    UpgradeList {
        upgrades: tables.next().map(rows_from_table).unwrap_or_default(),
        explicit: tables.flat_map(rows_from_table).collect(),
    }
}

/// `winget list`: every installed program winget can see, matched or not.
pub fn list_installed() -> anyhow::Result<Vec<WingetRow>> {
    let out = run(&["list"], LIST_TIMEOUT)?;
    let tables = table::parse_tables(&out.stdout);
    if tables.is_empty() {
        ensure_empty_result(&out, "list")?;
    }
    Ok(tables.iter().flat_map(rows_from_table).collect())
}

/// Ids of pinned packages (`winget pin list`).
pub fn list_pins() -> anyhow::Result<Vec<String>> {
    let out = run(&["pin", "list"], QUERY_TIMEOUT)?;
    let tables = table::parse_tables(&out.stdout);
    if tables.is_empty() {
        // No pins: a sentence and no table.
        ensure_empty_result(&out, "pin list")?;
    }
    Ok(tables.iter().flat_map(|t| &t.rows).filter_map(|row| row.get(1)).filter(|id| !id.is_empty()).cloned().collect())
}

/// Every version of a package available in its source, newest first
/// (`winget show --id <id> --exact --versions`).
pub fn list_versions(id: &str, source: Option<&str>) -> anyhow::Result<Vec<String>> {
    check_id(id)?;
    let mut args = vec!["show", "--id", id, "--exact", "--versions"];
    if let Some(source) = source {
        check_source_name(source)?;
        args.extend(["--source", source]);
    }
    let out = run(&args, QUERY_TIMEOUT)?;
    let versions = parse_versions(&out.stdout);
    if versions.is_empty() {
        if out.code == NO_APPLICATIONS_FOUND {
            bail!("package {id} not found");
        }
        return Err(failure(&out, "show --versions"));
    }
    Ok(versions)
}

/// Name search in the `winget` source, at most 10 rows (`winget search`).
pub fn search(query: &str) -> anyhow::Result<Vec<WingetRow>> {
    // A leading dash would be read as an option.
    let query = query.trim().trim_start_matches('-').trim();
    if query.is_empty() || query.chars().any(char::is_control) {
        bail!("invalid search query");
    }
    let out = run(&["search", "--name", query, "--source", "winget", "--count", "10"], QUERY_TIMEOUT)?;
    let tables = table::parse_tables(&out.stdout);
    if tables.is_empty() {
        ensure_empty_result(&out, "search")?;
    }
    Ok(parse_search(&tables))
}

/// Configured source names (`winget source list`), e.g. `["msstore", "winget"]`.
pub fn sources() -> anyhow::Result<Vec<String>> {
    // `source list` rejects `--accept-source-agreements`.
    let out = exec::run(PROGRAM, &["source", "list", "--disable-interactivity"], QUERY_TIMEOUT)?;
    let tables = table::parse_tables(&out.stdout);
    if out.code != 0 && tables.is_empty() {
        return Err(failure(&out, "source list"));
    }
    Ok(tables
        .iter()
        .flat_map(|t| &t.rows)
        .filter_map(|row| row.first())
        .filter(|name| !name.is_empty())
        .cloned()
        .collect())
}

/// Builds the winget command for a job (upgrade, specific version, forced
/// reinstall, interactive retry). Always `--exact --id`, `--source` when known,
/// `--accept-package-agreements`, `COMMON_FLAGS`, `--silent` (except
/// interactive). Rejects ids with characters outside `[A-Za-z0-9._+-]`.
pub fn build_command(job: &JobRequest) -> anyhow::Result<CommandSpec> {
    check_id(&job.id)?;
    let source = match job.source {
        Source::Winget | Source::Registry => "winget",
        Source::MsStore => "msstore",
        other => bail!("{} packages are not handled by winget", other.as_str()),
    };

    let mut args: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(|s| s.to_string()));
    match &job.action {
        JobAction::Upgrade | JobAction::Interactive => {
            push(&["upgrade", "--id", &job.id, "--exact", "--source", source]);
            if job.include_unknown {
                push(&["--include-unknown"]);
            }
            if job.include_pinned {
                push(&["--include-pinned"]);
            }
        }
        JobAction::InstallVersion { version } => {
            check_version(version)?;
            push(&["install", "--id", &job.id, "--exact", "--source", source, "--version", version, "--force"]);
        }
        JobAction::ForceReinstall => {
            push(&["install", "--id", &job.id, "--exact", "--source", source, "--force"]);
        }
        JobAction::StoreUpdateScan => bail!("the Store update scan is not a winget command"),
    }
    push(&["--accept-package-agreements"]);
    push(COMMON_FLAGS);
    push(&[if job.action == JobAction::Interactive { "--interactive" } else { "--silent" }]);
    Ok(CommandSpec { program: PROGRAM.to_string(), args })
}

/// Runs winget with [`COMMON_FLAGS`].
fn run(args: &[&str], timeout: Duration) -> anyhow::Result<Output> {
    let mut all = args.to_vec();
    all.extend_from_slice(COMMON_FLAGS);
    exec::run(PROGRAM, &all, timeout)
}

/// No table was printed: fine when winget says there is nothing to show.
fn ensure_empty_result(out: &Output, what: &str) -> anyhow::Result<()> {
    match out.code {
        0 | NO_APPLICATIONS_FOUND | UPDATE_NOT_APPLICABLE => Ok(()),
        _ => Err(failure(out, what)),
    }
}

fn failure(out: &Output, what: &str) -> anyhow::Error {
    anyhow!("winget {what} failed (exit code {:#X}): {}", out.code, out.excerpt(3))
}

fn check_source_name(source: &str) -> anyhow::Result<()> {
    let valid = !source.is_empty()
        && !source.starts_with('-')
        && source.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !valid {
        bail!("invalid winget source name: {source:?}");
    }
    Ok(())
}

/// Rows of a `list` / `upgrade` table: Name, Id, Version, then Available and
/// Source, each present only when at least one row has a value.
fn rows_from_table(table: &Table) -> Vec<WingetRow> {
    let (available, source) = match table.headers.len() {
        0..=2 => return Vec::new(),
        3 => (None, None),
        4 if fourth_is_source(table) => (None, Some(3)),
        4 => (Some(3), None),
        _ => (Some(3), Some(4)),
    };
    let cell = |row: &[String], k: Option<usize>| k.and_then(|k| row.get(k)).filter(|value| !value.is_empty()).cloned();
    table
        .rows
        .iter()
        .filter(|row| row.get(1).is_some_and(|id| !id.is_empty()))
        .map(|row| WingetRow {
            name: row[0].clone(),
            id: row[1].clone(),
            version: row.get(2).cloned().unwrap_or_default(),
            available: cell(row, available),
            source: cell(row, source),
        })
        .collect()
}

/// Is the 4th column of a 4-column table Source (rather than Available)?
/// Decided by content: source names are words, available versions are
/// versions. Custom sources (`winget source list`) are words too, so they
/// need no extra winget call.
fn fourth_is_source(table: &Table) -> bool {
    let values: Vec<&str> =
        table.rows.iter().filter_map(|row| row.get(3)).map(String::as_str).filter(|v| !v.is_empty()).collect();
    if values.iter().any(|v| DEFAULT_SOURCES.iter().any(|s| s.eq_ignore_ascii_case(v))) {
        return true;
    }
    !values.iter().any(|v| version::Version::parse(&version::strip_approx(v).0).is_some())
}

/// The single-column `Version` table of `winget show --versions`.
fn parse_versions(output: &str) -> Vec<String> {
    let tables = table::parse_tables(output);
    let Some(table) = tables.iter().find(|t| t.headers.len() == 1) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = Vec::new();
    for value in table.rows.iter().filter_map(|row| row.first()) {
        if !value.is_empty() && !versions.contains(value) {
            versions.push(value.clone());
        }
    }
    versions
}

/// Rows of `winget search --source winget`: Name, Id, Version (then Match).
fn parse_search(tables: &[Table]) -> Vec<WingetRow> {
    tables
        .iter()
        .filter(|t| t.headers.len() >= 3)
        .flat_map(|t| &t.rows)
        .filter(|row| row.get(1).is_some_and(|id| !id.is_empty()))
        .take(10)
        .map(|row| WingetRow {
            name: row[0].clone(),
            id: row[1].clone(),
            version: row[2].clone(),
            available: None,
            source: Some("winget".to_string()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::table::tests::*;
    use super::*;
    use crate::model::Scope;

    fn row(name: &str, id: &str, version: &str, available: Option<&str>, source: Option<&str>) -> WingetRow {
        WingetRow {
            name: name.into(),
            id: id.into(),
            version: version.into(),
            available: available.map(Into::into),
            source: source.map(Into::into),
        }
    }

    #[test]
    fn upgrade_rows_keep_unknown_and_approximate_versions() {
        let list = upgrade_list(&table::parse_tables(&crlf(UPGRADE_FR)));
        assert!(list.explicit.is_empty());
        assert_eq!(list.upgrades.len(), 5);
        assert_eq!(list.upgrades[0], row("Git", "Git.Git", "2.45.1", Some("2.46.0"), Some("winget")));
        assert_eq!(list.upgrades[2].version, "Unknown");
        assert_eq!(list.upgrades[3].version, "< 3.2.0");
        assert_eq!(
            list.upgrades[4],
            row("Terminal Windows", "9N0DX20HK701", "1.20.1", Some("1.21.2"), Some("msstore"))
        );
    }

    #[test]
    fn explicit_targeting_rows() {
        let list = upgrade_list(&table::parse_tables(&crlf(UPGRADE_EN_EXPLICIT)));
        assert_eq!(list.upgrades.len(), 3);
        assert_eq!(list.upgrades[1].id, "Mozilla.Firefox");
        assert_eq!(
            list.explicit,
            [row(
                "Microsoft Edge WebView2 Runtime",
                "Microsoft.EdgeWebView2Runtime",
                "127.0.2651.74",
                Some("127.0.2651.86"),
                Some("winget")
            )]
        );
    }

    #[test]
    fn list_with_source_as_fourth_column() {
        let tables = table::parse_tables(&crlf(LIST_FR_SOURCE));
        let rows = rows_from_table(&tables[0]);
        assert_eq!(rows.len(), 6);
        assert_eq!(
            rows[0],
            row("Accueil · Exemple", "ARP\\User\\X64\\0123456789abcdef0123456789abcdef", "1.0", None, None)
        );
        assert_eq!(rows[1], row("Café Crème Studio", "Contoso.CafeCreme", "3.1.4", None, Some("winget")));
        assert_eq!(rows[3].version, "> 4.2");
        assert_eq!(rows[4].source.as_deref(), Some("msstore"));
    }

    #[test]
    fn list_with_available_as_fourth_column() {
        let tables = table::parse_tables(&crlf(LIST_EN_AVAILABLE));
        let rows = rows_from_table(&tables[0]);
        assert_eq!(rows[0], row("Git", "Git.Git", "2.45.1", Some("2.46.0"), None));
        assert_eq!(rows[1], row("Node.js LTS", "OpenJS.NodeJS.LTS", "20.15.0", None, None));
    }

    #[test]
    fn fourth_column_with_custom_source() {
        let table = Table {
            headers: vec!["Name".into(), "Id".into(), "Version".into(), "Source".into()],
            rows: vec![vec!["Tool".into(), "Contoso.Tool".into(), "1.0".into(), "contoso-repo".into()]],
        };
        assert!(fourth_is_source(&table));
        assert_eq!(rows_from_table(&table)[0].source.as_deref(), Some("contoso-repo"));
        assert_eq!(rows_from_table(&table)[0].available, None);
    }

    #[test]
    fn list_with_five_columns() {
        let tables = table::parse_tables(&crlf(LIST_FR_5));
        let rows = rows_from_table(&tables[0]);
        assert_eq!(rows[0].available, None);
        assert_eq!(rows[0].source, None);
        assert_eq!(rows[1], row("Git", "Git.Git", "2.45.1", Some("2.46.0"), Some("winget")));
        assert_eq!(rows[3], row("Paint Studio", "9PCFS5B6T72H", "5.0.13", None, Some("msstore")));
    }

    #[test]
    fn versions_table() {
        assert_eq!(parse_versions(&crlf(VERSIONS_FR)), ["2.10.0", "2.9.1", "2.9.0", "1.0.0-beta"]);
        assert!(parse_versions("Aucun package ne correspond aux critères sélectionnés.\r\n").is_empty());
    }

    #[test]
    fn search_rows() {
        let output = crlf(&[
            "Nom                     ID                      Version",
            "--------------------------------------------------------------",
            "Contoso Editor          Contoso.Editor          2.10.0",
            "Contoso Editor Insiders Contoso.Editor.Insiders 2.11.0-insider",
        ]);
        let rows = parse_search(&table::parse_tables(&output));
        assert_eq!(
            rows,
            [
                row("Contoso Editor", "Contoso.Editor", "2.10.0", None, Some("winget")),
                row("Contoso Editor Insiders", "Contoso.Editor.Insiders", "2.11.0-insider", None, Some("winget")),
            ]
        );
    }

    #[test]
    fn empty_results_are_not_errors() {
        let out =
            |code| Output { code, stdout: "Aucune épingle n’est configurée.\r\n".into(), stderr: String::new() };
        assert!(ensure_empty_result(&out(0), "pin list").is_ok());
        assert!(ensure_empty_result(&out(NO_APPLICATIONS_FOUND), "upgrade").is_ok());
        let err = ensure_empty_result(&out(0x8A15_0002), "upgrade").unwrap_err().to_string();
        assert!(err.contains("0x8A150002"), "{err}");
        assert!(err.contains("Aucune épingle"), "{err}");
    }

    fn job(source: Source, id: &str, action: JobAction) -> JobRequest {
        JobRequest {
            key: format!("winget:{id}"),
            source,
            id: id.into(),
            name: id.into(),
            action,
            from_version: Some("1.0".into()),
            to_version: Some("2.0".into()),
            scope: Some(Scope::User),
            include_unknown: false,
            include_pinned: false,
            download_size: None,
            installer_type: None,
        }
    }

    fn args(spec: &CommandSpec) -> String {
        assert_eq!(spec.program, "winget");
        spec.args.join(" ")
    }

    #[test]
    fn upgrade_command() {
        let spec = build_command(&job(Source::Winget, "Git.Git", JobAction::Upgrade)).unwrap();
        assert_eq!(
            args(&spec),
            "upgrade --id Git.Git --exact --source winget --accept-package-agreements \
             --accept-source-agreements --disable-interactivity --silent"
        );

        let mut flagged = job(Source::Winget, "Contoso.Sync", JobAction::Upgrade);
        flagged.include_unknown = true;
        flagged.include_pinned = true;
        let spec = build_command(&flagged).unwrap();
        assert_eq!(
            args(&spec),
            "upgrade --id Contoso.Sync --exact --source winget --include-unknown --include-pinned \
             --accept-package-agreements --accept-source-agreements --disable-interactivity --silent"
        );
    }

    #[test]
    fn store_and_registry_sources() {
        let spec = build_command(&job(Source::MsStore, "9N0DX20HK701", JobAction::Upgrade)).unwrap();
        assert!(args(&spec).contains("--source msstore"));
        let spec = build_command(&job(Source::Registry, "Contoso.Tool", JobAction::ForceReinstall)).unwrap();
        assert_eq!(
            args(&spec),
            "install --id Contoso.Tool --exact --source winget --force --accept-package-agreements \
             --accept-source-agreements --disable-interactivity --silent"
        );
        assert!(build_command(&job(Source::Chocolatey, "git", JobAction::Upgrade)).is_err());
        assert!(build_command(&job(Source::Msix, "Contoso.App", JobAction::Upgrade)).is_err());
    }

    #[test]
    fn install_version_and_interactive_commands() {
        let action = JobAction::InstallVersion { version: "2.9.1".into() };
        let spec = build_command(&job(Source::Winget, "Contoso.Editor", action)).unwrap();
        assert_eq!(
            args(&spec),
            "install --id Contoso.Editor --exact --source winget --version 2.9.1 --force \
             --accept-package-agreements --accept-source-agreements --disable-interactivity --silent"
        );

        let mut interactive = job(Source::Winget, "Notepad++.Notepad++", JobAction::Interactive);
        interactive.include_unknown = true;
        let spec = build_command(&interactive).unwrap();
        assert_eq!(
            args(&spec),
            "upgrade --id Notepad++.Notepad++ --exact --source winget --include-unknown \
             --accept-package-agreements --accept-source-agreements --disable-interactivity --interactive"
        );
        assert!(!spec.args.contains(&"--silent".to_string()));

        let bad = JobAction::InstallVersion { version: "--override".into() };
        assert!(build_command(&job(Source::Winget, "Contoso.Editor", bad)).is_err());
        let bad = JobAction::InstallVersion { version: "1.0|calc".into() };
        assert!(build_command(&job(Source::Winget, "Contoso.Editor", bad)).is_err());
        assert!(build_command(&job(Source::Winget, "Git.Git", JobAction::StoreUpdateScan)).is_err());
    }

    #[test]
    fn ids_are_validated() {
        for bad in ["", "Git Git", "--force", "Git.Git;calc", "ARP\\Machine\\X64\\{GUID}", "MSIX\\Foo", "a\"b", "é.App"]
        {
            assert!(build_command(&job(Source::Winget, bad, JobAction::Upgrade)).is_err(), "{bad:?} accepted");
        }
        for good in ["Git.Git", "Notepad++.Notepad++", "9N0DX20HK701", "Microsoft.VCRedist.2015+.x64", "Foo_Bar-1"] {
            assert!(build_command(&job(Source::Winget, good, JobAction::Upgrade)).is_ok(), "{good:?} rejected");
        }
        assert!(list_versions("Git Git", None).is_err());
        assert!(list_versions("Git.Git", Some("--all")).is_err());
    }

    #[test]
    #[ignore = "runs the real winget"]
    fn smoke_real_winget() {
        println!("winget version: {:?}", version());
        let upgrades = list_upgrades(true).expect("winget upgrade");
        println!("upgrades: {}, explicit: {}", upgrades.upgrades.len(), upgrades.explicit.len());
        let installed = list_installed().expect("winget list");
        let matched = installed.iter().filter(|r| r.source.is_some()).count();
        println!("installed: {} ({} matched to a source)", installed.len(), matched);
        let versions = list_versions("Microsoft.VisualStudioCode", None).expect("winget show --versions");
        println!("Microsoft.VisualStudioCode versions: {} (newest {:?})", versions.len(), versions.first());
        println!("pins: {}", list_pins().expect("winget pin list").len());
        println!("sources: {:?}", sources().expect("winget source list"));
        println!("search 'notepad++': {}", search("notepad++").expect("winget search").len());
        assert!(!installed.is_empty());
        assert!(!versions.is_empty());
    }
}
