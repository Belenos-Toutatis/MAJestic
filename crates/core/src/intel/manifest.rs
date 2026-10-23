//! Package metadata from the winget community repository (winget-pkgs on
//! GitHub): release notes, links, publisher. Locale-independent, unlike
//! `winget show`.
//!
//! OWNER: intel agent.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{bail, Context};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use super::http;
use crate::model::Scope;

const RAW_BASE: &str = "https://raw.githubusercontent.com/microsoft/winget-pkgs/master/manifests";
const MAX_RELEASE_NOTES: usize = 2000;
const MAX_DESCRIPTION: usize = 1200;
const DEFAULT_LOCALE: &str = "en-US";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestMeta {
    pub publisher: Option<String>,
    pub publisher_url: Option<String>,
    pub package_url: Option<String>,
    pub support_url: Option<String>,
    pub release_notes: Option<String>,
    pub release_notes_url: Option<String>,
    pub license: Option<String>,
    pub short_description: Option<String>,
    /// Longer description, plain text.
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// What a package does, in one locale of its manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct About {
    /// `en-US`, `fr-FR`…
    pub locale: String,
    pub summary: Option<String>,
    pub description: Option<String>,
}

/// [`About`] of `id` at `version`: the `prefer` locale (`fr-FR`) when the
/// manifest has it with a description, otherwise the default locale.
pub fn fetch_about(id: &str, version: &str, cache_dir: &Path, prefer: Option<&str>) -> anyhow::Result<About> {
    let version_manifest = fetch_file(id, version, &format!("{id}.yaml"), cache_dir)?;
    if is_singleton(&version_manifest) {
        let locale: LocaleManifest = parse_yaml(&version_manifest)?;
        return Ok(locale.into_about(DEFAULT_LOCALE));
    }
    let header: VersionManifest = parse_yaml(&version_manifest)?;
    let default = header.default_locale.filter(|l| is_safe_segment(l)).unwrap_or_else(|| DEFAULT_LOCALE.to_string());
    if let Some(prefer) = prefer.filter(|p| is_safe_segment(p) && !p.eq_ignore_ascii_case(&default)) {
        if let Some(text) = fetch_optional_file(id, version, &format!("{id}.locale.{prefer}.yaml"), cache_dir)? {
            let about = parse_yaml::<LocaleManifest>(&text)?.into_about(prefer);
            if about.summary.is_some() || about.description.is_some() {
                return Ok(about);
            }
        }
    }
    let text = fetch_file(id, version, &format!("{id}.locale.{default}.yaml"), cache_dir)?;
    Ok(parse_yaml::<LocaleManifest>(&text)?.into_about(&default))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallerInfo {
    pub url: String,
    /// `msi`, `wix`, `inno`, `nullsoft`, `exe`, `burn`, `msix`, `appx`,
    /// `zip`, `portable` (the root-level `InstallerType` applies when the
    /// installer entry has none).
    pub installer_type: Option<String>,
    /// `x64`, `x86`, `arm64`, `neutral`.
    pub architecture: Option<String>,
    /// `user`, `machine`, or `None` when unspecified.
    pub scope: Option<String>,
    /// `InstallerLocale` (`en-US`…), the root-level one when the installer
    /// entry has none.
    pub locale: Option<String>,
    /// `NestedInstallerType` of a `zip` installer.
    pub nested_type: Option<String>,
    /// `InstallerType` of its `AppsAndFeaturesEntries` (the installations it
    /// can upgrade, besides its own type).
    pub arp_types: Vec<String>,
}

/// winget's installer families (`GetCompatibilitySet`): a program installed
/// by one family can only be upgraded by the same family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Technology {
    /// `exe`, `inno`, `nullsoft`, `burn`: what winget assumes for every
    /// registry entry without `WindowsInstaller`.
    Exe,
    /// `msi`, `wix`.
    Msi,
}

fn technology(installer_type: &str) -> Option<Technology> {
    match installer_type.to_ascii_lowercase().as_str() {
        "exe" | "inno" | "nullsoft" | "burn" => Some(Technology::Exe),
        "msi" | "wix" => Some(Technology::Msi),
        _ => None,
    }
}

impl InstallerInfo {
    /// The type winget compares: the nested one for a `zip`.
    pub fn effective_type(&self) -> Option<&str> {
        match self.installer_type.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("zip") => self.nested_type.as_deref(),
            other => other,
        }
    }
}

