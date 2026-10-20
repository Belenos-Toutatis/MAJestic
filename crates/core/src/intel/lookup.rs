//! Alternatives for programs that no package manager handles.
//!
//! OWNER: intel agent.
//!
//! Each request carries one program name (Chocolatey search) or one
//! repository (GitHub), never the list of installed software.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use serde::Deserialize;

use super::http;
use crate::model::{Alternative, Links};
use crate::version::{self, Version};

const CHOCOLATEY_SEARCH: &str = "https://community.chocolatey.org/api/v2/Search()";
const CHOCOLATEY_PACKAGE_PAGE: &str = "https://community.chocolatey.org/packages/";
/// Per-request budget: lookups run in the background, one program at a time.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Stops querying a catalog that keeps failing (outage, rate limit), so that
/// a scan of many programs does not wait for one timeout per program.
struct Breaker {
    failures: AtomicU32,
    open_until: Mutex<Option<Instant>>,
}

impl Breaker {
    const THRESHOLD: u32 = 3;
    const COOLDOWN: Duration = Duration::from_secs(10 * 60);

    const fn new() -> Breaker {
        Breaker { failures: AtomicU32::new(0), open_until: Mutex::new(None) }
    }

    fn is_open(&self) -> bool {
        let guard = self.open_until.lock().unwrap_or_else(|e| e.into_inner());
        guard.is_some_and(|until| Instant::now() < until)
    }

    fn success(&self) {
        self.failures.store(0, Ordering::Relaxed);
    }

    fn failure(&self) {
        if self.failures.fetch_add(1, Ordering::Relaxed) + 1 >= Self::THRESHOLD {
            self.open_for(Self::COOLDOWN);
        }
    }

    fn open_for(&self, duration: Duration) {
        self.failures.store(0, Ordering::Relaxed);
        *self.open_until.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + duration);
    }
}

static GITHUB: Breaker = Breaker::new();
static CHOCOLATEY: Breaker = Breaker::new();

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookupInput {
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// Registry URLs (URLInfoAbout, URLUpdateInfo, HelpLink).
    pub urls: Vec<String>,
}

/// Looks for a more recent version in public catalogs, most reliable first:
/// GitHub releases when one of the URLs is a github.com repository, then the
/// Chocolatey community catalog (search by name, accepting only confident
/// title matches). Returns `Alternative::LatestElsewhere` only when the found
/// version is strictly newer than `input.version`.
///
/// When the GitHub repository has a usable latest release, its answer is
/// final (an up-to-date program is not looked up on Chocolatey).
pub fn latest_elsewhere(input: &LookupInput) -> Option<Alternative> {
    let installed = input.version.as_deref()?.trim();
    Version::parse(installed)?;

    if let Some((owner, repo)) = github_repo(&input.urls).filter(|_| !GITHUB.is_open()) {
        match github_latest(&owner, &repo) {
            Ok(Some((latest, url))) if same_scheme(&latest, installed) => {
                GITHUB.success();
                return version::is_newer(&latest, installed).then(|| Alternative::LatestElsewhere {
                    version: latest,
                    origin: "github".to_string(),
                    url: Some(url),
                });
            }
            Ok(_) => {
                GITHUB.success();
                log::debug!("no usable GitHub release for {owner}/{repo}");
            }
            Err(err) => {
                match http::error_status(&err) {
                    // No release at all: a valid answer.
                    Some(404) => GITHUB.success(),
                    // Rate limit (60 requests/hour without a token).
                    Some(403 | 429) => GITHUB.open_for(Duration::from_secs(30 * 60)),
                    _ => GITHUB.failure(),
                }
                log::debug!("GitHub lookup of {owner}/{repo} failed: {err:#}");
            }
        }
    }

    let wanted = normalize_title(&input.name, input.publisher.as_deref());
    if wanted.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return None;
    }
    if CHOCOLATEY.is_open() {
        return None;
    }
    let packages = match chocolatey_search(&search_term(&input.name)) {
        Ok(packages) => {
            CHOCOLATEY.success();
            packages
        }
        Err(err) => {
            CHOCOLATEY.failure();
            log::debug!("Chocolatey lookup failed: {err:#}");
            return None;
        }
    };
    let (latest, id) = pick_chocolatey(&packages, &wanted, input.publisher.as_deref())?;
    (same_scheme(&latest, installed) && version::is_newer(&latest, installed)).then(|| Alternative::LatestElsewhere {
        version: latest,
        origin: "chocolatey".to_string(),
        url: Some(format!("{CHOCOLATEY_PACKAGE_PAGE}{}", encode(&id))),
    })
}

