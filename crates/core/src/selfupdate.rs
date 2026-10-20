//! MAJestic updating itself from its GitHub releases.
//!
//! [`check`] reads the latest release from the GitHub API. [`download`]
//! fetches `MAJestic.exe` next to the running executable and keeps it only if
//! its SHA-256 matches the `MAJestic.exe.sha256` published with it and its
//! version is the announced one. [`swap`] then puts it in place of the
//! running file (Windows lets a running executable be renamed); the window
//! starts the new file with `--after-update` and exits, and the new process
//! removes the old file with [`cleanup`].

use std::cmp::Ordering;
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::intel::http;
use crate::system::fileinfo;
use crate::version;

const LATEST_URL: &str = "https://api.github.com/repos/Belenos-Toutatis/MAJestic/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/Belenos-Toutatis/MAJestic/releases";
/// Release pages and assets are only accepted from this repository.
const REPO_PREFIX: &str = "https://github.com/Belenos-Toutatis/MAJestic/";
const DOWNLOAD_PREFIX: &str = "https://github.com/Belenos-Toutatis/MAJestic/releases/download/";
const EXE_ASSET: &str = "MAJestic.exe";
const HASH_ASSET: &str = "MAJestic.exe.sha256";
const MAX_EXE_SIZE: u64 = 200 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_NOTES: usize = 4000;

/// A newer MAJestic, as shown by the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    /// The release page on GitHub.
    pub page_url: String,
    /// Release notes (Markdown), truncated.
    pub notes: String,
    /// Size of `MAJestic.exe` in bytes.
    pub size: Option<u64>,
    #[serde(skip)]
    exe_url: String,
    #[serde(skip)]
    hash_url: String,
}

/// Why an update failed (shown by the UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// Download failed or was interrupted.
    Network,
    /// The file does not match the published SHA-256 (or the hash file is unreadable).
    HashMismatch,
    /// The file is not the announced version.
    VersionMismatch,
    /// MAJestic's folder cannot be written (protected folder...).
    WriteDenied,
    /// The new file could not be put in place.
    SwapFailed,
}

#[derive(Debug)]
pub struct Failure {
    pub kind: FailureKind,
    pub detail: String,
}

