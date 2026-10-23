//! Scoop's public catalogs (the Main and Extras buckets on GitHub): over
//! 4,000 Windows programs whose version a bot keeps up to date from each
//! publisher's site. Used for programs no package manager handles (Prince,
//! ImageMagick…), whether or not Scoop is installed.
//!
//! The list of manifest names is downloaded once a day (one GitHub request
//! per bucket, carrying no program name) and matched here; only the manifest
//! of a recognized program is then read.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::http;

/// `(bucket, GitHub repository)`, most reliable first.
const BUCKETS: &[(&str, &str)] = &[("Main", "ScoopInstaller/Main"), ("Extras", "ScoopInstaller/Extras")];
const INDEX_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const MANIFEST_MAX_AGE: Duration = Duration::from_secs(12 * 3600);
const TIMEOUT: Duration = Duration::from_secs(15);

/// Manifest names per bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub buckets: Vec<(String, Vec<String>)>,
}

impl Index {
    /// The bucket holding `name` (lowercase), most reliable bucket first.
    pub fn bucket_of(&self, name: &str) -> Option<&str> {
        self.buckets.iter().find(|(_, names)| names.binary_search_by(|n| n.as_str().cmp(name)).is_ok()).map(|(b, _)| b.as_str())
    }
}

/// What a manifest says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub bucket: String,
    pub name: String,
    pub version: String,
    pub homepage: Option<String>,
    pub description: Option<String>,
    /// The page the bot reads the version from (`checkver.url`), usually the
    /// publisher's download page.
    pub checkver_url: Option<String>,
}

impl Manifest {
    /// The page to send the user to: the download page when known.
    pub fn page(&self) -> Option<&str> {
        self.checkver_url.as_deref().filter(|u| u.starts_with("https://") || u.starts_with("http://")).or(self.homepage.as_deref())
    }
}

fn index_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("scoop").join("index.json")
}

fn fresh(path: &Path, max_age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < max_age)
}

fn write(path: &Path, text: &str) {
    let written = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(path, text));
    if let Err(e) = written {
        log::debug!("cannot cache {}: {e}", path.display());
    }
}

/// The names of every manifest, from the cache when less than a day old.
/// A stale cache is better than nothing when GitHub does not answer.
pub fn index(cache_dir: &Path) -> anyhow::Result<Index> {
    let path = index_path(cache_dir);
    let cached = || std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Index>(&b).ok());
    if fresh(&path, INDEX_MAX_AGE) {
        if let Some(index) = cached() {
            return Ok(index);
        }
    }
    match download_index() {
        Ok(index) => {
            write(&path, &serde_json::to_string(&index)?);
            Ok(index)
        }
        Err(e) => cached().ok_or(e),
    }
}

#[derive(Deserialize)]
struct Tree {
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
}

fn download_index() -> anyhow::Result<Index> {
    let mut buckets = Vec::new();
    for (bucket, repo) in BUCKETS {
        let url = format!("https://api.github.com/repos/{repo}/git/trees/master?recursive=1");
        let tree: Tree = serde_json::from_str(&http::get_text_within(&url, TIMEOUT)?)?;
        buckets.push((bucket.to_string(), names_of(&tree)));
    }
    Ok(Index { buckets })
}

