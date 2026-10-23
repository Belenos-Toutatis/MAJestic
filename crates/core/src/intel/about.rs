//! What a program does, for the details panel: a summary and a longer
//! description from the catalog that knows it.
//!
//! Like the other lookups, each request carries one package id or program
//! name, never the list of installed software.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::{lookup, manifest};
use crate::model::Source;
use crate::pkgmgr::scoop;
use crate::version;

/// The program asked about (from the UI).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AboutRequest {
    pub source: Option<Source>,
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    /// Versions whose winget manifest may be read, most useful first
    /// (available, then installed).
    pub versions: Vec<String>,
    /// Language of the interface (`fr`, `en`).
    pub lang: String,
    /// The program's links, to check a catalog entry is about it.
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct About {
    pub summary: Option<String>,
    pub description: Option<String>,
    /// Language of the text (`fr-FR`, `en-US`…).
    pub locale: String,
    /// `winget`, `chocolatey`, `scoop` or `majestic`.
    pub origin: &'static str,
    /// Page the text comes from, when there is one.
    pub url: Option<String>,
}

/// Looks for a description: MAJestic's own (known programs), the local Scoop
/// manifest, then, when `online`, the winget-pkgs manifest or the Chocolatey
/// catalog. `None` when nobody describes the program.
pub fn about(req: &AboutRequest, cache_dir: &std::path::Path, online: bool) -> Option<About> {
    let french = req.lang.to_ascii_lowercase().starts_with("fr");
    if let Some(text) = lookup::known_about(&req.name, french) {
        return Some(About {
            summary: Some(text.to_string()),
            locale: if french { "fr-FR" } else { "en-US" }.to_string(),
            origin: "majestic",
            ..About::default()
        });
    }
    if req.source == Some(Source::Scoop) {
        if let Some(text) = scoop::description(&req.id) {
            return Some(About { summary: Some(text), locale: "en-US".to_string(), origin: "scoop", ..About::default() });
        }
    }
    if !online {
        return None;
    }
    match req.source {
        Some(Source::Winget) => {
            let prefer = french.then_some("fr-FR");
            req.versions.iter().filter(|v| !version::is_unknown(v)).find_map(|v| {
                manifest::fetch_about(&req.id, v, cache_dir, prefer)
                    .map_err(|e| log::debug!("about {} {v}: {e:#}", req.id))
                    .ok()
                    .filter(|a| a.summary.is_some() || a.description.is_some())
                    .map(|a| About {
                        summary: a.summary,
                        description: a.description,
                        locale: a.locale,
                        origin: "winget",
                        url: None,
                    })
            })
        }
        Some(Source::Chocolatey) => from_chocolatey(lookup::chocolatey_about(Some(&req.id), &req.name, None)),
        Some(Source::Registry) | None => {
            let scoop = lookup::scoop_manifest(&req.name, req.publisher.as_deref(), &req.urls, cache_dir)
                .and_then(|m| {
                    let page = m.page().map(str::to_string);
                    m.description.map(|d| About {
                        summary: Some(d),
                        locale: "en-US".to_string(),
                        origin: "scoop",
                        url: page,
                        ..About::default()
                    })
                });
            scoop.or_else(|| from_chocolatey(lookup::chocolatey_about(None, &req.name, req.publisher.as_deref())))
        }
        _ => None,
    }
}

fn from_chocolatey(found: Option<lookup::ChocoAbout>) -> Option<About> {
    let found = found?;
    let summary = found.summary.as_deref().and_then(|t| manifest::clean_text(t, 400));
    let description = found
        .description
        .as_deref()
        .and_then(markdown_intro)
        .filter(|d| summary.as_deref() != Some(d.as_str()));
    (summary.is_some() || description.is_some()).then(|| About {
        summary,
        description,
        locale: "en-US".to_string(),
        origin: "chocolatey",
        url: Some(found.url),
    })
}

