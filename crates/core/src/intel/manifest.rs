//! Package metadata from the winget community repository (winget-pkgs on
//! GitHub): release notes, links, publisher. Locale-independent, unlike
//! `winget show`.
//!
//! OWNER: intel agent.

use std::path::Path;

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
    pub tags: Vec<String>,
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
}

/// Installers of `id` at `version` (`<id>.installer.yaml`), cached like
/// [`fetch`].
pub fn fetch_installers(id: &str, version: &str, cache_dir: &Path) -> anyhow::Result<Vec<InstallerInfo>> {
    let _ = (id, version, cache_dir);
    todo!("manifest::fetch_installers")
}

/// The installer winget would most likely pick on this PC: native
/// architecture first (x64 on x64 Windows, then neutral, then x86), matching
/// `scope` when given (`user`/`machine`), first one otherwise.
pub fn pick_installer<'a>(installers: &'a [InstallerInfo], scope: Option<crate::model::Scope>) -> Option<&'a InstallerInfo> {
    let _ = (installers, scope);
    todo!("manifest::pick_installer")
}

/// Fetches (and caches under `cache_dir/manifests/`) the default-locale
/// manifest of `id` at `version`:
/// `manifests/<first letter>/<id segments as folders>/<version>/<id>.yaml`
/// for `DefaultLocale`, then `<id>.locale.<DefaultLocale>.yaml`.
pub fn fetch(id: &str, version: &str, cache_dir: &Path) -> anyhow::Result<ManifestMeta> {
    let _ = (id, version, cache_dir);
    todo!("manifest::fetch")
}
