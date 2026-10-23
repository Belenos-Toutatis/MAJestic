//! Programs that update themselves and have a page that starts it (the
//! browsers): the way to update them when winget cannot, for instance
//! Microsoft Edge installed by Windows with its own setup.exe while the
//! winget manifest only has the enterprise MSI.

use std::path::{Path, PathBuf};

pub struct SelfUpdater {
    pub winget_ids: &'static [&'static str],
    /// Name shown on the button ("Update in Microsoft Edge").
    pub app: &'static str,
    pub exe: &'static str,
    /// Page that checks for and installs updates.
    pub page: &'static str,
    /// Folders of `exe` when the registry gives none (`%VAR%` expanded).
    pub default_dirs: &'static [&'static str],
}

pub const SELF_UPDATERS: &[SelfUpdater] = &[
    SelfUpdater {
        winget_ids: &["Microsoft.Edge"],
        app: "Microsoft Edge",
        exe: "msedge.exe",
        page: "edge://settings/help",
        default_dirs: &[r"%ProgramFiles(x86)%\Microsoft\Edge\Application", r"%ProgramFiles%\Microsoft\Edge\Application"],
    },
    SelfUpdater {
        winget_ids: &["Google.Chrome", "Google.Chrome.EXE"],
        app: "Google Chrome",
        exe: "chrome.exe",
        page: "chrome://settings/help",
        default_dirs: &[
            r"%ProgramFiles%\Google\Chrome\Application",
            r"%ProgramFiles(x86)%\Google\Chrome\Application",
            r"%LOCALAPPDATA%\Google\Chrome\Application",
        ],
    },
    SelfUpdater {
        winget_ids: &["Brave.Brave"],
        app: "Brave",
        exe: "brave.exe",
        page: "brave://settings/help",
        default_dirs: &[
            r"%ProgramFiles%\BraveSoftware\Brave-Browser\Application",
            r"%LOCALAPPDATA%\BraveSoftware\Brave-Browser\Application",
        ],
    },
];

pub fn for_id(id: &str) -> Option<&'static SelfUpdater> {
    SELF_UPDATERS.iter().find(|u| u.winget_ids.iter().any(|w| w.eq_ignore_ascii_case(id)))
}

fn expand(dir: &str) -> Option<PathBuf> {
    let mut out = String::new();
    let mut rest = dir;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        out.push_str(&std::env::var(&after[..end]).ok()?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(PathBuf::from(out))
}

/// The program's executable: in its install location, else in the usual folders.
pub fn exe_path(u: &SelfUpdater, install_location: Option<&str>) -> Option<PathBuf> {
    install_location
        .map(|l| Path::new(l.trim().trim_matches('"')).to_path_buf())
        .into_iter()
        .chain(u.default_dirs.iter().filter_map(|d| expand(d)))
        .map(|dir| dir.join(u.exe))
        .find(|p| p.is_file())
}

/// Opens the page that updates the program, with the user's normal rights.
pub fn open(u: &SelfUpdater, install_location: Option<&str>) -> anyhow::Result<()> {
    let exe = exe_path(u, install_location).ok_or_else(|| anyhow::anyhow!("{} not found", u.exe))?;
    crate::system::procs::launch_with_arg(&exe, u.page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_programs() {
        assert_eq!(for_id("microsoft.edge").map(|u| u.page), Some("edge://settings/help"));
        assert_eq!(for_id("Google.Chrome.EXE").map(|u| u.exe), Some("chrome.exe"));
        assert!(for_id("Mozilla.Firefox").is_none());
        let windir = std::env::var("SystemRoot").unwrap();
        assert_eq!(expand(r"%SystemRoot%\System32"), Some(PathBuf::from(format!(r"{windir}\System32"))));
        assert_eq!(expand(r"%MAJESTIC_NO_SUCH_VAR%\x"), None);
        assert_eq!(expand(r"C:\Plain"), Some(PathBuf::from(r"C:\Plain")));
    }
}

#[cfg(test)]
mod real {
    use super::*;

    /// Opens Microsoft Edge on its "About" page (starts its update). Run with
    /// `--ignored`.
    #[test]
    #[ignore = "opens Microsoft Edge"]
    fn open_edge_about_page() {
        let edge = for_id("Microsoft.Edge").unwrap();
        println!("{:?}", exe_path(edge, None));
        open(edge, None).unwrap();
    }
}