/// `true` when winget will refuse every installer of the manifest for a
/// program installed by `installed` (`UPDATE_INSTALL_TECHNOLOGY_MISMATCH`):
/// each installer, and each of its `AppsAndFeaturesEntries`, belongs to the
/// other family. Cautious: any installer of another kind (MSIX, portable,
/// unknown) or no installer at all means "cannot tell" (`false`).
pub fn technology_mismatch(installers: &[InstallerInfo], installed: Technology) -> bool {
    !installers.is_empty()
        && installers.iter().all(|i| {
            let types = i.effective_type().into_iter().chain(i.arp_types.iter().map(String::as_str));
            let families: Vec<Option<Technology>> = types.map(technology).collect();
            !families.is_empty() && families.iter().all(|f| matches!(f, Some(t) if *t != installed))
        })
}

/// Installers of `id` at `version` (`<id>.installer.yaml`), cached like
/// [`fetch`].
pub fn fetch_installers(id: &str, version: &str, cache_dir: &Path) -> anyhow::Result<Vec<InstallerInfo>> {
    let version_manifest = fetch_file(id, version, &format!("{id}.yaml"), cache_dir)?;
    let text = if is_singleton(&version_manifest) {
        version_manifest
    } else {
        fetch_file(id, version, &format!("{id}.installer.yaml"), cache_dir)?
    };
    let manifest: InstallerManifest = parse_yaml(&text)?;
    Ok(manifest.into_installers())
}

/// The installer winget would most likely pick on this PC: native
/// architecture first (x64 on x64 Windows, then neutral, then x86), matching
/// `scope` when given (`user`/`machine`), first one otherwise.
pub fn pick_installer(installers: &[InstallerInfo], scope: Option<crate::model::Scope>) -> Option<&InstallerInfo> {
    pick_for_arch(installers, scope, std::env::consts::ARCH)
}

/// The language of every installer, when they all declare the same one:
/// winget has no other language to choose from. `None` when one declares
/// none (winget then accepts it whatever the language) or when they differ.
pub fn single_locale(installers: &[InstallerInfo]) -> Option<&str> {
    let first = installers.first()?.locale.as_deref()?;
    installers
        .iter()
        .all(|i| i.locale.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(first)))
        .then_some(first)
}

/// A broken manifest: the installer announced for `available` names the
/// `installed` version in its URL and not `available` (it happened with
/// Microsoft.VisualStudioCode 1.142.0, which shipped the 1.141.2 setup).
/// Installing it changes nothing, so the update comes back after every run.
/// Returns the version the installer really carries.
pub fn stale_installer(url: &str, installed: &str, available: &str) -> Option<String> {
    use crate::version;
    use std::cmp::Ordering;

    if !version::is_newer(available, installed) {
        return None;
    }
    // The announced version anywhere in the URL (query included) clears it;
    // the installed one must be in the path to count.
    if version::versions_in_text(url).iter().any(|t| version::compare(t, available) == Ordering::Equal) {
        return None;
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    version::versions_in_text(path).into_iter().find(|t| version::compare(t, installed) == Ordering::Equal)
}

/// Fetches (and caches under `cache_dir/manifests/`) the default-locale
/// manifest of `id` at `version`:
/// `manifests/<first letter>/<id segments as folders>/<version>/<id>.yaml`
/// for `DefaultLocale`, then `<id>.locale.<DefaultLocale>.yaml`.
pub fn fetch(id: &str, version: &str, cache_dir: &Path) -> anyhow::Result<ManifestMeta> {
    let version_manifest = fetch_file(id, version, &format!("{id}.yaml"), cache_dir)?;
    if is_singleton(&version_manifest) {
        let locale: LocaleManifest = parse_yaml(&version_manifest)?;
        return Ok(locale.into_meta());
    }
    let header: VersionManifest = parse_yaml(&version_manifest)?;
    let locale = header.default_locale.filter(|l| is_safe_segment(l)).unwrap_or_else(|| DEFAULT_LOCALE.to_string());
    let text = fetch_file(id, version, &format!("{id}.locale.{locale}.yaml"), cache_dir)?;
    let manifest: LocaleManifest = parse_yaml(&text)?;
    Ok(manifest.into_meta())
}

// ---------------------------------------------------------------------------
// Download and cache
// ---------------------------------------------------------------------------

/// `<first char lowercase>/<id segments>/<version>` (URL-encoded segments).
fn manifest_dir(id: &str, version: &str) -> anyhow::Result<String> {
    if !is_safe_segment(id) || !is_safe_segment(version) || id.split('.').any(str::is_empty) {
        bail!("invalid package id or version: {id:?} {version:?}");
    }
    let first = id.chars().next().map(|c| c.to_lowercase().to_string()).unwrap_or_default();
    let mut parts = vec![encode_segment(&first)];
    parts.extend(id.split('.').map(encode_segment));
    parts.push(encode_segment(version));
    Ok(parts.join("/"))
}

/// Non-empty, no path separator, no `..`, no control character.
fn is_safe_segment(s: &str) -> bool {
    !s.trim().is_empty() && s != "." && s != ".." && !s.contains(['/', '\\']) && !s.chars().any(char::is_control)
}

fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// File-system safe name (Windows forbids `<>:"/\|?*` and trailing dots).
fn fs_name(s: &str) -> String {
    let cleaned: String =
        s.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "_".to_string()
    } else {
        trimmed.to_string()
    }
}

