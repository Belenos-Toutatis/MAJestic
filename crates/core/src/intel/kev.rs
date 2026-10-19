//! CISA Known Exploited Vulnerabilities catalog.
//! <https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json>
//!
//! OWNER: intel agent.
//!
//! Matching is deliberately conservative: a false "actively exploited" alert
//! on a harmless program destroys the user's trust in every other alert.

use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context};
use serde::Deserialize;

use super::http;
use crate::model::Reason;

const CATALOG_URL: &str = "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";
const CACHE_FILE: &str = "kev.json";
const MAX_CVES: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevEntry {
    pub cve: String,
    pub vendor: String,
    pub product: String,
    pub name: String,
    /// YYYY-MM-DD
    pub date_added: String,
    pub ransomware: bool,
}

#[derive(Debug, Clone, Default)]
pub struct KevCatalog {
    pub entries: Vec<KevEntry>,
}

impl KevCatalog {
    /// Entries concerning a program, matched conservatively on its display
    /// name and publisher: every significant word of the KEV product must be
    /// in the name, and the vendor must match the publisher or the name.
    /// Generic products (`Windows`, `Office`, ...) only match Microsoft
    /// software with that exact product name.
    pub fn lookup(&self, name: &str, publisher: Option<&str>) -> Vec<&KevEntry> {
        let subject = Subject::new(name, publisher);
        if subject.sig.is_empty() {
            return Vec::new();
        }
        self.entries.iter().filter(|entry| subject.matches(entry)).collect()
    }
}

/// Loads the catalog from `cache_dir/kev.json` when younger than `max_age`,
/// downloads it otherwise (when `online`), falls back to a stale cache.
pub fn load(cache_dir: &Path, max_age: Duration, online: bool) -> anyhow::Result<KevCatalog> {
    let path = cache_dir.join(CACHE_FILE);

    let cache_age = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .map(|modified| SystemTime::now().duration_since(modified).unwrap_or_default());
    if cache_age.is_some_and(|age| age < max_age) {
        match read_cache(&path) {
            Ok(catalog) => return Ok(catalog),
            Err(err) => log::warn!("KEV cache unreadable, downloading again: {err:#}"),
        }
    }

    let mut download_error = None;
    if online {
        match download(cache_dir) {
            Ok(catalog) => return Ok(catalog),
            Err(err) => {
                log::warn!("KEV download failed: {err:#}");
                download_error = Some(err);
            }
        }
    }

    if cache_age.is_some() {
        if let Ok(catalog) = read_cache(&path) {
            log::info!("using stale KEV cache ({} entries)", catalog.entries.len());
            return Ok(catalog);
        }
    }
    match download_error {
        Some(err) => Err(err.context("KEV catalog unavailable (no usable cache)")),
        None => bail!("KEV catalog unavailable: offline and no cache"),
    }
}

/// Builds the `Reason::KnownExploited` for a set of hits (`None` if empty).
/// `today` is YYYY-MM-DD.
pub fn to_reason(hits: &[&KevEntry], today: &str) -> Option<Reason> {
    let mut sorted: Vec<&KevEntry> = hits.to_vec();
    sorted.sort_by(|a, b| b.date_added.cmp(&a.date_added).then_with(|| cve_key(&b.cve).cmp(&cve_key(&a.cve))));
    let most_recent = sorted.first()?.date_added.clone();

    let mut cves: Vec<String> = Vec::new();
    for entry in &sorted {
        if !cves.contains(&entry.cve) {
            cves.push(entry.cve.clone());
            if cves.len() == MAX_CVES {
                break;
            }
        }
    }

    Some(Reason::KnownExploited {
        cves,
        recent: is_recent(&most_recent, today),
        most_recent,
        ransomware: hits.iter().any(|e| e.ransomware),
    })
}

/// `CVE-2026-11645` → `(2026, 11645)`, for a numeric ordering.
fn cve_key(cve: &str) -> (u32, u64, &str) {
    let mut parts = cve.trim().splitn(3, '-').skip(1);
    let year = parts.next().and_then(|y| y.parse().ok()).unwrap_or(0);
    let number = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    (year, number, cve)
}

/// `date` (YYYY-MM-DD) is less than two years before `today`.
fn is_recent(date: &str, today: &str) -> bool {
    use chrono::{Months, NaiveDate};
    let parse = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    match (parse(date), parse(today)) {
        (Some(date), Some(today)) => today.checked_sub_months(Months::new(24)).is_some_and(|limit| date > limit),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Catalog download and cache
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawCatalog {
    #[serde(default)]
    vulnerabilities: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "cveID", default)]
    cve_id: Option<String>,
    #[serde(rename = "vendorProject", default)]
    vendor_project: Option<String>,
    #[serde(default)]
    product: Option<String>,
    #[serde(rename = "vulnerabilityName", default)]
    vulnerability_name: Option<String>,
    #[serde(rename = "dateAdded", default)]
    date_added: Option<String>,
    #[serde(rename = "knownRansomwareCampaignUse", default)]
    known_ransomware_campaign_use: Option<String>,
}

