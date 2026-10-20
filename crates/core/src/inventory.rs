//! The list of installed programs, saved to a file to reinstall them on
//! another PC: packages a manager can install again, and the other programs
//! as a reminder (name, version, link).

use serde::{Deserialize, Serialize};

use crate::model::{Category, ItemKind, Scope, Source, UpdateItem};
use crate::pkgmgr::check_id;

pub const FORMAT: &str = "majestic-inventory";
pub const VERSION: u32 = 1;
/// Larger files are refused.
pub const MAX_FILE: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub format: String,
    pub version: u32,
    /// RFC 3339.
    pub created_at: String,
    #[serde(default)]
    pub packages: Vec<Package>,
    #[serde(default)]
    pub manual: Vec<Manual>,
}

/// A program a package manager can install again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub source: Source,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub scope: Option<Scope>,
}

/// A program no package manager knows: to reinstall by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manual {
    pub name: String,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// Download page or homepage.
    #[serde(default)]
    pub url: Option<String>,
}

impl Package {
    /// The key a scan gives this package (`winget:<id>`…).
    pub fn key(&self) -> String {
        let prefix = match self.source {
            Source::MsStore => "msstore",
            Source::Chocolatey => "choco",
            Source::Scoop => "scoop",
            _ => "winget",
        };
        format!("{prefix}:{}", self.id)
    }
}

/// Where the dialogs open: the Documents folder.
pub fn default_folder() -> Option<std::path::PathBuf> {
    dirs::document_dir()
}

fn installable(source: Source) -> bool {
    matches!(source, Source::Winget | Source::MsStore | Source::Chocolatey | Source::Scoop)
}

/// The inventory of a scan: every package of winget, the Store, Chocolatey
/// and Scoop, and the other applications (not drivers, games, system
/// components…). Sorted by name.
pub fn build(items: &[UpdateItem]) -> Inventory {
    let mut packages: Vec<Package> = items
        .iter()
        .filter(|i| installable(i.source) && !i.id.is_empty())
        .map(|i| Package {
            source: i.source,
            id: i.id.clone(),
            name: i.name.clone(),
            version: i.installed_version.clone(),
            scope: i.scope,
        })
        .collect();
    let mut manual: Vec<Manual> = items
        .iter()
        .filter(|i| i.source == Source::Registry && i.kind != ItemKind::Update && i.category == Category::Application)
        .map(|i| Manual {
            name: i.name.clone(),
            publisher: i.publisher.clone(),
            version: i.installed_version.clone(),
            url: i.links.download.clone().or_else(|| i.links.homepage.clone()).filter(|u| u.starts_with("http")),
        })
        .collect();
    packages.sort_by_key(|p| p.name.to_lowercase());
    packages.dedup_by(|a, b| a.key().eq_ignore_ascii_case(&b.key()));
    manual.sort_by_key(|m| m.name.to_lowercase());
    manual.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    Inventory {
        format: FORMAT.to_string(),
        version: VERSION,
        created_at: chrono::Local::now().to_rfc3339(),
        packages,
        manual,
    }
}

/// Adds the packages Chocolatey and Scoop have installed (the scan only
/// lists the outdated ones), unless already listed.
pub fn add_installed(inv: &mut Inventory, chocolatey: Vec<(String, String)>, scoop: Vec<(String, Option<String>)>) {
    let extra = chocolatey
        .into_iter()
        .map(|(id, v)| Package { source: Source::Chocolatey, name: id.clone(), id, version: Some(v), scope: None })
        .chain(scoop.into_iter().map(|(id, version)| Package {
            source: Source::Scoop,
            name: id.clone(),
            id,
            version,
            scope: Some(Scope::User),
        }));
    for p in extra {
        if !inv.packages.iter().any(|q| q.key().eq_ignore_ascii_case(&p.key())) {
            inv.packages.push(p);
        }
    }
    inv.packages.sort_by_key(|p| p.name.to_lowercase());
}

/// Reads an inventory file. Packages with an invalid id or a source that
/// cannot install are dropped.
pub fn parse(text: &str) -> anyhow::Result<Inventory> {
    let mut inv: Inventory = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    if inv.format != FORMAT {
        anyhow::bail!("not a MAJestic list (format {:?})", inv.format);
    }
    if inv.version > VERSION {
        anyhow::bail!("list made by a newer MAJestic (version {})", inv.version);
    }
    inv.packages.retain(|p| installable(p.source) && check_id(&p.id).is_ok());
    inv.manual.retain(|m| !m.name.trim().is_empty());
    for m in &mut inv.manual {
        m.url = m.url.take().filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    }
    Ok(inv)
}

/// What importing a list would do on this PC.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub created_at: String,
    /// Not installed here: can be installed.
    pub to_install: Vec<Package>,
    /// Already installed here.
    pub installed: Vec<Package>,
    /// Programs to reinstall by hand that are not found here.
    pub manual: Vec<Manual>,
    /// Programs to reinstall by hand already found here (by name).
    pub manual_installed: usize,
}