/// A program whose registry entry does not lead to where it is published.
struct KnownRepo {
    /// Start of the display name, lowercase.
    name_prefix: &'static str,
    owner: &'static str,
    repo: &'static str,
}

/// Euro-Office, a fork of ONLYOFFICE, kept ONLYOFFICE's publisher and links
/// in the registry (onlyoffice.com): its releases are on GitHub.
const KNOWN_REPOS: &[KnownRepo] = &[KnownRepo { name_prefix: "euro-office", owner: "Euro-Office", repo: "DesktopEditors" }];

/// Links of a program listed in [`KNOWN_REPOS`], to use instead of the
/// registry ones: its GitHub repository, latest release and issues. They
/// also make [`latest_elsewhere`] look up its GitHub releases.
pub fn known_links(display_name: &str) -> Option<Links> {
    let name = display_name.trim().to_lowercase();
    let known = KNOWN_REPOS.iter().find(|k| name.starts_with(k.name_prefix))?;
    let repo = format!("https://github.com/{}/{}", known.owner, known.repo);
    Some(Links {
        homepage: Some(repo.clone()),
        download: Some(format!("{repo}/releases/latest")),
        release_notes: None,
        support: Some(format!("{repo}/issues")),
    })
}

/// `owner/repo` of the first github.com repository URL.
pub fn github_repo(urls: &[String]) -> Option<(String, String)> {
    urls.iter().find_map(|url| parse_github_url(url))
}

/// Web search URL to find the official download page.
pub fn web_search_url(name: &str) -> String {
    let without_parens = strip_parenthesized(name);
    let without_versions = DOTTED_VERSION.replace_all(&without_parens, " ");
    let words: Vec<&str> = without_versions
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric) && !ARCH_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();
    let subject = if words.is_empty() { name.trim().to_string() } else { words.join(" ") };
    format!("https://duckduckgo.com/?q={}", encode(&format!("{subject} download official")))
}

// ---------------------------------------------------------------------------
// GitHub
// ---------------------------------------------------------------------------

/// First path segments of github.com that are not user or organization names.
const GITHUB_RESERVED: &[&str] = &[
    "about", "account", "apps", "blog", "codespaces", "collections", "contact", "customer-stories", "enterprise",
    "events", "explore", "features", "issues", "join", "login", "marketplace", "new", "notifications", "orgs",
    "organizations", "pricing", "pulls", "readme", "search", "security", "settings", "signup", "site", "sponsors",
    "topics", "trending", "users",
];

fn parse_github_url(url: &str) -> Option<(String, String)> {
    let mut rest = url.trim();
    rest = rest.strip_prefix("git+").unwrap_or(rest);
    for scheme in ["https://", "http://", "git://"] {
        if rest.len() >= scheme.len() && rest[..scheme.len()].eq_ignore_ascii_case(scheme) {
            rest = &rest[scheme.len()..];
            break;
        }
    }
    if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("www.") {
        rest = &rest[4..];
    }
    let (host, path) = rest.split_once('/')?;
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    let owner = segments.next()?;
    let repo = segments.next()?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);

    let owner_ok = owner.len() <= 39
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !GITHUB_RESERVED.iter().any(|r| r.eq_ignore_ascii_case(owner));
    let repo_ok = !repo.is_empty()
        && repo != "."
        && repo != ".."
        && repo.len() <= 100
        && repo.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    (owner_ok && repo_ok).then(|| (owner.to_string(), repo.to_string()))
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    #[serde(default)]
    tag_name: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Latest release of a repository: `(version, release page)`. `Ok(None)` when