static MD_IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").expect("valid regex"));
static MD_EMPTY_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\s*\]\([^)]*\)").expect("valid regex"));
static MD_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").expect("valid regex"));

/// The opening paragraphs of a Markdown description (Chocolatey's are long
/// pages with badges, headings and feature lists), as plain text.
fn markdown_intro(markdown: &str) -> Option<String> {
    let text = MD_IMAGE.replace_all(markdown, "");
    let text = MD_EMPTY_LINK.replace_all(&text, "");
    let text = MD_LINK.replace_all(&text, "$1");
    let text = text.replace("**", "").replace("__", "").replace("\r\n", "\n");
    let mut paragraphs: Vec<String> = Vec::new();
    for block in text.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        // The intro ends at the first heading or list.
        if block.starts_with('#') || block.starts_with("- ") || block.starts_with("* ") || block.starts_with("1.") {
            break;
        }
        paragraphs.push(block.split_whitespace().collect::<Vec<_>>().join(" "));
        if paragraphs.len() == 2 {
            break;
        }
    }
    manifest::clean_text(&paragraphs.join("\n\n"), 700)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_programs_are_described_offline() {
        let req = |lang: &str| AboutRequest {
            source: Some(Source::Registry),
            id: "Euro-Office Desktop Editors_is1".into(),
            name: "Euro-Office (Standalone) 9.3.5 (x64)".into(),
            lang: lang.into(),
            ..AboutRequest::default()
        };
        let dir = std::env::temp_dir();
        let fr = about(&req("fr"), &dir, false).unwrap();
        assert_eq!(fr.origin, "majestic");
        assert_eq!(fr.locale, "fr-FR");
        assert!(fr.summary.unwrap().starts_with("Suite bureautique libre"));
        let en = about(&req("en"), &dir, false).unwrap();
        assert!(en.summary.unwrap().starts_with("Free office suite"));
        // Offline, nothing else is asked.
        let other = AboutRequest { name: "Contoso Tool".into(), lang: "fr".into(), ..AboutRequest::default() };
        assert_eq!(about(&other, &dir, false), None);
    }

    #[test]
    fn chocolatey_markdown() {
        let md = "FileZilla Client is a fast and reliable [FTP](https://x/ftp) client.\r\n\r\n\
                  [![](https://img.shields.io/badge.svg)](https://x) ![logo](https://x/l.png)\r\n\r\n\
                  It has **tabs**.\n\n## Features\n\n- Easy to use\n- Fast";
        assert_eq!(
            markdown_intro(md).as_deref(),
            Some("FileZilla Client is a fast and reliable FTP client.\n\nIt has tabs.")
        );
        assert_eq!(markdown_intro("## Only a heading\n\n- and a list"), None);
    }

    #[test]
    fn requests_from_the_ui() {
        let req: AboutRequest = serde_json::from_str(
            r#"{"source":"winget","id":"7zip.7zip","name":"7-Zip","versions":["26.04","24.08"],"lang":"fr"}"#,
        )
        .unwrap();
        assert_eq!(req.source, Some(Source::Winget));
        assert_eq!(req.versions, ["26.04", "24.08"]);
        let empty: AboutRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.source, None);
    }

    /// Real catalogs. Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "network"]
    fn real_descriptions() {
        let cache = std::env::temp_dir().join("majestic-about-real");
        let cases = [
            (Source::Winget, "7zip.7zip", "7-Zip", "26.04"),
            (Source::Winget, "Mozilla.Firefox", "Mozilla Firefox", "149.0"),
            (Source::Registry, "FileZilla Client", "FileZilla 3.71.1", ""),
        ];
        for (source, id, name, version) in cases {
            let req = AboutRequest {
                source: Some(source),
                id: id.into(),
                name: name.into(),
                versions: vec![version.into()],
                lang: "fr".into(),
                ..AboutRequest::default()
            };
            let found = about(&req, &cache, true);
            println!("{name}: {found:#?}");
        }
    }
}
