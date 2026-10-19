//! Alternatives for programs that no package manager handles.
//!
//! OWNER: intel agent.

use crate::model::Alternative;

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
pub fn latest_elsewhere(input: &LookupInput) -> Option<Alternative> {
    let _ = input;
    todo!("lookup::latest_elsewhere")
}

/// `owner/repo` of the first github.com repository URL.
pub fn github_repo(urls: &[String]) -> Option<(String, String)> {
    let _ = urls;
    todo!("lookup::github_repo")
}

/// Web search URL to find the official download page.
pub fn web_search_url(name: &str) -> String {
    let _ = name;
    todo!("lookup::web_search_url")
}