/// the release has no version-like tag.
fn github_latest(owner: &str, repo: &str) -> anyhow::Result<Option<(String, String)>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let release: GithubRelease = serde_json::from_str(&http::get_text_within(&url, LOOKUP_TIMEOUT)?)?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = release
        .tag_name
        .as_deref()
        .and_then(version_from_tag)
        .or_else(|| release.name.as_deref().and_then(version_from_tag))
    else {
        return Ok(None);
    };
    let url = release
        .html_url
        .filter(|u| u.starts_with("https://github.com/"))
        .unwrap_or_else(|| format!("https://github.com/{owner}/{repo}/releases/latest"));
    Ok(Some((version, url)))
}

/// `v1.2.3` → `1.2.3`, `Audacity-3.7.1` → `3.7.1`, `curl-8_10_1` → `8.10.1`,
/// `release/2.0 (final)` → `2.0`. `None` without a version.
fn version_from_tag(tag: &str) -> Option<String> {
    let tag = tag.trim();
    let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
    let start = tag.find(|c: char| c.is_ascii_digit())?;
    // The prefix is a product name and/or `v`, ended by a separator: letters
    // glued to the number (`x64`, `build7`) are not a version.
    let prefix = tag[..start].to_ascii_lowercase();
    let separated = |p: &str| p.is_empty() || p.ends_with(['-', '_', '/', ' ', '.', '@']);
    let prefix_ok = separated(&prefix) || prefix.strip_suffix('v').is_some_and(separated);
    if !prefix_ok {
        return None;
    }
    let mut candidate: String = tag[start..].split_whitespace().next()?.to_string();
    if !candidate.contains('.') && candidate.contains('_') {
        candidate = candidate.replace('_', ".");
    }
    let candidate = candidate.trim_end_matches(['.', '-', '_', ')', ']', ',']).to_string();
    Version::parse(&candidate).map(|_| candidate)
}

/// Both versions use the same numbering scheme: calendar (`2024.10`) or
/// date (`20241001`) versions are not comparable with semantic ones.
fn same_scheme(a: &str, b: &str) -> bool {
    let (Some(a), Some(b)) = (Version::parse(a), Version::parse(b)) else {
        return false;
    };
    let year = |n: u64| (1990..=2100).contains(&n);
    let date = |n: u64| (19_900_101..=21_001_231).contains(&n);
    year(a.major()) == year(b.major()) && date(a.major()) == date(b.major())
}

// ---------------------------------------------------------------------------
// Chocolatey
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ChocoPackage {
    id: String,
    title: String,
    version: String,
    project_url: Option<String>,
}