fn cache_path(cache_dir: &Path, id: &str, version: &str, file: &str) -> PathBuf {
    cache_dir.join("manifests").join(fs_name(id)).join(fs_name(version)).join(fs_name(file))
}

/// One manifest file, from the cache (published manifests are immutable) or
/// from GitHub.
fn fetch_file(id: &str, version: &str, file: &str, cache_dir: &Path) -> anyhow::Result<String> {
    let dir = manifest_dir(id, version)?;
    let path = cache_path(cache_dir, id, version, file);
    if let Ok(text) = std::fs::read_to_string(&path) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    let url = format!("{RAW_BASE}/{dir}/{}", encode_segment(file));
    let text = http::get_text(&url)?;
    if let Err(err) = write_atomically(&path, &text) {
        log::debug!("cannot cache {}: {err:#}", path.display());
    }
    Ok(text)
}

/// [`fetch_file`] for a file that may not exist (a translation): `None` when
/// GitHub answers 404, remembered with an empty `.absent` file since
/// published manifests never change.
fn fetch_optional_file(id: &str, version: &str, file: &str, cache_dir: &Path) -> anyhow::Result<Option<String>> {
    let absent = cache_path(cache_dir, id, version, &format!("{file}.absent"));
    if absent.exists() {
        return Ok(None);
    }
    match fetch_file(id, version, file, cache_dir) {
        Ok(text) => Ok(Some(text)),
        Err(err) if http::error_status(&err) == Some(404) => {
            if let Err(e) = write_atomically(&absent, "") {
                log::debug!("cannot cache {}: {e:#}", absent.display());
            }
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

fn write_atomically(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Parsing (lenient: every field optional, unknown fields ignored)
// ---------------------------------------------------------------------------

fn parse_yaml<T: DeserializeOwned>(text: &str) -> anyhow::Result<T> {
    serde_yaml_ng::from_str(text.trim_start_matches('\u{feff}')).context("invalid manifest YAML")
}

/// Any scalar as a trimmed string; anything else (or empty) is `None`.
fn scalar_to_string(value: serde_yaml_ng::Value) -> Option<String> {
    use serde_yaml_ng::Value;
    let s = match value {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Tagged(tagged) => return scalar_to_string(tagged.value),
        _ => return None,
    };
    let trimmed = s.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn lenient_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(scalar_to_string(serde_yaml_ng::Value::deserialize(deserializer)?))
}

fn lenient_strings<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Ok(match serde_yaml_ng::Value::deserialize(deserializer)? {
        serde_yaml_ng::Value::Sequence(items) => items.into_iter().filter_map(scalar_to_string).collect(),
        other => scalar_to_string(other).into_iter().collect(),
    })
}

/// [`lenient_seq`] that tells an absent field (`None`) from an empty list.
fn lenient_seq_opt<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Option<Vec<T>>, D::Error> {
    lenient_seq(deserializer).map(Some)
}

/// A sequence of structs, skipping malformed elements.
fn lenient_seq<'de, D: Deserializer<'de>, T: DeserializeOwned>(deserializer: D) -> Result<Vec<T>, D::Error> {
    Ok(match serde_yaml_ng::Value::deserialize(deserializer)? {
        serde_yaml_ng::Value::Sequence(items) => {
            items.into_iter().filter_map(|item| serde_yaml_ng::from_value(item).ok()).collect()
        }
        _ => Vec::new(),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct VersionManifest {
    #[serde(deserialize_with = "lenient_string")]
    default_locale: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    manifest_type: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    publisher: Option<String>,
}

fn is_singleton(version_manifest: &str) -> bool {
    match parse_yaml::<VersionManifest>(version_manifest) {
        Ok(m) => match m.manifest_type {
            Some(t) => t.eq_ignore_ascii_case("singleton"),
            // Very old manifests have no type: a publisher means everything is here.
            None => m.publisher.is_some(),
        },
        Err(_) => false,
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct LocaleManifest {
    #[serde(deserialize_with = "lenient_string")]
    publisher: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    publisher_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    package_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    publisher_support_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    license: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    short_description: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    description: Option<String>,
    #[serde(deserialize_with = "lenient_strings")]
    tags: Vec<String>,
    #[serde(deserialize_with = "lenient_string")]
    release_notes: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    release_notes_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    package_locale: Option<String>,
}

impl LocaleManifest {
    /// `locale`: the one asked for, unless the manifest names its own.
    fn into_about(self, locale: &str) -> About {
        let summary = self.short_description.map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "));
        let description = self
            .description
            .as_deref()
            .and_then(|d| clean_text(d, MAX_DESCRIPTION))
            .filter(|d| summary.as_deref() != Some(d.as_str()));
        About {
            locale: self.package_locale.filter(|l| is_safe_segment(l)).unwrap_or_else(|| locale.to_string()),
            summary,
            description,
        }
    }

    fn into_meta(self) -> ManifestMeta {
        ManifestMeta {
            publisher: self.publisher,
            publisher_url: self.publisher_url.filter(|u| is_web_url(u)),
            package_url: self.package_url.filter(|u| is_web_url(u)),
            support_url: self.publisher_support_url.filter(|u| is_web_url(u)),
            release_notes: self.release_notes.as_deref().and_then(clean_release_notes),
            release_notes_url: self.release_notes_url.filter(|u| is_web_url(u)),
            license: self.license,
            short_description: self.short_description,
            description: self.description.as_deref().and_then(|d| clean_text(d, MAX_DESCRIPTION)),
            tags: self.tags,
        }
    }
}

fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct InstallerManifest {
    #[serde(deserialize_with = "lenient_string")]
    installer_type: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    scope: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    installer_locale: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    nested_installer_type: Option<String>,
    #[serde(deserialize_with = "lenient_seq")]
    apps_and_features_entries: Vec<ArpEntry>,
    #[serde(deserialize_with = "lenient_seq")]
    installers: Vec<InstallerEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct ArpEntry {
    #[serde(deserialize_with = "lenient_string")]
    installer_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct InstallerEntry {
    #[serde(deserialize_with = "lenient_string")]
    architecture: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    installer_type: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    scope: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    installer_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    installer_locale: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    nested_installer_type: Option<String>,
    /// `None` when absent: the root-level entries apply.
    #[serde(deserialize_with = "lenient_seq_opt")]
    apps_and_features_entries: Option<Vec<ArpEntry>>,
}

impl InstallerManifest {
    fn into_installers(self) -> Vec<InstallerInfo> {
        let lower = |s: Option<String>| s.map(|s| s.to_ascii_lowercase());
        let root_type = lower(self.installer_type);
        let root_scope = lower(self.scope);
        let root_locale = self.installer_locale;
        let root_nested = lower(self.nested_installer_type);
        let arp_types =
            |entries: &[ArpEntry]| entries.iter().filter_map(|e| lower(e.installer_type.clone())).collect::<Vec<_>>();
        let root_arp = arp_types(&self.apps_and_features_entries);
        self.installers
            .into_iter()
            .filter_map(|entry| {
                let url = entry.installer_url.filter(|u| is_web_url(u))?;
                Some(InstallerInfo {
                    url,
                    installer_type: lower(entry.installer_type).or_else(|| root_type.clone()),
                    architecture: lower(entry.architecture),
                    scope: lower(entry.scope).or_else(|| root_scope.clone()),
                    locale: entry.installer_locale.or_else(|| root_locale.clone()),
                    nested_type: lower(entry.nested_installer_type).or_else(|| root_nested.clone()),
                    arp_types: entry
                        .apps_and_features_entries
                        .as_deref()
                        .map(arp_types)
                        .unwrap_or_else(|| root_arp.clone()),
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Installer choice
// ---------------------------------------------------------------------------

/// Architectures runnable on `host_arch` (`std::env::consts::ARCH`), most
/// preferred first.
fn arch_preference(host_arch: &str) -> &'static [&'static str] {
    match host_arch {
        "aarch64" => &["arm64", "x64", "neutral", "x86"],
        "x86" => &["x86", "neutral"],
        _ => &["x64", "neutral", "x86"],
    }
}

fn pick_for_arch<'a>(
    installers: &'a [InstallerInfo],
    scope: Option<Scope>,
    host_arch: &str,
) -> Option<&'a InstallerInfo> {
    let preference = arch_preference(host_arch);
    let wanted_scope = scope.map(|s| match s {
        Scope::User => "user",
        Scope::Machine => "machine",
    });
    installers
        .iter()
        .enumerate()
        .filter_map(|(index, installer)| {
            let arch_rank = match installer.architecture.as_deref() {
                // Not declared: assume it runs anywhere, like `neutral`.
                None => preference.iter().position(|a| *a == "neutral")?,
                Some(arch) => preference.iter().position(|a| a.eq_ignore_ascii_case(arch))?,
            };
            let scope_rank = match (wanted_scope, installer.scope.as_deref()) {
                (None, _) => 0,
                (Some(w), Some(s)) if s.eq_ignore_ascii_case(w) => 0,
                (Some(_), None) => 1,
                (Some(_), Some(_)) => 2,
            };
            Some(((arch_rank, scope_rank, index), installer))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, installer)| installer)
}

// ---------------------------------------------------------------------------
// Release notes
// ---------------------------------------------------------------------------

static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)</?(?:p|br|li|ul|ol|div|h[1-6]|a|b|i|u|strong|em|span|code|pre|table|tr|td)\b[^>]*>")
        .expect("valid regex")
});
static BLOCK_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>|</(?:p|li|div|h[1-6]|tr|ul|ol|pre)>").expect("valid regex"));
static LIST_ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<li\b[^>]*>").expect("valid regex"));
static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"</?[a-zA-Z][^>]*>").expect("valid regex"));

fn strip_html(text: &str) -> String {
    let text = BLOCK_END.replace_all(text, "\n");
    let text = LIST_ITEM.replace_all(&text, "- ");
    let text = ANY_TAG.replace_all(&text, "");
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Plain text, trimmed, blank lines collapsed, at most 2000 characters.
fn clean_release_notes(raw: &str) -> Option<String> {
    clean_text(raw, MAX_RELEASE_NOTES)
}

/// Plain text, trimmed, blank lines collapsed, at most `max` characters.
pub(crate) fn clean_text(raw: &str, max: usize) -> Option<String> {
    let mut text = raw.replace("\r\n", "\n").replace('\r', "\n");
    if HTML_TAG.is_match(&text) {
        text = strip_html(&text);
    }
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    let text = lines.join("\n").trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(truncate_chars(&text, max))
}

/// At most `max` characters, cut at a word boundary when possible, with `…`.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    let boundary = cut.rfind(char::is_whitespace).filter(|&pos| cut[pos..].chars().count() < 200).unwrap_or(cut.len());
    format!("{}…", cut[..boundary].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_installer_detection() {
        let vscode = "https://vscode.download.prss.microsoft.com/dbazure/download/stable/\
                      16507d91be0dc1ce4f94c04d4d0430210b8685d9/VSCodeUserSetup-x64-1.141.2.exe";
        assert_eq!(stale_installer(vscode, "1.141.2", "1.142.0").as_deref(), Some("1.141.2"));
        // Right installer, or a URL without the installed version: fine.
        assert_eq!(stale_installer(vscode, "1.141.2", "1.141.2"), None);
        assert_eq!(stale_installer(vscode, "1.140.0", "1.141.2"), None);
        assert_eq!(stale_installer("https://example.com/app-1.0.exe?v=1.1", "1.0", "1.1"), None);
        assert_eq!(stale_installer("https://example.com/v1.0/app-1.1.exe", "1.0", "1.1"), None);
        assert_eq!(stale_installer("https://example.com/download/latest", "1.0", "1.1"), None);
        // 1.0 == 1.0.0 for winget.
        assert_eq!(stale_installer("https://example.com/app-1.0.0.msi", "1.0", "1.2").as_deref(), Some("1.0.0"));
    }

    fn inst(arch: Option<&str>, scope: Option<&str>, url: &str) -> InstallerInfo {
        InstallerInfo {
            url: url.into(),
            installer_type: Some("exe".into()),
            architecture: arch.map(str::to_string),
            scope: scope.map(str::to_string),
            locale: None,
            nested_type: None,
            arp_types: Vec::new(),
        }
    }

    #[test]
    fn paths_and_urls() {
        assert_eq!(
            manifest_dir("Microsoft.VisualStudioCode", "1.95.3").unwrap(),
            "m/Microsoft/VisualStudioCode/1.95.3"
        );
        assert_eq!(manifest_dir("7zip.7zip", "24.09").unwrap(), "7/7zip/7zip/24.09");
        assert_eq!(manifest_dir("Python.Python.3.13", "3.13.5").unwrap(), "p/Python/Python/3/13/3.13.5");
        assert_eq!(manifest_dir("Notepad++.Notepad++", "8.7").unwrap(), "n/Notepad%2B%2B/Notepad%2B%2B/8.7");
        assert!(manifest_dir("../evil", "1.0").is_err());
        assert!(manifest_dir("A.B", "..").is_err());
        assert!(manifest_dir("A..B", "1").is_err());
        assert!(manifest_dir("", "1").is_err());
        assert_eq!(fs_name("a<b>:c?."), "a_b__c_");
    }

    const LOCALE: &str = r#"# Created using wingetcreate
# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.1.12.0.schema.json

PackageIdentifier: 7zip.7zip
PackageVersion: "26.04"
PackageLocale: en-US
Publisher: Igor Pavlov
PublisherUrl: https://7-zip.org/
PublisherSupportUrl: https://7-zip.org/support.html
PackageName: 7-Zip
PackageUrl: https://7-zip.org/download.html
License: LGPL-2.1
ShortDescription: A file archiver with a high compression ratio.
Tags:
- archive
- 7z
- 2024
ReleaseNotes: |-
  - Some bugs and vulnerabilities were fixed.


  - Faster.
ReleaseNotesUrl: https://github.com/ip7z/7zip/releases/tag/26.04
Documentations:
- DocumentLabel: FAQ
  DocumentUrl: https://7-zip.org/faq.html
ManifestType: defaultLocale
ManifestVersion: 1.12.0
"#;

    #[test]
    fn locale_manifest_parsing() {
        let meta = parse_yaml::<LocaleManifest>(LOCALE).unwrap().into_meta();
        assert_eq!(meta.publisher.as_deref(), Some("Igor Pavlov"));
        assert_eq!(meta.publisher_url.as_deref(), Some("https://7-zip.org/"));
        assert_eq!(meta.package_url.as_deref(), Some("https://7-zip.org/download.html"));
        assert_eq!(meta.support_url.as_deref(), Some("https://7-zip.org/support.html"));
        assert_eq!(meta.license.as_deref(), Some("LGPL-2.1"));
        assert_eq!(meta.tags, ["archive", "7z", "2024"]);
        assert_eq!(meta.release_notes.as_deref(), Some("- Some bugs and vulnerabilities were fixed.\n\n- Faster."));
        assert_eq!(meta.release_notes_url.as_deref(), Some("https://github.com/ip7z/7zip/releases/tag/26.04"));
    }

    const INSTALLER: &str = r#"PackageIdentifier: Example.App
PackageVersion: 1.2.3
InstallerType: inno
Scope: machine
FileExtensions:
- "001"
ReleaseDate: 2026-10-06
Installers:
- Architecture: x86
  InstallerUrl: https://example.com/app-x86.exe
  InstallerSha256: 00
- Architecture: x64
  InstallerType: msi
  InstallerUrl: https://example.com/app-x64.msi
- Architecture: x64
  Scope: user
  InstallerUrl: https://example.com/app-x64-user.exe
- Architecture: arm64
  InstallerUrl: https://example.com/app-arm64.exe
- Architecture: x64
  InstallerUrl: 42
- just a string
ManifestType: installer
ManifestVersion: 1.6.0
"#;

    #[test]
    fn installer_manifest_parsing() {
        let installers = parse_yaml::<InstallerManifest>(INSTALLER).unwrap().into_installers();
        assert_eq!(installers.len(), 4);
        assert_eq!(installers[0].installer_type.as_deref(), Some("inno"));
        assert_eq!(installers[0].scope.as_deref(), Some("machine"));
        assert_eq!(installers[1].installer_type.as_deref(), Some("msi"));
        assert_eq!(installers[2].scope.as_deref(), Some("user"));
        assert_eq!(installers[3].architecture.as_deref(), Some("arm64"));
    }

    #[test]
    fn installation_technologies() {
        // Microsoft.Edge 156: enterprise MSI only, Windows installs Edge with setup.exe.
        let edge = r#"InstallerType: msi
Scope: machine
AppsAndFeaturesEntries:
- UpgradeCode: '{883C2625-37F7-357F-A0F4-DFAF391B2B9C}'
Installers:
  - Architecture: x64
    InstallerUrl: https://download.microsoft.com/MicrosoftEdgeEnterpriseX64.msi
  - Architecture: x86
    InstallerUrl: https://download.microsoft.com/MicrosoftEdgeEnterpriseX86.msi
"#;
        let installers = parse_yaml::<InstallerManifest>(edge).unwrap().into_installers();
        assert!(installers[0].arp_types.is_empty());
        assert!(technology_mismatch(&installers, Technology::Exe));
        assert!(!technology_mismatch(&installers, Technology::Msi));

        // An AppsAndFeaturesEntries type lets the MSI upgrade an exe install.
        let declared = edge.replace("- UpgradeCode: '{883C2625-37F7-357F-A0F4-DFAF391B2B9C}'", "- InstallerType: exe");
        let installers = parse_yaml::<InstallerManifest>(&declared).unwrap().into_installers();
        assert_eq!(installers[1].arp_types, ["exe"]);
        assert!(!technology_mismatch(&installers, Technology::Exe));

        // One compatible installer is enough; inno and exe are one family.
        let mixed = r#"Installers:
- InstallerType: msi
  InstallerUrl: https://example.com/a.msi
- InstallerType: inno
  InstallerUrl: https://example.com/a.exe
"#;
        let installers = parse_yaml::<InstallerManifest>(mixed).unwrap().into_installers();
        assert!(!technology_mismatch(&installers, Technology::Exe));
        assert!(!technology_mismatch(&installers, Technology::Msi));

        // zip: its nested type counts; portable, msix or unknown: cannot tell.
        let zip = r#"InstallerType: zip
NestedInstallerType: wix
Installers:
- InstallerUrl: https://example.com/a.zip
"#;
        let installers = parse_yaml::<InstallerManifest>(zip).unwrap().into_installers();
        assert_eq!(installers[0].effective_type(), Some("wix"));
        assert!(technology_mismatch(&installers, Technology::Exe));
        for other in ["portable", "msix", "appx"] {
            let one = InstallerInfo { installer_type: Some(other.into()), ..inst(None, None, "u") };
            assert!(!technology_mismatch(&[one], Technology::Exe), "{other}");
        }
        let unknown = InstallerInfo { installer_type: None, ..inst(None, None, "u") };
        assert!(!technology_mismatch(&[unknown], Technology::Msi));
        assert!(!technology_mismatch(&[], Technology::Msi));
    }

    #[test]
    fn installer_languages() {
        // Kieirra.Murmure 2.0.3: one English installer, declared at the root.
        let murmure = r#"PackageIdentifier: Kieirra.Murmure
PackageVersion: 2.0.3
InstallerLocale: en-US
InstallerType: wix
Scope: machine
Installers:
- Architecture: x64
  InstallerUrl: https://github.com/Kieirra/murmure/releases/download/2.0.3/Murmure_x64.msi
"#;
        let installers = parse_yaml::<InstallerManifest>(murmure).unwrap().into_installers();
        assert_eq!(installers[0].locale.as_deref(), Some("en-US"));
        assert_eq!(single_locale(&installers), Some("en-US"));

        // An entry's own language wins over the root one.
        let two = r#"InstallerLocale: en-US
Installers:
- InstallerUrl: https://example.com/en.exe
- InstallerUrl: https://example.com/fr.exe
  InstallerLocale: fr-FR
"#;
        let installers = parse_yaml::<InstallerManifest>(two).unwrap().into_installers();
        assert_eq!(installers[1].locale.as_deref(), Some("fr-FR"));
        assert_eq!(single_locale(&installers), None);

        let lang = |l: Option<&str>| InstallerInfo { locale: l.map(str::to_string), ..inst(Some("x64"), None, "u") };
        assert_eq!(single_locale(&[lang(Some("en-US")), lang(Some("EN-us"))]), Some("en-US"));
        assert_eq!(single_locale(&[lang(Some("en-US")), lang(None)]), None);
        assert_eq!(single_locale(&[lang(None)]), None);
        assert_eq!(single_locale(&[]), None);
    }

    #[test]
    fn singleton_manifest() {
        let singleton = r#"PackageIdentifier: Old.App
PackageVersion: 1.0
PackageName: Old App
Publisher: Old Corp
License: MIT
ShortDescription: Old.
ReleaseNotes: <p>Security fix</p><ul><li>CVE-2024-1234 fixed</li><li>Faster &amp; smaller</li></ul>
Installers:
- Architecture: neutral
  InstallerType: zip
  InstallerUrl: https://example.com/app.zip
ManifestType: singleton
ManifestVersion: 1.0.0
"#;
        assert!(is_singleton(singleton));
        assert!(!is_singleton("PackageIdentifier: A.B\nDefaultLocale: en-US\nManifestType: version\n"));
        let meta = parse_yaml::<LocaleManifest>(singleton).unwrap().into_meta();
        assert_eq!(meta.publisher.as_deref(), Some("Old Corp"));
        assert_eq!(meta.release_notes.as_deref(), Some("Security fix\n- CVE-2024-1234 fixed\n- Faster & smaller"));
        let installers = parse_yaml::<InstallerManifest>(singleton).unwrap().into_installers();
        assert_eq!(installers.len(), 1);
        assert_eq!(installers[0].installer_type.as_deref(), Some("zip"));
    }

    #[test]
    fn cached_fetch_works_offline() {
        let dir = std::env::temp_dir().join(format!("majestic-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (id, version) = ("Example.App", "1.2.3");
        let put = |file: &str, text: &str| write_atomically(&cache_path(&dir, id, version, file), text).unwrap();
        put(
            "Example.App.yaml",
            "PackageIdentifier: Example.App\nPackageVersion: 1.2.3\nDefaultLocale: fr-FR\nManifestType: version\n",
        );
        put("Example.App.locale.fr-FR.yaml", LOCALE);
        put("Example.App.installer.yaml", INSTALLER);
        let meta = fetch(id, version, &dir).unwrap();
        assert_eq!(meta.publisher.as_deref(), Some("Igor Pavlov"));
        let installers = fetch_installers(id, version, &dir).unwrap();
        assert_eq!(installers.len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installer_choice() {
        let all = vec![
            inst(Some("x86"), Some("machine"), "x86-machine"),
            inst(Some("x64"), Some("machine"), "x64-machine"),
            inst(Some("x64"), Some("user"), "x64-user"),
            inst(Some("arm64"), None, "arm64"),
            inst(Some("neutral"), None, "neutral"),
        ];
        assert_eq!(pick_for_arch(&all, None, "x86_64").unwrap().url, "x64-machine");
        assert_eq!(pick_for_arch(&all, Some(Scope::User), "x86_64").unwrap().url, "x64-user");
        assert_eq!(pick_for_arch(&all, Some(Scope::Machine), "x86_64").unwrap().url, "x64-machine");
        assert_eq!(pick_for_arch(&all, None, "aarch64").unwrap().url, "arm64");
        assert_eq!(pick_for_arch(&all, None, "x86").unwrap().url, "x86-machine");

        let no_x64 = vec![inst(Some("x86"), None, "x86"), inst(Some("neutral"), None, "neutral")];
        assert_eq!(pick_for_arch(&no_x64, None, "x86_64").unwrap().url, "neutral");
        let only_x86 = vec![inst(Some("arm64"), None, "arm64"), inst(Some("x86"), Some("user"), "x86")];
        assert_eq!(pick_for_arch(&only_x86, Some(Scope::Machine), "x86_64").unwrap().url, "x86");
        // Scope without a match: unspecified scope beats the other scope.
        let scopes = vec![inst(Some("x64"), Some("user"), "user"), inst(Some("x64"), None, "any")];
        assert_eq!(pick_for_arch(&scopes, Some(Scope::Machine), "x86_64").unwrap().url, "any");
        assert!(pick_for_arch(&[inst(Some("arm64"), None, "arm")], None, "x86_64").is_none());
        assert!(pick_installer(&[], None).is_none());
        assert_eq!(pick_installer(&all, None).map(|i| i.url.as_str()), Some("x64-machine"));
    }

    #[test]
    fn release_notes_cleaning() {
        assert_eq!(clean_release_notes("  \n\n "), None);
        assert_eq!(clean_release_notes("Line 1\r\n\r\n\r\n\r\nLine 2  \n").as_deref(), Some("Line 1\n\nLine 2"));
        assert_eq!(clean_release_notes("a < b and c > d").as_deref(), Some("a < b and c > d"));
        let long = "word ".repeat(1000);
        let cleaned = clean_release_notes(&long).unwrap();
        assert!(cleaned.chars().count() <= MAX_RELEASE_NOTES);
        assert!(cleaned.ends_with("word…"));
        let unbroken = "é".repeat(3000);
        assert_eq!(clean_release_notes(&unbroken).unwrap().chars().count(), MAX_RELEASE_NOTES);
    }

    /// Real packages; finds the latest version through the GitHub contents
    /// API (one request per package). Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "network"]
    fn real_packages() {
        #[derive(Deserialize)]
        struct Entry {
            name: String,
            #[serde(rename = "type")]
            kind: String,
        }
        let cache = std::env::temp_dir().join("majestic-manifest-real");
        for id in [
            "Microsoft.VisualStudioCode", "7zip.7zip", "Audacity.Audacity", "Python.Python.3.13", "Notepad++.Notepad++",
        ] {
            let dir = manifest_dir(id, "x").unwrap();
            let dir = dir.trim_end_matches("/x");
            let url = format!("https://api.github.com/repos/microsoft/winget-pkgs/contents/manifests/{dir}");
            let entries: Vec<Entry> = http::get_json(&url).expect("GitHub listing");
            let version = entries
                .into_iter()
                .filter(|e| e.kind == "dir" && crate::version::Version::parse(&e.name).is_some())
                .map(|e| e.name)
                .max_by(|a, b| crate::version::compare(a, b))
                .expect("a version");
            let meta = fetch(id, &version, &cache).expect("meta");
            let installers = fetch_installers(id, &version, &cache).expect("installers");
            let picked = pick_installer(&installers, None).expect("an installer");
            let size = http::content_length(&picked.url);
            println!("{id} {version}");
            println!("  publisher={:?} license={:?}", meta.publisher, meta.license);
            println!("  package_url={:?} notes_url={:?}", meta.package_url, meta.release_notes_url);
            let notes = meta.release_notes.as_deref().unwrap_or("");
            println!("  notes ({} chars): {:?}", notes.chars().count(), notes.chars().take(160).collect::<String>());
            println!(
                "  {} installer(s); picked {:?} {:?} {:?}",
                installers.len(),
                picked.architecture,
                picked.scope,
                picked.installer_type
            );
            println!("  {} -> {:?} bytes", picked.url, size);
            assert!(meta.publisher.is_some());
            assert!(size.is_some_and(|s| s > 100_000), "size of {}", picked.url);
        }
    }
}