fn parse_catalog(json: &str) -> anyhow::Result<KevCatalog> {
    let raw: RawCatalog = serde_json::from_str(json.trim_start_matches('\u{feff}')).context("invalid KEV JSON")?;
    let entries: Vec<KevEntry> = raw
        .vulnerabilities
        .into_iter()
        .filter_map(|v| {
            let cve = v.cve_id?.trim().to_string();
            if cve.is_empty() {
                return None;
            }
            Some(KevEntry {
                cve,
                vendor: v.vendor_project.unwrap_or_default().trim().to_string(),
                product: v.product.unwrap_or_default().trim().to_string(),
                name: v.vulnerability_name.unwrap_or_default().trim().to_string(),
                date_added: v.date_added.unwrap_or_default().trim().to_string(),
                ransomware: v.known_ransomware_campaign_use.is_some_and(|s| s.trim().eq_ignore_ascii_case("known")),
            })
        })
        .collect();
    if entries.is_empty() {
        bail!("KEV catalog has no entries");
    }
    Ok(KevCatalog { entries })
}

fn read_cache(path: &Path) -> anyhow::Result<KevCatalog> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_catalog(&text)
}

fn download(cache_dir: &Path) -> anyhow::Result<KevCatalog> {
    let text = http::get_text(CATALOG_URL)?;
    // Validate before caching, so an error page never replaces a good cache.
    let catalog = parse_catalog(&text)?;
    if let Err(err) = write_atomically(&cache_dir.join(CACHE_FILE), &text) {
        log::warn!("cannot cache the KEV catalog: {err:#}");
    }
    Ok(catalog)
}

fn write_atomically(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

/// Words that carry no identity: articles, company forms, generic words.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "the", "of", "for", "by", "on", "with", "to", "inc", "llc", "ltd", "limited", "gmbh", "corp",
    "corporation", "incorporated", "co", "company", "sa", "ag", "bv", "srl", "sas", "plc", "pty", "kg", "software",
    "systems", "technologies", "technology", "networks", "labs", "solutions", "group", "international", "foundation",
    "team", "developers", "community", "edition", "editions", "multiple", "products", "product", "windows",
    "application", "applications", "app", "apps", "tm", "r",
];

/// Words in a program name showing it is an add-on or a satellite of the
/// product rather than the product itself (`Git Extensions`, `Java Auto
/// Updater`, `ScreenConnect Client`...).
const COMPANION_WORDS: &[&str] = &[
    "extension", "extensions", "plugin", "plugins", "plug", "addin", "addins", "add", "addon", "addons", "connector",
    "credential", "lfs", "helper", "updater", "toolbar", "integration", "companion", "client", "agent", "bridge",
    "service", "guest", "uninstaller",
];

/// Tokens ignored when comparing "core" names (architecture, channel,
/// edition, language).
const NOISE_WORDS: &[&str] = &[
    "bit", "x64", "x86", "arm64", "amd64", "ia32", "win32", "win64", "beta", "dev", "canary", "stable", "nightly",
    "insider", "insiders", "preview", "professional", "pro", "plus", "proplus", "home", "student", "business",
    "standard", "enterprise", "ultimate", "premium", "personal", "ltsc", "express", "en", "us", "gb", "fr", "de", "es",
    "it", "nl", "pt", "br", "ja", "jp", "ko", "kr", "zh", "cn", "tw", "ru", "pl", "sv", "se", "da", "fi", "nb", "cs",
];

/// Chromium-based browsers, as core names.
const CHROMIUM_BROWSERS: &[&[&str]] = &[
    &["google", "chrome"],
    &["chromium"],
    &["microsoft", "edge"],
    &["brave"],
    &["brave", "browser"],
    &["opera"],
    &["opera", "gx"],
    &["vivaldi"],
];

/// Browsers concerned by products labelled `Chrome ...` (rather than
/// `Chromium ...`).
const CHROME_BROWSERS: &[&[&str]] = &[&["google", "chrome"], &["chromium"]];

/// Google products that are Chromium components shipped in every Chromium
/// browser.
const CHROMIUM_COMPONENTS: &[&str] = &["skia", "dawn"];

/// Office component products that concern an Office suite install.
const OFFICE_COMPONENTS: &[&[&str]] =
    &[&["office"], &["excel"], &["word"], &["powerpoint"], &["outlook"], &["office", "outlook"], &["publisher"]];

/// Microsoft products that are part of Windows (or its security platform):
/// never matched to an installed program.
const MICROSOFT_PLATFORM: &[&[&str]] = &[&["defender"], &["win32k"], &["internet", "explorer"], &["smbv1"]];

/// Vendors handled by [`Subject::matches_alias`] (their product label is not
/// in the program name).
const ALIAS_VENDORS: &[&str] = &["oracle", "teamviewer"];

/// Other names a KEV vendor publishes under.
const VENDOR_ALIASES: &[(&[&str], &[&[&str]])] = &[
    (&["rarlab"], &[&["win", "rar"], &["alexander", "roshal"]]),
    (&["broadcom"], &[&["vmware"]]),
    (&["vmware"], &[&["broadcom"]]),
];

fn is_stop(word: &str) -> bool {
    STOP_WORDS.contains(&word)
}