fn chocolatey_search(term: &str) -> anyhow::Result<Vec<ChocoPackage>> {
    // OData string literal: quotes are doubled.
    let literal = term.replace('\'', "''");
    let url = format!(
        "{CHOCOLATEY_SEARCH}?$filter=IsLatestVersion&searchTerm='{}'&targetFramework=''&includePrerelease=false&$top=5",
        encode(&literal)
    );
    Ok(parse_chocolatey_feed(&http::get_text_within(&url, LOOKUP_TIMEOUT)?))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChocoField {
    EntryTitle,
    Id,
    Title,
    Version,
    ProjectUrl,
}

/// Parses an OData v2 Atom feed (`<entry>`, `<m:properties>` with
/// `d:Id`/`d:Title`/`d:Version`/`d:ProjectUrl`; the Atom `<title>` of an
/// entry is the package id when `d:Id` is absent).
fn parse_chocolatey_feed(xml: &str) -> Vec<ChocoPackage> {
    #[derive(Default)]
    struct Partial {
        entry_title: String,
        id: String,
        title: String,
        version: String,
        project_url: String,
    }

    let mut reader = Reader::from_str(xml);
    let mut packages = Vec::new();
    let mut entry: Option<Partial> = None;
    let mut in_properties = false;
    let mut field: Option<ChocoField> = None;
    let mut text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = name.local_name();
                let prefixed = name.prefix().is_some();
                match (local.as_ref(), prefixed) {
                    (b"entry", false) => entry = Some(Partial::default()),
                    (b"properties", _) => in_properties = true,
                    (local, _) if entry.is_some() => {
                        field = match (local, in_properties) {
                            (b"title", false) if !prefixed => Some(ChocoField::EntryTitle),
                            (b"Id", true) => Some(ChocoField::Id),
                            (b"Title", true) => Some(ChocoField::Title),
                            (b"Version", true) => Some(ChocoField::Version),
                            (b"ProjectUrl", true) => Some(ChocoField::ProjectUrl),
                            _ => None,
                        };
                        text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if field.is_some() => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Ok(Event::CData(t)) if field.is_some() => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Ok(Event::GeneralRef(r)) if field.is_some() => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Ok(name) = r.decode() {
                    text.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                match (name.local_name().as_ref(), name.prefix().is_some()) {
                    (b"entry", false) => {
                        if let Some(p) = entry.take() {
                            let id = if p.id.is_empty() { p.entry_title } else { p.id };
                            if !id.is_empty() && !p.version.is_empty() {
                                packages.push(ChocoPackage {
                                    title: if p.title.is_empty() { id.clone() } else { p.title },
                                    id,
                                    version: p.version,
                                    project_url: (!p.project_url.is_empty()).then_some(p.project_url),
                                });
                            }
                        }
                        in_properties = false;
                    }
                    (b"properties", _) => in_properties = false,
                    _ => {
                        if let (Some(f), Some(p)) = (field.take(), entry.as_mut()) {
                            let value = text.trim().to_string();
                            match f {
                                ChocoField::EntryTitle => p.entry_title = value,
                                ChocoField::Id => p.id = value,
                                ChocoField::Title => p.title = value,
                                ChocoField::Version => p.version = value,
                                ChocoField::ProjectUrl => p.project_url = value,
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                log::debug!("Chocolatey feed parse error: {err}");
                break;
            }
            _ => {}
        }
    }
    packages
}

/// Chocolatey "package fix version notation": `1.2.3.20240814` is version
/// `1.2.3` repackaged on that date.
fn strip_package_fix(version: &str) -> String {
    let version = version.trim();
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 3 {
        if let Some(last) = parts.last() {
            let date_like = last.len() >= 8
                && last.chars().all(|c| c.is_ascii_digit())
                && (last.starts_with("19") || last.starts_with("20"));
            if date_like {
                return parts[..parts.len() - 1].join(".");
            }
        }
    }
    version.to_string()
}

/// The newest package whose normalized title is exactly the wanted name:
/// `(version, id)`.
fn pick_chocolatey(packages: &[ChocoPackage], wanted: &str, publisher: Option<&str>) -> Option<(String, String)> {
    packages
        .iter()
        .filter(|p| normalize_title(&p.title, publisher) == wanted)
        .map(|p| (strip_package_fix(&p.version), p.id.clone()))
        .filter(|(v, _)| Version::parse(v).is_some())
        .max_by(|(va, ia), (vb, ib)| version::compare(va, vb).then_with(|| ib.len().cmp(&ia.len())))
}

// ---------------------------------------------------------------------------
// Name normalization
// ---------------------------------------------------------------------------

/// Dotted versions: `3.7.1`, `v8.6.9`, `24.08`, `1.2.3-beta`, `8_10_1`.
static DOTTED_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bv?\d+(?:[._]\d+)+[0-9a-z.+_-]*").expect("valid regex"));

const ARCH_WORDS: &[&str] = &[
    "x64", "x86", "x32", "amd64", "arm64", "aarch64", "win64", "win32", "ia32", "i386", "i686", "32bit", "64bit", "bit",
];
const EDITION_WORDS: &[&str] = &["edition", "free", "freeware", "community", "home", "personal", "stable", "release"];
const COMPANY_WORDS: &[&str] = &[
    "the", "inc", "llc", "ltd", "limited", "gmbh", "corp", "corporation", "co", "company", "software", "foundation",
    "team", "developers", "project", "community", "sa", "ag", "bv", "srl", "sas", "technologies", "systems",
];

/// Lowercase words of a display name without parenthesized groups, versions,
/// architecture and edition words, or trailing numbers.
fn display_words(name: &str) -> Vec<String> {
    let s: String = strip_parenthesized(name)
        .chars()
        .filter(|c| !matches!(c, '®' | '™' | '©'))
        .flat_map(char::to_lowercase)
        .collect();
    let s = DOTTED_VERSION.replace_all(&s, " ");
    let mut words: Vec<String> = s
        .split(|c: char| !(c.is_alphanumeric() || c == '+' || c == '#'))
        .filter(|w| !w.is_empty() && w.chars().any(char::is_alphanumeric))
        .filter(|w| !ARCH_WORDS.contains(w) && !EDITION_WORDS.contains(w))
        .map(str::to_string)
        .collect();
    while words.len() > 1 && words.last().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit())) {
        words.pop();
    }
    // `64-bit`: the number is architecture noise.
    words.retain(|w| w != "32" && w != "64");
    words
}

/// Comparable title: [`display_words`] without the publisher's name in front
/// (`Mozilla Firefox` by `Mozilla` → `firefox`).
fn normalize_title(name: &str, publisher: Option<&str>) -> String {
    let mut words = display_words(name);
    if let Some(publisher) = publisher {
        let publisher_words: Vec<String> =
            display_words(publisher).into_iter().filter(|w| !COMPANY_WORDS.contains(&w.as_str())).collect();
        let common = words.iter().zip(&publisher_words).take_while(|(a, b)| a == b).count();
        if common > 0 && common < words.len() {
            words.drain(..common);
        }
    }
    words.join(" ")
}

/// Search term sent to Chocolatey: the cleaned display name only.
fn search_term(name: &str) -> String {
    display_words(name).join(" ")
}

/// Removes `(...)` and `[...]` groups.
fn strip_parenthesized(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Percent-encoding of everything but unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn github_urls() {
        let repo = |u: &str| github_repo(&urls(&[u]));
        let pair = |o: &str, r: &str| Some((o.to_string(), r.to_string()));
        assert_eq!(repo("https://github.com/keepassxreboot/keepassxc"), pair("keepassxreboot", "keepassxc"));
        assert_eq!(repo("http://www.GitHub.com/Owner/Repo.git"), pair("Owner", "Repo"));
        assert_eq!(repo("https://github.com/owner/repo/releases/tag/v1.0"), pair("owner", "repo"));
        assert_eq!(repo("https://github.com/owner/repo/issues"), pair("owner", "repo"));
        assert_eq!(repo("github.com/owner/my.repo#readme"), pair("owner", "my.repo"));
        assert_eq!(repo("https://github.com/owner/repo?tab=readme-ov-file"), pair("owner", "repo"));
        assert_eq!(repo("git+https://github.com/owner/repo.git"), pair("owner", "repo"));
        assert_eq!(repo("https://github.com/owner"), None);
        assert_eq!(repo("https://github.com/owner/"), None);
        assert_eq!(repo("https://github.com/features/actions"), None);
        assert_eq!(repo("https://github.com/sponsors/someone"), None);
        assert_eq!(repo("https://gist.github.com/owner/abc"), None);
        assert_eq!(repo("https://example.com/github.com/a/b"), None);
        assert_eq!(repo("https://github.com.evil.com/a/b"), None);
        assert_eq!(repo("not a url"), None);
        assert_eq!(
            github_repo(&urls(&["https://example.com/", "https://github.com/a/b", "https://github.com/c/d"])),
            pair("a", "b")
        );
        assert_eq!(github_repo(&[]), None);
    }

    #[test]
    fn known_programs() {
        let links = known_links("Euro-Office (Standalone) 9.3.5 (x64)").unwrap();
        assert_eq!(links.homepage.as_deref(), Some("https://github.com/Euro-Office/DesktopEditors"));
        assert_eq!(links.download.as_deref(), Some("https://github.com/Euro-Office/DesktopEditors/releases/latest"));
        let found: Vec<String> = [links.homepage, links.download, links.support].into_iter().flatten().collect();
        assert_eq!(github_repo(&found), Some(("Euro-Office".to_string(), "DesktopEditors".to_string())));
        assert!(known_links("  euro-office desktop editors").is_some());
        assert!(known_links("ONLYOFFICE Desktop Editors 9.3.1 (x64)").is_none());

        // Its tags carry a package revision: v9.3.5-2 is the installed 9.3.5.0.
        let tag = version_from_tag("v9.3.5-2").unwrap();
        assert!(same_scheme(&tag, "9.3.5.0"));
        assert!(!version::is_newer(&tag, "9.3.5.0"));
        assert!(version::is_newer(&version_from_tag("v9.3.6-1").unwrap(), "9.3.5.0"));
        assert!(!version::is_newer(&version_from_tag("v9.3.6-1").unwrap(), "9.3.6.0"));
    }

    /// The real Euro-Office releases, as an older 9.3.4 would see them. Run
    /// with `--ignored`.
    #[test]
    #[ignore = "network"]
    fn euro_office_releases() {
        let name = "Euro-Office (Standalone) 9.3.4 (x64)";
        let links = known_links(name).unwrap();
        let input = LookupInput {
            name: name.into(),
            publisher: Some("Ascensio System SIA".into()),
            version: Some("9.3.4.0".into()),
            urls: [links.homepage, links.download, links.support].into_iter().flatten().collect(),
        };
        match latest_elsewhere(&input) {
            Some(Alternative::LatestElsewhere { version, origin, url }) => {
                println!("{version} from {origin}: {url:?}");
                assert_eq!(origin, "github");
                assert!(url.unwrap().starts_with("https://github.com/Euro-Office/DesktopEditors/releases/"));
            }
            other => panic!("no newer release found: {other:?}"),
        }
    }

    #[test]
    fn tags_to_versions() {
        let v = |t: &str| version_from_tag(t);
        assert_eq!(v("v1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(v("V2.0").as_deref(), Some("2.0"));
        assert_eq!(v("1.0.0").as_deref(), Some("1.0.0"));
        assert_eq!(v("Audacity-3.7.1").as_deref(), Some("3.7.1"));
        assert_eq!(v("curl-8_10_1").as_deref(), Some("8.10.1"));
        assert_eq!(v("release/2.0").as_deref(), Some("2.0"));
        assert_eq!(v("keepassxc 2.7.9").as_deref(), Some("2.7.9"));
        assert_eq!(v("app-v4.5.6").as_deref(), Some("4.5.6"));
        assert_eq!(v("OBS 30.2.3 hotfix").as_deref(), Some("30.2.3"));
        assert_eq!(v("refs/tags/v0.9.1").as_deref(), Some("0.9.1"));
        assert_eq!(v("1.4.0-beta.2").as_deref(), Some("1.4.0-beta.2"));
        assert_eq!(v("latest"), None);
        assert_eq!(v("nightly"), None);
        assert_eq!(v("build7"), None);
        assert_eq!(v(""), None);
    }

    #[test]
    fn schemes() {
        assert!(same_scheme("1.2.3", "1.1"));
        assert!(same_scheme("2024.10", "2023.1"));
        assert!(!same_scheme("2024.10.01", "1.5"));
        assert!(!same_scheme("20241001", "1.5"));
        assert!(!same_scheme("latest", "1.5"));
    }

    const FEED: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<feed xml:base="https://community.chocolatey.org/api/v2/" xmlns:d="http://schemas.microsoft.com/ado/2007/08/dataservices" xmlns:m="http://schemas.microsoft.com/ado/2007/08/dataservices/metadata" xmlns="http://www.w3.org/2005/Atom">
  <title type="text">Search</title>
  <entry>
    <id>https://community.chocolatey.org/api/v2/Packages(Id='audacity',Version='3.7.9')</id>
    <title type="text">audacity</title>
    <m:properties>
      <d:Version>3.7.9</d:Version>
      <d:Title>Audacity</d:Title>
      <d:Description>Audacity &amp; friends</d:Description>
      <d:ProjectUrl>https://audacityteam.org/</d:ProjectUrl>
    </m:properties>
  </entry>
  <entry>
    <id>https://community.chocolatey.org/api/v2/Packages(Id='audacity-lame',Version='3.100.0.20190331')</id>
    <title type="text">audacity-lame</title>
    <m:properties>
      <d:Version>3.100.0.20190331</d:Version>
      <d:Title>LAME for Audacity</d:Title>
      <d:ProjectUrl m:null="true" />
    </m:properties>
  </entry>
  <entry>
    <title type="text">ignored-title</title>
    <m:properties>
      <d:Id>notepadplusplus</d:Id>
      <d:Version>8.7.1</d:Version>
      <d:Title>Notepad&#43;&#43; &amp; co</d:Title>
    </m:properties>
  </entry>
  <entry>
    <title type="text">broken</title>
    <m:properties><d:Title>No version</d:Title></m:properties>
  </entry>
</feed>"#;

    #[test]
    fn chocolatey_feed_parsing() {
        let packages = parse_chocolatey_feed(FEED);
        assert_eq!(packages.len(), 3);
        assert_eq!(
            packages[0],
            ChocoPackage {
                id: "audacity".into(),
                title: "Audacity".into(),
                version: "3.7.9".into(),
                project_url: Some("https://audacityteam.org/".into()),
            }
        );
        assert_eq!(packages[1].id, "audacity-lame");
        assert_eq!(packages[1].project_url, None);
        assert_eq!(packages[2].id, "notepadplusplus");
        assert_eq!(packages[2].title, "Notepad++ & co");
        assert!(parse_chocolatey_feed("<html>error</html>").is_empty());
        assert!(parse_chocolatey_feed("not xml at all <<<").is_empty());
    }

    #[test]
    fn package_fix_versions() {
        assert_eq!(strip_package_fix("3.100.0.20190331"), "3.100.0");
        assert_eq!(strip_package_fix("24.8.0.20240814"), "24.8.0");
        assert_eq!(strip_package_fix("3.7.9"), "3.7.9");
        assert_eq!(strip_package_fix("2024.10.01"), "2024.10.01");
        assert_eq!(strip_package_fix("1.20240814"), "1.20240814");
    }

    #[test]
    fn title_normalization() {
        let n = |name: &str, publisher: Option<&str>| normalize_title(name, publisher);
        assert_eq!(n("Mozilla Firefox (x64 fr)", Some("Mozilla")), n("Mozilla Firefox", Some("Mozilla")));
        assert_eq!(n("Mozilla Firefox (x64 fr)", Some("Mozilla")), "firefox");
        assert_eq!(n("Firefox", Some("Mozilla")), "firefox");
        assert_eq!(n("7-Zip 24.08 (x64)", Some("Igor Pavlov")), "7 zip");
        assert_eq!(n("7-Zip", Some("Igor Pavlov")), "7 zip");
        assert_eq!(n("Notepad++ (64-bit x64)", Some("Notepad++ Team")), "notepad++");
        assert_eq!(n("PuTTY release 0.81 (64-bit)", Some("Simon Tatham")), "putty");
        assert_eq!(n("Audacity 3.7.1", None), "audacity");
        assert_eq!(n("calibre 64bit", Some("Kovid Goyal")), "calibre");
        assert_eq!(n("WinRAR 7.01 (64-bit)", Some("win.rar GmbH")), "winrar");
        assert_eq!(n("Google Chrome", Some("Google LLC")), "chrome");
        assert_eq!(n("TeamViewer", Some("TeamViewer")), "teamviewer");
        assert_eq!(n("Python 3.13.5 (64-bit)", Some("Python Software Foundation")), "python");
        assert_eq!(n("OBS Studio 30", None), "obs studio");
        assert_eq!(n("Paint.NET", None), "paint net");
        assert_ne!(n("Audacity Portable", None), "audacity");
        assert_ne!(n("LAME for Audacity", None), "audacity");
    }

    #[test]
    fn chocolatey_choice() {
        let packages = parse_chocolatey_feed(FEED);
        assert_eq!(
            pick_chocolatey(&packages, &normalize_title("Audacity 3.0.0", None), None),
            Some(("3.7.9".to_string(), "audacity".to_string()))
        );
        assert_eq!(pick_chocolatey(&packages, "tenacity", None), None);
        // Same title twice: newest version, then the shortest id.
        let twins = vec![
            ChocoPackage {
                id: "7zip.install".into(),
                title: "7-Zip (Install)".into(),
                version: "24.9.0".into(),
                project_url: None,
            },
            ChocoPackage { id: "7zip".into(), title: "7-Zip".into(), version: "24.9.0".into(), project_url: None },
            ChocoPackage { id: "7zip.old".into(), title: "7-Zip".into(), version: "9.20".into(), project_url: None },
        ];
        assert_eq!(pick_chocolatey(&twins, "7 zip", None), Some(("24.9.0".to_string(), "7zip".to_string())));
    }

    #[test]
    fn search_urls() {
        assert_eq!(web_search_url("Foo Bar 1.2.3 (x64)"), "https://duckduckgo.com/?q=Foo%20Bar%20download%20official");
        assert_eq!(web_search_url("Notepad++"), "https://duckduckgo.com/?q=Notepad%2B%2B%20download%20official");
        assert_eq!(web_search_url("Café & Co"), "https://duckduckgo.com/?q=Caf%C3%A9%20Co%20download%20official");
        assert_eq!(search_term("Mozilla Firefox (x64 fr)"), "mozilla firefox");
        assert_eq!(encode("a b'c"), "a%20b%27c");
    }

    #[test]
    fn breaker_opens_after_repeated_failures() {
        let breaker = Breaker::new();
        assert!(!breaker.is_open());
        breaker.failure();
        breaker.failure();
        breaker.success();
        breaker.failure();
        breaker.failure();
        assert!(!breaker.is_open());
        breaker.failure();
        assert!(breaker.is_open());
        let rate_limited = Breaker::new();
        rate_limited.open_for(Duration::from_secs(60));
        assert!(rate_limited.is_open());
        rate_limited.open_for(Duration::ZERO);
        assert!(!rate_limited.is_open());
    }

    #[test]
    fn no_lookup_without_installed_version() {
        let input = LookupInput {
            name: "Audacity".into(),
            urls: urls(&["https://github.com/audacity/audacity"]),
            ..Default::default()
        };
        assert_eq!(latest_elsewhere(&input), None);
        let unknown = LookupInput { version: Some("Unknown".into()), ..input };
        assert_eq!(latest_elsewhere(&unknown), None);
    }

    /// Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "network"]
    fn real_lookups() {
        let input = |name: &str, publisher: Option<&str>, version: &str, url: &[&str]| LookupInput {
            name: name.into(),
            publisher: publisher.map(str::to_string),
            version: Some(version.into()),
            urls: urls(url),
        };
        let cases = [
            input("KeePassXC", Some("KeePassXC Team"), "2.7.0", &["https://github.com/keepassxreboot/keepassxc"]),
            input("KeePassXC", Some("KeePassXC Team"), "999.0", &["https://github.com/keepassxreboot/keepassxc"]),
            input("Audacity 3.0.0", Some("Audacity Team"), "3.0.0", &["https://www.audacityteam.org/"]),
            input(
                "PuTTY release 0.70 (64-bit)",
                Some("Simon Tatham"),
                "0.70",
                &["https://www.chiark.greenend.org.uk/~sgtatham/putty/"],
            ),
            input("Notepad++ (64-bit x64)", Some("Notepad++ Team"), "8.0", &["https://notepad-plus-plus.org/"]),
            input("Totally Unknown Thing XYZ", None, "1.0", &[]),
        ];
        match chocolatey_search("audacity") {
            Ok(packages) => println!("chocolatey search: {} package(s): {packages:?}", packages.len()),
            Err(err) => println!("chocolatey search failed: {err:#}"),
        }
        for case in &cases {
            let found = latest_elsewhere(case);
            println!("{} {:?} -> {found:?}", case.name, case.version);
        }
        assert!(
            matches!(latest_elsewhere(&cases[0]), Some(Alternative::LatestElsewhere { ref origin, .. }) if origin == "github")
        );
        assert_eq!(latest_elsewhere(&cases[1]), None);
        assert_eq!(latest_elsewhere(&cases[5]), None);
    }
}