impl Failure {
    fn new(kind: FailureKind, detail: impl fmt::Display) -> Self {
        Failure { kind, detail: detail.to_string() }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for Failure {}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

/// The latest release, when it is newer than `current`.
pub fn check(current: &str) -> anyhow::Result<Option<Release>> {
    let text = http::get_text(LATEST_URL)?;
    Ok(parse_release(&text)?.filter(|r| version::is_newer(&r.version, current)))
}

/// A usable release from the GitHub API answer: `None` for drafts,
/// pre-releases, and releases without both assets from this repository.
fn parse_release(json: &str) -> anyhow::Result<Option<Release>> {
    let api: ApiRelease = serde_json::from_str(json).context("decoding the GitHub release")?;
    if api.draft || api.prerelease {
        return Ok(None);
    }
    let version = api.tag_name.trim_start_matches(['v', 'V']).to_string();
    if version::Version::parse(&version).is_none() {
        return Ok(None);
    }
    let asset = |name: &str| {
        api.assets.iter().find(|a| a.name == name && a.browser_download_url.starts_with(DOWNLOAD_PREFIX))
    };
    let (Some(exe), Some(hash)) = (asset(EXE_ASSET), asset(HASH_ASSET)) else {
        return Ok(None);
    };
    let mut notes = whats_new(api.body.as_deref().unwrap_or_default());
    if notes.len() > MAX_NOTES {
        let mut end = MAX_NOTES;
        while !notes.is_char_boundary(end) {
            end -= 1;
        }
        notes.truncate(end);
        notes.push('…');
    }
    let page_url = if api.html_url.starts_with(REPO_PREFIX) { api.html_url.clone() } else { RELEASES_PAGE.to_string() };
    Ok(Some(Release {
        version,
        page_url,
        notes,
        size: exe.size,
        exe_url: exe.browser_download_url.clone(),
        hash_url: hash.browser_download_url.clone(),
    }))
}

/// The release-specific part of the notes: what comes before the download
/// instructions shared by every release (`.github/release-notes.md`).
fn whats_new(body: &str) -> String {
    let end = [DOWNLOAD_NOTES_MARKER, "**Téléchargez `MAJestic.exe`", "**Full Changelog**"]
        .iter()
        .filter_map(|m| body.find(m))
        .min()
        .unwrap_or(body.len());
    body[..end].trim().to_string()
}

/// First line of `.github/release-notes.md`.
const DOWNLOAD_NOTES_MARKER: &str = "<!-- download-notes -->";

/// `<exe>.new`: where the new version is downloaded (same folder, so that
/// the final rename stays on one volume).
pub fn staging_path(exe: &Path) -> PathBuf {
    with_suffix(exe, "new")
}

/// `<exe>.old`: the previous version, deleted by the next start.
pub fn old_path(exe: &Path) -> PathBuf {
    with_suffix(exe, "old")
}

fn with_suffix(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    exe.with_file_name(name)
}

/// Downloads the new executable to `dest` and checks it: SHA-256 equal to the
/// published one, version equal to the announced one. `progress(done, total)`
/// follows the download. On failure, `dest` is removed.
pub fn download(release: &Release, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), Failure> {
    let result = download_checked(release, dest, progress);
    if result.is_err() {
        let _ = fs::remove_file(dest);
    }
    result
}

fn download_checked(release: &Release, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), Failure> {
    use FailureKind::*;

    let hash_text = http::get_text(&release.hash_url).map_err(|e| Failure::new(Network, format!("{e:#}")))?;
    let expected = parse_hash(&hash_text).ok_or_else(|| Failure::new(HashMismatch, "unreadable .sha256 file"))?;

    let mut file = File::create(dest).map_err(|e| Failure::new(WriteDenied, format!("{}: {e}", dest.display())))?;
    let (mut reader, total) = http::get_stream(&release.exe_url, DOWNLOAD_TIMEOUT, MAX_EXE_SIZE)
        .map_err(|e| Failure::new(Network, format!("{e:#}")))?;
    let total = total.or(release.size);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    progress(0, total);
    loop {
        let n = reader.read(&mut buf).map_err(|e| Failure::new(Network, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| Failure::new(WriteDenied, e))?;
        done += n as u64;
        progress(done, total);
    }
    file.flush().and_then(|_| file.sync_all()).map_err(|e| Failure::new(WriteDenied, e))?;
    drop(file);

    let actual = hex(&hasher.finalize());
    if actual != expected {
        return Err(Failure::new(HashMismatch, format!("expected {expected}, got {actual}")));
    }
    // The announced version, not an older file published under a new tag.
    match fileinfo::file_version(dest) {
        Some(v) if version::compare(&v, &release.version) == Ordering::Equal => Ok(()),
        found => Err(Failure::new(VersionMismatch, format!("file version {found:?}, release {}", release.version))),
    }
}

/// First token of a `sha256sum`-style line: 64 hex digits, lowercased.
fn parse_hash(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?;
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())).then(|| token.to_ascii_lowercase())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Puts `new` in place of the running `exe`: `exe` → `<exe>.old`, then
/// `new` → `exe`. The first rename is undone if the second one fails.
pub fn swap(exe: &Path, new: &Path) -> Result<(), Failure> {
    let old = old_path(exe);
    if old.exists() {
        fs::remove_file(&old).map_err(|e| Failure::new(FailureKind::SwapFailed, format!("{}: {e}", old.display())))?;
    }
    fs::rename(exe, &old).map_err(|e| Failure::new(FailureKind::WriteDenied, format!("{}: {e}", exe.display())))?;
    if let Err(e) = fs::rename(new, exe) {
        let _ = fs::rename(&old, exe);
        return Err(Failure::new(FailureKind::SwapFailed, format!("{}: {e}", new.display())));
    }
    Ok(())
}

/// Removes `<exe>.old` and a leftover `<exe>.new`, retrying for a few seconds
/// while the previous process exits.
pub fn cleanup(exe: &Path) {
    for path in [old_path(exe), staging_path(exe)] {
        for _ in 0..20 {
            if !path.exists() || fs::remove_file(&path).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        if path.exists() {
            log::warn!("cannot remove {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_json(tag: &str, exe_url: &str, draft: bool) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/Belenos-Toutatis/MAJestic/releases/tag/{tag}"),
            "body": "## Nouveautés\n- Something",
            "draft": draft,
            "prerelease": false,
            "assets": [
                { "name": "MAJestic.exe", "size": 6574592, "browser_download_url": exe_url },
                { "name": "MAJestic.exe.sha256", "size": 81,
                  "browser_download_url": format!("https://github.com/Belenos-Toutatis/MAJestic/releases/download/{tag}/MAJestic.exe.sha256") }
            ]
        })
        .to_string()
    }

    const EXE: &str = "https://github.com/Belenos-Toutatis/MAJestic/releases/download/v0.2.0/MAJestic.exe";

    #[test]
    fn release_parsing() {
        let r = parse_release(&api_json("v0.2.0", EXE, false)).unwrap().unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.size, Some(6574592));
        assert_eq!(r.exe_url, EXE);
        assert!(r.hash_url.ends_with("/v0.2.0/MAJestic.exe.sha256"));
        assert!(r.page_url.ends_with("/tag/v0.2.0"));
        assert_eq!(r.notes, "## Nouveautés\n- Something");
        assert_eq!(whats_new("A\n\n<!-- download-notes -->\n**Téléchargez..."), "A");
        assert_eq!(whats_new("A\n**Téléchargez `MAJestic.exe` ci-dessous"), "A");
        assert_eq!(whats_new("**Full Changelog**: x"), "");
        // Drafts and assets from elsewhere are refused.
        assert_eq!(parse_release(&api_json("v0.2.0", EXE, true)).unwrap(), None);
        assert_eq!(parse_release(&api_json("v0.2.0", "https://evil.example/MAJestic.exe", false)).unwrap(), None);
        assert_eq!(parse_release(&api_json("nightly", EXE, false)).unwrap(), None);
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn hash_file_parsing() {
        let h = "9B9E7305BC5F2D9176DFF8263BA146FC7B0D65CE1FB499271FD7746EC53D66ED";
        assert_eq!(parse_hash(&format!("{h}  MAJestic.exe\r\n")), Some(h.to_ascii_lowercase()));
        assert_eq!(parse_hash(&h.to_ascii_lowercase()), Some(h.to_ascii_lowercase()));
        assert_eq!(parse_hash("abc  MAJestic.exe"), None);
        assert_eq!(parse_hash(""), None);
    }

    #[test]
    fn swap_and_cleanup() {
        let dir = std::env::temp_dir().join(format!("majestic-swap-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("MAJestic.exe");
        fs::write(&exe, b"old").unwrap();
        fs::write(staging_path(&exe), b"new").unwrap();
        swap(&exe, &staging_path(&exe)).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new");
        assert_eq!(fs::read(old_path(&exe)).unwrap(), b"old");
        assert!(!staging_path(&exe).exists());
        // A failed second rename puts the old file back.
        assert!(swap(&exe, &dir.join("missing")).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"new");
        cleanup(&exe);
        assert!(!old_path(&exe).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "network"]
    fn latest_release_on_github() {
        let r = check("0.0.1").unwrap().expect("a release");
        println!("{r:?}");
        assert!(r.exe_url.starts_with(DOWNLOAD_PREFIX));
    }
}