fn is_noise(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_digit()) || NOISE_WORDS.contains(&word)
}

/// Lowercase; `®`, `™`, `©` and apostrophes removed; every other character
/// that is not alphanumeric (or `+`, for `Notepad++`) becomes a space.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '®' | '™' | '©' | '\'' | '’' => {}
            c if c.is_alphanumeric() || c == '+' => out.extend(c.to_lowercase()),
            _ => out.push(' '),
        }
    }
    out
}

/// Normalized words, stop words included.
fn words(s: &str) -> Vec<String> {
    normalize(s).split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).map(str::to_string).collect()
}

/// Normalized significant words (stop words removed).
fn sig_tokens(s: &str) -> Vec<String> {
    words(s).into_iter().filter(|w| !is_stop(w)).collect()
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

/// Significant words without architecture/version/channel/edition noise,
/// parenthesized groups removed.
fn core_tokens(s: &str) -> Vec<String> {
    sig_tokens(&strip_parenthesized(s)).into_iter().filter(|w| !is_noise(w)).collect()
}

/// Product alternatives: `Firefox, Firefox ESR, and Thunderbird` →
/// `[firefox] [firefox esr] [thunderbird]`. Parenthesized acronyms are
/// dropped; alternatives too short to identify anything are skipped.
fn product_alternatives(product: &str) -> Vec<Vec<String>> {
    let separated = strip_parenthesized(product).replace([',', '/', '&', ';'], " and ");
    words(&separated)
        .split(|w| w == "and")
        .map(|group| group.iter().filter(|w| !is_stop(w)).cloned().collect::<Vec<String>>())
        .filter(|group| group.iter().map(|w| w.chars().count()).sum::<usize>() >= 3)
        .collect()
}

fn contains_run(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

fn same(tokens: &[String], expected: &[&str]) -> bool {
    tokens.len() == expected.len() && tokens.iter().zip(expected).all(|(a, b)| a == b)
}

fn starts_with(tokens: &[String], prefix: &[String]) -> bool {
    tokens.len() >= prefix.len() && tokens[..prefix.len()] == *prefix
}

/// The program being matched, pre-tokenized once.
struct Subject {
    /// Significant words of the display name.
    sig: Vec<String>,
    /// Same, without parenthesized groups.
    sig_bare: Vec<String>,
    /// Core name (no noise), see [`core_tokens`].
    core: Vec<String>,
    publisher: Vec<String>,
    chromium_browser: bool,
    chrome_browser: bool,
    office_suite: bool,
}

impl Subject {
    fn new(name: &str, publisher: Option<&str>) -> Subject {
        let sig = sig_tokens(name);
        let sig_bare = sig_tokens(&strip_parenthesized(name));
        let core = core_tokens(name);
        let chromium_browser = CHROMIUM_BROWSERS.iter().any(|b| same(&core, b));
        let chrome_browser = CHROME_BROWSERS.iter().any(|b| same(&core, b));
        // `Microsoft Office Professional Plus 2019 - fr-fr`, `Microsoft 365
        // Apps for enterprise - en-us`; not `Microsoft Office Proofing Tools`.
        let office_suite = sig_bare.first().is_some_and(|w| w == "microsoft")
            && match sig_bare.get(1).map(String::as_str) {
                Some("office") => same(&core, &["microsoft", "office"]),
                Some("365") => same(&core, &["microsoft"]),
                _ => false,
            };
        Subject {
            sig,
            sig_bare,
            core,
            publisher: publisher.map(sig_tokens).unwrap_or_default(),
            chromium_browser,
            chrome_browser,
            office_suite,
        }
    }

    fn matches(&self, entry: &KevEntry) -> bool {
        let vendor_raw = entry.vendor.trim();
        if vendor_raw.eq_ignore_ascii_case("microsoft") {
            return self.sig_bare.first().is_some_and(|w| w == "microsoft")
                && (self.office_suite || self.may_contain_product(&entry.product))
                && self.matches_microsoft(&entry.product);
        }
        if vendor_raw.eq_ignore_ascii_case("google") {
            if let Some(chrome_only) = chromium_family(&entry.product) {
                return if chrome_only { self.chrome_browser } else { self.chromium_browser };
            }
        }
        // Allocation-free pre-filter (most entries stop here): at least one
        // product word must be in the name, unless an alias applies.
        let alias_vendor = ALIAS_VENDORS.iter().any(|v| vendor_raw.eq_ignore_ascii_case(v));
        if !alias_vendor && !self.may_contain_product(&entry.product) {
            return false;
        }
        let vendor = sig_tokens(&strip_parenthesized(&entry.vendor));
        if vendor.is_empty() {
            return false;
        }
        self.matches_alias(&vendor, &entry.product) || self.matches_generic(&vendor, &entry.product)
    }

    /// Conservative pre-filter: `false` only when no word of `product` can be
    /// a word of the name.
    fn may_contain_product(&self, product: &str) -> bool {
        if !product.is_ascii() || product.contains('\'') {
            return true; // let the full normalization decide
        }
        product
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '+'))
            .filter(|w| !w.is_empty())
            .any(|w| self.sig.iter().any(|t| t.eq_ignore_ascii_case(w)))
    }

    /// Microsoft: only an exact product name (`Microsoft Edge` for `Edge`),
    /// or an Office suite for Office components. Windows components never
    /// match. The program name must start with `Microsoft`.
    fn matches_microsoft(&self, product: &str) -> bool {
        let name_core: Vec<String> = self.core.iter().filter(|w| *w != "microsoft").cloned().collect();
        product_alternatives(product).into_iter().any(|alt| {
            let alt: Vec<String> = alt.into_iter().filter(|w| w != "microsoft").collect();
            let alt_core: Vec<String> = alt.iter().filter(|w| !is_noise(w)).cloned().collect();
            if alt_core.is_empty() || MICROSOFT_PLATFORM.iter().any(|p| same(&alt, p)) {
                return false;
            }
            if self.office_suite && OFFICE_COMPONENTS.iter().any(|c| same(&alt, c)) {
                return true;
            }
            !name_core.is_empty() && name_core == alt_core
        })
    }

    /// Products whose KEV label differs from the installed program's name.
    fn matches_alias(&self, vendor: &[String], product: &str) -> bool {
        let has_companion = self.sig.iter().any(|w| COMPANION_WORDS.contains(&w.as_str()));
        if has_companion {
            return false;
        }
        let alternatives = product_alternatives(product);
        // Oracle Java: `Java 8 Update 401`, `Java(TM) SE Development Kit 17`.
        if same(vendor, &["oracle"]) {
            let java_product =
                alternatives.iter().any(|alt| alt.iter().any(|w| w == "java" || w == "jre" || w == "jdk"));
            return java_product
                && self.sig_bare.first().is_some_and(|w| w == "java")
                && self.publisher.iter().any(|w| w == "oracle");
        }
        // TeamViewer `Desktop` is the TeamViewer program itself.
        if same(vendor, &["teamviewer"]) {
            return alternatives.iter().any(|alt| same(alt, &["desktop"]))
                && self.sig_bare.first().is_some_and(|w| w == "teamviewer");
        }
        false
    }

    /// Every significant word of one product alternative appears (in order,
    /// contiguously) in the name, the name is not an add-on, and the vendor
    /// matches the publisher or appears in the name. When the product is
    /// named after the vendor (`Git`, `7-Zip`), the name must be exactly the
    /// product plus versions/architecture.
    fn matches_generic(&self, vendor: &[String], product: &str) -> bool {
        let vendor_ok = contains_run(&self.publisher, vendor)
            || contains_run(&self.sig, vendor)
            || VENDOR_ALIASES.iter().any(|(v, aliases)| {
                same(vendor, v)
                    && aliases.iter().any(|alias| {
                        let alias: Vec<String> = alias.iter().map(|w| w.to_string()).collect();
                        contains_run(&self.publisher, &alias) || contains_run(&self.sig, &alias)
                    })
            });
        if !vendor_ok {
            return false;
        }
        product_alternatives(product).into_iter().any(|alt| {
            if alt == vendor {
                return starts_with(&self.sig_bare, &alt) && self.sig_bare[alt.len()..].iter().all(|w| is_noise(w));
            }
            contains_run(&self.sig, &alt)
                && !self.sig.iter().any(|w| COMPANION_WORDS.contains(&w.as_str()) && !alt.contains(w))
        })
    }
}