/// `bucket/<name>.json` → `name`, lowercase, sorted.
fn names_of(tree: &Tree) -> Vec<String> {
    let mut names: Vec<String> = tree
        .tree
        .iter()
        .filter_map(|e| e.path.strip_prefix("bucket/")?.strip_suffix(".json").map(str::to_lowercase))
        .filter(|n| !n.contains('/'))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[derive(Deserialize)]
struct RawManifest {
    version: Option<String>,
    homepage: Option<String>,
    #[serde(default)]
    description: Option<serde_json::Value>,
    #[serde(default)]
    checkver: Option<serde_json::Value>,
}

/// Reads one manifest (cached 12 hours).
pub fn manifest(bucket: &str, name: &str, cache_dir: &Path) -> anyhow::Result<Manifest> {
    let repo = BUCKETS.iter().find(|(b, _)| *b == bucket).map(|(_, r)| *r).ok_or_else(|| anyhow::anyhow!("unknown bucket {bucket}"))?;
    let safe = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'));
    if !safe || name.starts_with('.') {
        anyhow::bail!("invalid manifest name {name:?}");
    }
    let path = cache_dir.join("scoop").join(bucket).join(format!("{name}.json"));
    let text = match std::fs::read_to_string(&path).ok().filter(|_| fresh(&path, MANIFEST_MAX_AGE)) {
        Some(text) => text,
        None => {
            let url = format!("https://raw.githubusercontent.com/{repo}/master/bucket/{name}.json");
            let text = http::get_text_within(&url, TIMEOUT)?;
            write(&path, &text);
            text
        }
    };
    parse_manifest(bucket, name, &text)
}

fn parse_manifest(bucket: &str, name: &str, text: &str) -> anyhow::Result<Manifest> {
    let raw: RawManifest = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    let version = raw.version.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).ok_or_else(|| anyhow::anyhow!("no version"))?;
    // `description` is a string or a list of lines.
    let description = match raw.description {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Array(lines)) => Some(lines.iter().filter_map(|l| l.as_str()).collect::<Vec<_>>().join(" ")),
        _ => None,
    }
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    // `checkver` is "github", a regex, or an object with a `url`.
    let checkver_url = raw.checkver.as_ref().and_then(|c| c.get("url")).and_then(|u| u.as_str()).map(str::to_string);
    Ok(Manifest {
        bucket: bucket.to_string(),
        name: name.to_string(),
        version,
        homepage: raw.homepage.filter(|h| h.starts_with("https://") || h.starts_with("http://")),
        description,
        checkver_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_manifests() {
        let tree = Tree {
            tree: ["bucket", "bucket/prince.json", "bucket/ImageMagick.json", "scripts/x.ps1", "bucket/a/b.json"]
                .iter()
                .map(|p| TreeEntry { path: p.to_string() })
                .collect(),
        };
        assert_eq!(names_of(&tree), ["imagemagick", "prince"]);
        let index = Index {
            buckets: vec![("Main".into(), vec!["prince".into()]), ("Extras".into(), vec!["filezilla".into(), "prince".into()])],
        };
        assert_eq!(index.bucket_of("prince"), Some("Main"));
        assert_eq!(index.bucket_of("filezilla"), Some("Extras"));
        assert_eq!(index.bucket_of("nothing"), None);

        let prince = r#"{
            "version": "17",
            "description": "Convert HTML to PDF with CSS",
            "homepage": "https://www.princexml.com",
            "checkver": { "url": "https://www.princexml.com/download/", "regex": ">Prince ([\\d.]+)<" }
        }"#;
        let m = parse_manifest("Main", "prince", prince).unwrap();
        assert_eq!(m.version, "17");
        assert_eq!(m.page(), Some("https://www.princexml.com/download/"));
        assert_eq!(m.description.as_deref(), Some("Convert HTML to PDF with CSS"));
        let github = r#"{ "version": "1.5.0", "homepage": "https://github.com/rustdesk/rustdesk",
                          "description": ["Remote desktop", "software"], "checkver": "github" }"#;
        let m = parse_manifest("Extras", "rustdesk", github).unwrap();
        assert_eq!(m.page(), Some("https://github.com/rustdesk/rustdesk"));
        assert_eq!(m.description.as_deref(), Some("Remote desktop software"));
        assert!(parse_manifest("Main", "x", "{}").is_err());
        assert!(manifest("Main", "../evil", Path::new(".")).is_err());
    }

    /// The real buckets. Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "network"]
    fn real_buckets() {
        let cache = std::env::temp_dir().join("majestic-scoop-real");
        let index = index(&cache).expect("index");
        for (bucket, names) in &index.buckets {
            println!("{bucket}: {} manifests", names.len());
        }
        let bucket = index.bucket_of("prince").expect("prince is in a bucket");
        let m = manifest(bucket, "prince", &cache).expect("manifest");
        println!("{m:?}");
    }
}