/// Compares the list with the programs found by the last scan of this PC.
pub fn plan(inv: &Inventory, items: &[UpdateItem]) -> ImportPlan {
    let keys: std::collections::HashSet<String> = items.iter().map(|i| i.key.to_lowercase()).collect();
    let names: std::collections::HashSet<String> = items.iter().map(|i| simple_name(&i.name)).collect();
    let (installed, to_install): (Vec<Package>, Vec<Package>) =
        inv.packages.iter().cloned().partition(|p| keys.contains(&p.key().to_lowercase()));
    let (found, manual): (Vec<Manual>, Vec<Manual>) =
        inv.manual.iter().cloned().partition(|m| names.contains(&simple_name(&m.name)));
    ImportPlan { created_at: inv.created_at.clone(), to_install, installed, manual, manual_installed: found.len() }
}

/// Name without version, architecture or punctuation: `FileZilla 3.71.1`
/// and `FileZilla 3.72.0 (x64)` match.
fn simple_name(name: &str) -> String {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .filter(|w| !matches!(w.to_lowercase().as_str(), "x64" | "x86" | "arm64" | "bit"))
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(key: &str, source: Source, id: &str, name: &str) -> UpdateItem {
        let mut item = UpdateItem::new(key.into(), source, id.into(), name.into());
        item.installed_version = Some("1.0".into());
        item.kind = ItemKind::UpToDate;
        item
    }

    #[test]
    fn build_and_read_back() {
        let mut firefox = item("winget:Mozilla.Firefox", Source::Winget, "Mozilla.Firefox", "Mozilla Firefox");
        firefox.scope = Some(Scope::Machine);
        let mut filezilla = item("arp:HKLM32\\FileZilla Client", Source::Registry, "FileZilla Client", "FileZilla 3.71.1");
        filezilla.kind = ItemKind::Unmanaged;
        filezilla.links.homepage = Some("https://filezilla-project.org/".into());
        let mut driver = item("arp:HKLM64\\Pilote", Source::Registry, "Pilote", "Pilote audio");
        driver.category = Category::Driver;
        let items = vec![
            firefox,
            item("scoop:yt-dlp", Source::Scoop, "yt-dlp", "yt-dlp"),
            item("msix:Microsoft.Paint", Source::Msix, "Microsoft.Paint", "Paint"),
            filezilla,
            driver,
        ];
        let inv = build(&items);
        assert_eq!(inv.packages.iter().map(Package::key).collect::<Vec<_>>(), ["winget:Mozilla.Firefox", "scoop:yt-dlp"]);
        assert_eq!(inv.packages[0].scope, Some(Scope::Machine));
        assert_eq!(inv.manual.len(), 1);
        assert_eq!(inv.manual[0].url.as_deref(), Some("https://filezilla-project.org/"));

        let mut inv = inv;
        add_installed(
            &mut inv,
            vec![("python313".into(), "3.13.16".into())],
            vec![("yt-dlp".into(), Some("2026.10.1".into())), ("ffmpeg".into(), None)],
        );
        assert_eq!(
            inv.packages.iter().map(Package::key).collect::<Vec<_>>(),
            ["scoop:ffmpeg", "winget:Mozilla.Firefox", "choco:python313", "scoop:yt-dlp"]
        );
        let text = serde_json::to_string_pretty(&inv).unwrap();
        assert!(text.contains("\"format\": \"majestic-inventory\""));
        assert_eq!(parse(&text).unwrap(), inv);
    }

    #[test]
    fn reading_is_strict_about_the_format_and_ids() {
        assert!(parse("{}").is_err());
        assert!(parse(r#"{"format":"other","version":1,"createdAt":""}"#).is_err());
        assert!(parse(r#"{"format":"majestic-inventory","version":99,"createdAt":""}"#).is_err());
        let inv = parse(
            r#"{"format":"majestic-inventory","version":1,"createdAt":"2026-10-20T18:00:00+02:00",
                "packages":[{"source":"winget","id":"Git.Git","name":"Git"},
                            {"source":"winget","id":"bad id; rm","name":"Evil"},
                            {"source":"registry","id":"X","name":"X"}],
                "manual":[{"name":"FileZilla 3.71.1","url":"javascript:alert(1)"},{"name":" "}]}"#,
        )
        .unwrap();
        assert_eq!(inv.packages.len(), 1);
        assert_eq!(inv.manual.len(), 1);
        assert_eq!(inv.manual[0].url, None);
    }

    #[test]
    fn plan_against_this_pc() {
        let inv = Inventory {
            format: FORMAT.into(),
            version: VERSION,
            created_at: "2026-10-20".into(),
            packages: vec![
                Package { source: Source::Winget, id: "Git.Git".into(), name: "Git".into(), version: None, scope: None },
                Package { source: Source::Winget, id: "Inkscape.Inkscape".into(), name: "Inkscape".into(), version: None, scope: None },
            ],
            manual: vec![
                Manual { name: "FileZilla 3.71.1".into(), publisher: None, version: None, url: None },
                Manual { name: "Jasc Paint Shop Pro 9".into(), publisher: None, version: None, url: None },
            ],
        };
        let here = vec![
            item("winget:git.git", Source::Winget, "git.git", "Git"),
            item("arp:HKLM32\\FileZilla Client", Source::Registry, "FileZilla Client", "FileZilla 3.72.0 (x64)"),
        ];
        let plan = plan(&inv, &here);
        assert_eq!(plan.installed.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["Git.Git"]);
        assert_eq!(plan.to_install.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["Inkscape.Inkscape"]);
        assert_eq!(plan.manual.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Jasc Paint Shop Pro 9"]);
        assert_eq!(plan.manual_installed, 1);
    }
}