/// `Some(chrome_only)` when a Google product is part of Chromium:
/// `Chromium V8` (every Chromium browser), `Chrome Blink` (Google Chrome).
fn chromium_family(product: &str) -> Option<bool> {
    let words = words(product);
    if words.iter().any(|w| w == "android" || w == "ios") {
        return None;
    }
    match words.first().map(String::as_str) {
        Some("chromium") => Some(false),
        Some("chrome") => Some(true),
        Some(w) if words.len() == 1 && CHROMIUM_COMPONENTS.contains(&w) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(cve: &str, vendor: &str, product: &str, date: &str, ransomware: bool) -> KevEntry {
        KevEntry {
            cve: cve.into(),
            vendor: vendor.into(),
            product: product.into(),
            name: format!("{vendor} {product} Vulnerability"),
            date_added: date.into(),
            ransomware,
        }
    }

    fn synthetic() -> KevCatalog {
        KevCatalog {
            entries: vec![
                entry("CVE-2025-0001", "Google", "Chromium V8", "2025-06-01", false),
                entry("CVE-2025-0002", "Google", "Chrome Blink", "2025-05-01", false),
                entry("CVE-2026-0003", "Google", "Skia", "2026-03-13", false),
                entry("CVE-2025-0004", "Google", "Chrome for Android UI", "2025-01-01", false),
                entry("CVE-2025-0010", "Microsoft", "Windows", "2025-09-01", true),
                entry("CVE-2025-0011", "Microsoft", "Win32k", "2025-09-02", false),
                entry("CVE-2025-0012", "Microsoft", "Office", "2026-01-26", false),
                entry("CVE-2025-0013", "Microsoft", "Excel", "2022-03-03", false),
                entry("CVE-2025-0014", "Microsoft", "Edge and Internet Explorer", "2022-03-28", true),
                entry("CVE-2025-0015", "Microsoft", "Defender", "2026-05-20", false),
                entry("CVE-2025-0016", "Microsoft", "SQL Server", "2026-08-26", false),
                entry("CVE-2025-0017", "Microsoft", "Microsoft", "2024-01-01", false),
                entry("CVE-2025-0018", "Microsoft", ".NET Core and Visual Studio", "2023-08-09", false),
                entry("CVE-2025-0020", "7-Zip", "7-Zip", "2025-02-06", false),
                entry("CVE-2025-0021", "Git", "Git", "2025-08-25", false),
                entry("CVE-2025-0022", "RARLAB", "WinRAR", "2025-08-12", true),
                entry("CVE-2025-0023", "Mozilla", "Firefox and Thunderbird", "2022-05-23", false),
                entry("CVE-2025-0024", "Mozilla", "Multiple Products", "2025-10-06", false),
                entry("CVE-2025-0025", "Apache", "Apache", "2023-01-01", false),
                entry("CVE-2025-0026", "Oracle", "Java SE", "2022-03-03", true),
                entry("CVE-2025-0027", "Oracle", "VirtualBox", "2022-03-03", false),
                entry("CVE-2025-0028", "TeamViewer", "Desktop", "2021-11-03", false),
                entry("CVE-2025-0029", "Notepad++", "Notepad++", "2026-02-12", false),
                entry("CVE-2025-0030", "Adobe", "Acrobat and Reader", "2026-04-13", false),
                entry("CVE-2025-0031", "Docker", "Desktop Community Edition", "2021-11-03", false),
                entry("CVE-2025-0032", "VMware", "ESXi, Workstation, and Fusion", "2025-03-04", false),
                entry("CVE-2025-0033", "ConnectWise", "ScreenConnect", "2024-02-22", true),
                entry("CVE-2025-0034", "Erlang", "Erlang/OTP", "2025-06-09", false),
                entry("CVE-2025-0035", "Adobe", "Commerce and\u{202f}Magento", "2026-09-08", false),
            ],
        }
    }

    fn cves(catalog: &KevCatalog, name: &str, publisher: Option<&str>) -> Vec<String> {
        let mut v: Vec<String> = catalog.lookup(name, publisher).iter().map(|e| e.cve.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn normalization() {
        assert_eq!(sig_tokens("Adobe® Acrobat™ Reader DC (64-bit)"), ["adobe", "acrobat", "reader", "dc", "64", "bit"]);
        assert_eq!(sig_tokens("Notepad++ (64-bit x64)"), ["notepad++", "64", "bit", "x64"]);
        assert_eq!(sig_tokens("Google LLC"), ["google"]);
        assert_eq!(sig_tokens("The Git Development Community"), ["git", "development"]);
        assert_eq!(core_tokens("Opera Stable 112.0.5197.53"), ["opera"]);
        assert_eq!(core_tokens("Microsoft Office Professional Plus 2019 - fr-fr"), ["microsoft", "office"]);
        assert_eq!(
            product_alternatives("Firefox, Firefox ESR, and Thunderbird"),
            vec![vec!["firefox".to_string()], vec!["firefox".into(), "esr".into()], vec!["thunderbird".into()]]
        );
        assert_eq!(
            product_alternatives("Java Runtime Environment (JRE)"),
            vec![vec!["java", "runtime", "environment"]]
        );
        assert_eq!(product_alternatives("Commerce and\u{202f}Magento"), vec![vec!["commerce"], vec!["magento"]]);
        assert!(product_alternatives("Multiple Products").is_empty());
        assert!(product_alternatives("GitLab CE/EE").iter().all(|alt| alt != &vec!["ee".to_string()]));
    }

    #[test]
    fn chromium_browsers() {
        let kev = synthetic();
        let all = vec!["CVE-2025-0001", "CVE-2025-0002", "CVE-2026-0003"];
        assert_eq!(cves(&kev, "Google Chrome", Some("Google LLC")), all);
        assert_eq!(cves(&kev, "Google Chrome Beta", Some("Google LLC")), all);
        let chromium_only = vec!["CVE-2025-0001", "CVE-2026-0003"];
        assert_eq!(cves(&kev, "Brave", Some("Brave Software Inc")), chromium_only);
        assert_eq!(cves(&kev, "Opera Stable 112.0.5197.53", Some("Opera Software")), chromium_only);
        assert_eq!(cves(&kev, "Opera GX Stable 112.0.5197.53", Some("Opera Software")), chromium_only);
        assert_eq!(cves(&kev, "Vivaldi", Some("Vivaldi Technologies AS")), chromium_only);
        // Edge: Chromium entries and its own Edge entries.
        assert_eq!(
            cves(&kev, "Microsoft Edge", Some("Microsoft Corporation")),
            vec!["CVE-2025-0001", "CVE-2025-0014", "CVE-2026-0003"]
        );
        assert!(cves(&kev, "Chrome Remote Desktop Host", Some("Google LLC")).is_empty());
        assert!(cves(&kev, "Google Chrome Remote Desktop", Some("Google LLC")).is_empty());
        assert!(cves(&kev, "Microsoft Edge WebView2 Runtime", Some("Microsoft Corporation")).is_empty());
        assert!(cves(&kev, "Microsoft Edge Update", Some("Microsoft Corporation")).is_empty());
    }

    #[test]
    fn microsoft_generic_products() {
        let kev = synthetic();
        let ms = Some("Microsoft Corporation");
        for name in [
            "Microsoft Teams",
            "Microsoft Visual Studio Code",
            "Microsoft OneDrive",
            "Windows Terminal",
            "Microsoft Windows Desktop Runtime - 8.0.10 (x64)",
            "Microsoft Defender",
            "Microsoft Office Proofing Tools 2016 - Deutsch",
            "Microsoft 365 Copilot",
            "Microsoft SQL Server Management Studio - 20.2",
        ] {
            assert!(cves(&kev, name, ms).is_empty(), "{name} should not match");
        }
        let office = vec!["CVE-2025-0012", "CVE-2025-0013"];
        assert_eq!(cves(&kev, "Microsoft Office Professional Plus 2019 - fr-fr", ms), office);
        assert_eq!(cves(&kev, "Microsoft 365 Apps for enterprise - en-us", ms), office);
        assert_eq!(cves(&kev, "Microsoft Office Home and Business 2021 - fr-fr", ms), office);
        assert_eq!(cves(&kev, "Microsoft SQL Server 2019 (64-bit)", ms), vec!["CVE-2025-0016"]);
        assert_eq!(cves(&kev, "Microsoft Excel 2010", ms), vec!["CVE-2025-0013"]);
    }

    #[test]
    fn regular_products() {
        let kev = synthetic();
        assert_eq!(cves(&kev, "7-Zip 24.08 (x64)", Some("Igor Pavlov")), vec!["CVE-2025-0020"]);
        assert_eq!(cves(&kev, "7-Zip 9.20", None), vec!["CVE-2025-0020"]);
        assert_eq!(cves(&kev, "Git", Some("The Git Development Community")), vec!["CVE-2025-0021"]);
        assert!(cves(&kev, "Git Extensions 5.1.0", Some("Git Extensions Team")).is_empty());
        assert!(cves(&kev, "Git LFS version 3.5.1", Some("GitHub, Inc.")).is_empty());
        assert!(cves(&kev, "Git Cola", None).is_empty());
        assert!(cves(&kev, "GitHub Desktop", Some("GitHub, Inc.")).is_empty());
        assert_eq!(cves(&kev, "WinRAR 7.01 (64-bit)", Some("win.rar GmbH")), vec!["CVE-2025-0022"]);
        assert_eq!(cves(&kev, "Mozilla Firefox (x64 fr)", Some("Mozilla")), vec!["CVE-2025-0023"]);
        assert_eq!(cves(&kev, "Mozilla Thunderbird (x64 en-US)", Some("Mozilla")), vec!["CVE-2025-0023"]);
        assert!(cves(&kev, "Mozilla Maintenance Service", Some("Mozilla")).is_empty());
        assert!(cves(&kev, "Apache OpenOffice 4.1.15", Some("Apache Software Foundation")).is_empty());
        assert_eq!(cves(&kev, "Java 8 Update 401 (64-bit)", Some("Oracle Corporation")), vec!["CVE-2025-0026"]);
        assert_eq!(
            cves(&kev, "Java(TM) SE Development Kit 17.0.12 (64-bit)", Some("Oracle Corporation")),
            vec!["CVE-2025-0026"]
        );
        assert!(cves(&kev, "Java Auto Updater", Some("Oracle Corporation")).is_empty());
        assert!(cves(&kev, "Eclipse Temurin JDK with Hotspot 21.0.4+7 (x64)", Some("Eclipse Adoptium")).is_empty());
        assert_eq!(cves(&kev, "Oracle VirtualBox 7.1.4", Some("Oracle Corporation")), vec!["CVE-2025-0027"]);
        assert_eq!(cves(&kev, "TeamViewer", Some("TeamViewer")), vec!["CVE-2025-0028"]);
        assert!(cves(&kev, "TeamViewer Meeting Add-In", Some("TeamViewer")).is_empty());
        assert_eq!(cves(&kev, "Notepad++ (64-bit x64)", Some("Notepad++ Team")), vec!["CVE-2025-0029"]);
        assert!(cves(&kev, "Windows Notepad", Some("Microsoft Corporation")).is_empty());
        assert_eq!(cves(&kev, "Adobe Acrobat Reader DC", Some("Adobe Systems Incorporated")), vec!["CVE-2025-0030"]);
        assert_eq!(cves(&kev, "Adobe Acrobat (64-bit)", Some("Adobe")), vec!["CVE-2025-0030"]);
        assert!(cves(&kev, "Adobe Creative Cloud", Some("Adobe Inc.")).is_empty());
        assert!(cves(&kev, "Foxit PDF Reader", Some("Foxit Software Inc.")).is_empty());
        assert_eq!(cves(&kev, "Docker Desktop", Some("Docker Inc.")), vec!["CVE-2025-0031"]);
        assert_eq!(cves(&kev, "VMware Workstation", Some("Broadcom Inc.")), vec!["CVE-2025-0032"]);
        assert!(cves(&kev, "ScreenConnect Client (1a2b3c4d)", Some("ScreenConnect Software")).is_empty());
        assert_eq!(cves(&kev, "Erlang OTP 27 (27.1)", Some("Ericsson AB")), vec!["CVE-2025-0034"]);
        // Nothing for programs absent from the catalog.
        for name in ["VLC media player", "Audacity 3.7.1", "KeePass Password Safe 2.57", "PuTTY release 0.81 (64-bit)"]
        {
            assert!(cves(&kev, name, None).is_empty(), "{name}");
        }
    }

    #[test]
    fn reason_building() {
        let a = entry("CVE-2023-1111", "X", "Y", "2023-01-10", false);
        let b = entry("CVE-2025-2222", "X", "Y", "2025-06-01", true);
        let c = entry("CVE-2024-3333", "X", "Y", "2024-02-01", false);
        assert_eq!(to_reason(&[], "2026-10-19"), None);
        let reason = to_reason(&[&a, &b, &c], "2026-10-19");
        assert_eq!(
            reason,
            Some(Reason::KnownExploited {
                cves: vec!["CVE-2025-2222".into(), "CVE-2024-3333".into(), "CVE-2023-1111".into()],
                most_recent: "2025-06-01".into(),
                recent: true,
                ransomware: true,
            })
        );
        // Same day: numeric CVE order.
        let d = entry("CVE-2026-5281", "X", "Y", "2026-09-09", false);
        let e = entry("CVE-2026-11645", "X", "Y", "2026-09-09", false);
        match to_reason(&[&d, &e], "2026-10-19") {
            Some(Reason::KnownExploited { cves, .. }) => assert_eq!(cves, ["CVE-2026-11645", "CVE-2026-5281"]),
            other => panic!("unexpected {other:?}"),
        }
        let old = to_reason(&[&a, &c], "2026-10-19");
        assert!(matches!(old, Some(Reason::KnownExploited { recent: false, ransomware: false, .. })));

        let many: Vec<KevEntry> =
            (1..=8).map(|i| entry(&format!("CVE-2020-000{i}"), "X", "Y", &format!("2020-01-0{i}"), false)).collect();
        let refs: Vec<&KevEntry> = many.iter().collect();
        match to_reason(&refs, "2020-06-01") {
            Some(Reason::KnownExploited { cves, most_recent, recent, .. }) => {
                assert_eq!(cves.len(), 5);
                assert_eq!(cves[0], "CVE-2020-0008");
                assert_eq!(most_recent, "2020-01-08");
                assert!(recent);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn recent_window() {
        assert!(is_recent("2024-10-20", "2026-10-19"));
        assert!(!is_recent("2024-10-15", "2026-10-19"));
        assert!(!is_recent("garbage", "2026-10-19"));
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("majestic-kev-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const SAMPLE: &str = r#"{"title":"t","count":2,"vulnerabilities":[
        {"cveID":"CVE-2025-0411","vendorProject":"7-Zip","product":"7-Zip","vulnerabilityName":"7-Zip MotW Bypass",
         "dateAdded":"2025-02-06","knownRansomwareCampaignUse":"Unknown","notes":"x"},
        {"cveID":"CVE-2023-38831","vendorProject":"RARLAB","product":"WinRAR","vulnerabilityName":"WinRAR RCE",
         "dateAdded":"2023-08-24","knownRansomwareCampaignUse":"Known"},
        {"vendorProject":"no cve"}]}"#;

    #[test]
    fn cache_loading() {
        let dir = temp_dir("cache");
        // Nothing at all, offline: error.
        assert!(load(&dir, Duration::from_secs(3600), false).is_err());

        std::fs::write(dir.join(CACHE_FILE), SAMPLE).unwrap();
        let catalog = load(&dir, Duration::from_secs(3600), false).unwrap();
        assert_eq!(catalog.entries.len(), 2);
        assert_eq!(catalog.entries[1].cve, "CVE-2023-38831");
        assert!(catalog.entries[1].ransomware);
        assert!(!catalog.entries[0].ransomware);

        // Stale cache (max_age 0) and offline: still used.
        let stale = load(&dir, Duration::ZERO, false).unwrap();
        assert_eq!(stale.entries.len(), 2);

        // A corrupt cache is not a catalog.
        std::fs::write(dir.join(CACHE_FILE), "<html>oops</html>").unwrap();
        assert!(load(&dir, Duration::from_secs(3600), false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real catalog against real-world display names. Run with
    /// `cargo test -p majestic-core kev::tests::real_catalog -- --ignored --nocapture`.
    #[test]
    #[ignore = "network"]
    fn real_catalog() {
        // Kept between runs (one download a day at most).
        let dir = std::env::temp_dir().join("majestic-kev-real");
        let catalog = load(&dir, Duration::from_secs(86400), true).expect("download");
        println!("{} entries", catalog.entries.len());
        let programs: &[(&str, Option<&str>)] = &[
            ("Google Chrome", Some("Google LLC")),
            ("Mozilla Firefox (x64 fr)", Some("Mozilla")),
            ("Mozilla Thunderbird (x64 fr)", Some("Mozilla")),
            ("Microsoft Edge", Some("Microsoft Corporation")),
            ("Microsoft Edge WebView2 Runtime", Some("Microsoft Corporation")),
            ("Brave", Some("Brave Software Inc")),
            ("Opera Stable 112.0.5197.53", Some("Opera Software")),
            ("Vivaldi", Some("Vivaldi Technologies AS")),
            ("Chrome Remote Desktop Host", Some("Google LLC")),
            ("7-Zip 24.08 (x64)", Some("Igor Pavlov")),
            ("WinRAR 7.01 (64-bit)", Some("win.rar GmbH")),
            ("Adobe Acrobat Reader DC", Some("Adobe Systems Incorporated")),
            ("Adobe Acrobat (64-bit)", Some("Adobe")),
            ("Notepad++ (64-bit x64)", Some("Notepad++ Team")),
            ("VLC media player", Some("VideoLAN")),
            ("Zoom Workplace", Some("Zoom Communications, Inc.")),
            ("Microsoft Visual Studio Code", Some("Microsoft Corporation")),
            ("Git", Some("The Git Development Community")),
            ("GitHub Desktop", Some("GitHub, Inc.")),
            ("Audacity 3.7.1", Some("Audacity Team")),
            ("Python 3.13.5 (64-bit)", Some("Python Software Foundation")),
            ("Microsoft Teams", Some("Microsoft Corporation")),
            ("Microsoft OneDrive", Some("Microsoft Corporation")),
            ("Microsoft Office Professional Plus 2019 - fr-fr", Some("Microsoft Corporation")),
            ("Microsoft 365 Apps for enterprise - en-us", Some("Microsoft Corporation")),
            ("Microsoft Windows Desktop Runtime - 8.0.10 (x64)", Some("Microsoft Corporation")),
            ("Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.40.33810", Some("Microsoft Corporation")),
            ("Nextcloud", Some("Nextcloud GmbH")),
            ("KeePass Password Safe 2.57", Some("Dominik Reichl")),
            ("KeePassXC", Some("KeePassXC Team")),
            ("Syncthing", Some("Syncthing")),
            ("calibre 64bit", Some("Kovid Goyal")),
            ("LibreOffice 24.8.2.1", Some("The Document Foundation")),
            ("PuTTY release 0.81 (64-bit)", Some("Simon Tatham")),
            ("WinSCP 6.3.5", Some("Martin Prikryl")),
            ("FileZilla 3.67.1", Some("Tim Kosse")),
            ("Oracle VirtualBox 7.1.4", Some("Oracle Corporation")),
            ("Java 8 Update 401 (64-bit)", Some("Oracle Corporation")),
            ("Eclipse Temurin JDK with Hotspot 21.0.4+7 (x64)", Some("Eclipse Adoptium")),
            ("TeamViewer", Some("TeamViewer")),
            ("AnyDesk", Some("AnyDesk Software GmbH")),
            ("Steam", Some("Valve Corporation")),
            ("Discord", Some("Discord Inc.")),
            ("Spotify", Some("Spotify AB")),
            ("WhatsApp", Some("WhatsApp Inc.")),
            ("Docker Desktop", Some("Docker Inc.")),
            ("VMware Workstation", Some("VMware, Inc.")),
            ("Node.js", Some("Node.js Foundation")),
            ("OpenVPN 2.6.12-I001 amd64", Some("OpenVPN, Inc.")),
            ("WireGuard", Some("WireGuard LLC")),
            ("Bitwarden", Some("Bitwarden Inc.")),
            ("Dropbox", Some("Dropbox, Inc.")),
            ("Foxit PDF Reader", Some("Foxit Software Inc.")),
            ("SumatraPDF", Some("Krzysztof Kowalczyk")),
            ("GIMP 2.10.38", Some("The GIMP Team")),
            ("Inkscape", Some("Inkscape")),
            ("OBS Studio", Some("OBS Project")),
            ("Wireshark 4.4.1 x64", Some("The Wireshark developer community, https://www.wireshark.org")),
            ("Apache OpenOffice 4.1.15", Some("Apache Software Foundation")),
            ("WPS Office (12.2.0.18165)", Some("Kingsoft Corp.")),
            ("DAEMON Tools Lite", Some("Disc Soft Ltd")),
        ];
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        for (name, publisher) in programs {
            let hits = catalog.lookup(name, *publisher);
            if hits.is_empty() {
                println!("  -  {name}");
                continue;
            }
            let reason = to_reason(&hits, &today);
            println!("  +  {name}: {} hit(s), {reason:?}", hits.len());
            for hit in hits.iter().take(3) {
                println!("       {} | {} | {} | {}", hit.cve, hit.vendor, hit.product, hit.name);
            }
        }
        let started = std::time::Instant::now();
        for _ in 0..10 {
            for (name, publisher) in programs {
                let _ = catalog.lookup(name, *publisher);
            }
        }
        println!("lookup: {:?} per program", started.elapsed() / (10 * programs.len() as u32));
    }
}
