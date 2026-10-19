//! Priority and default selection of an update.
//!
//! OWNER: intel agent.

use std::sync::LazyLock;

use regex::Regex;

use super::exposure::exposure;
use crate::model::{Assessment, ItemKind, Priority, Reason, UpdateItem, VersionCertainty};
use crate::version;

const MAX_KEYWORDS: usize = 6;

/// Security vocabulary, English and French. Longer phrases first so they win
/// over the single words they contain.
static SECURITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?xi)
        \b(?:
            CVE-\d{4}-\d{4,}
          | correctifs?\ de\ s[ée]curit[ée]
          | remote[\ -]code[\ -]execution
          | arbitrary[\ -]code
          | privileges?[\ -]escalation
          | (?:elevation|escalation)\ of\ privileges?
          | cross[\ -]site[\ -]scripting
          | sandbox[\ -]escape
          | buffer[\ -](?:overflow|overrun)
          | heap[\ -](?:buffer[\ -])?overflow
          | use[\ -]after[\ -]free
          | vulnerabilit(?:y|ies)
          | vulnerable
          | vuln[ée]rabilit[ée]s?
          | security
          | s[ée]curit[ée]
          | failles?
          | exploit(?:s|ed|able|ation)?
          | RCE
          | XSS
        )\b",
    )
    .expect("valid regex")
});

/// `security key(s)` (FIDO hardware keys) and `Windows Security` (the app)
/// are not about vulnerabilities.
static SECURITY_KEY_AFTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s+(?:keys?|tokens?)\b").expect("valid regex"));
static SECURITY_APP_BEFORE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:\bwindows|\bclés?\s+de|\bcles?\s+de)\s+$").expect("valid regex"));

/// Security-related keywords found in release notes (English and French:
/// `security`, `vulnerability`, `CVE-xxxx-xxxx`, `exploit`, `sécurité`,
/// `faille`, `remote code execution`, ...). Returns the distinct matches as
/// found (CVE ids verbatim), at most 6.
pub fn security_keywords(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for m in SECURITY.find_iter(text) {
        let word = m.as_str();
        let is_security_word = {
            let lower = word.to_lowercase();
            lower == "security" || lower == "sécurité" || lower == "securite"
        };
        if is_security_word
            && (SECURITY_KEY_AFTER.is_match(&text[m.end()..]) || SECURITY_APP_BEFORE.is_match(&text[..m.start()]))
        {
            continue;
        }
        let word = word.split_whitespace().collect::<Vec<_>>().join(" ");
        let lower = word.to_lowercase();
        if !found.iter().any(|f| f.to_lowercase() == lower) {
            found.push(word);
            if found.len() == MAX_KEYWORDS {
                break;
            }
        }
    }
    found
}

/// Computes the assessment of an item whose base fields are filled (kind,
/// versions, certainty, explicit_only, pinned, running, category).
///
/// * Critical: `kev` reason with `recent`, or security keywords in the notes.
/// * Major: major version jump (`version::is_major_jump`).
/// * Recommended: exposed software family, or old (non-recent) KEV history.
/// * Uncertain: installed version unknown/approximate (unless read from file).
/// * Normal otherwise.
///
/// `preselect` is true for Critical/Recommended/Normal automatic updates, false
/// for Major, Uncertain, Manual items, ignored or pinned items.
/// Reasons list everything that applies (also informative ones like
/// `AppRunning`, `RequiresExplicit`, `Pinned`, `VersionFromFile`).
///
/// Precedence is Critical > Major > Uncertain > Recommended > Normal: an
/// exposed program whose installed version is unknown is Uncertain (with the
/// `Exposed` reason), so that it is never preselected for a blind reinstall.
/// A Critical update of an `Unknown` installed version is not preselected
/// either. Up-to-date items keep their informative reasons but stay Normal;
/// unmanaged programs (no update known) only get KEV/exposure priorities.
pub fn assess(item: &UpdateItem, kev: Option<Reason>, release_notes: Option<&str>) -> Assessment {
    let has_update = matches!(item.kind, ItemKind::Update | ItemKind::Manual);
    let mut reasons: Vec<Reason> = Vec::new();

    let mut kev_recent = false;
    let mut kev_any = false;
    if let Some(reason) = kev {
        if let Reason::KnownExploited { recent, .. } = &reason {
            kev_recent = *recent;
            kev_any = true;
        }
        reasons.push(reason);
    }

    let keywords = if has_update { release_notes.map(security_keywords).unwrap_or_default() } else { Vec::new() };
    let security_fix = !keywords.is_empty();
    if security_fix {
        reasons.push(Reason::SecurityFix { keywords });
    }

    let installed = item.installed_version.as_deref().map(|v| version::strip_approx(v).0);
    let version_unknown =
        item.version_certainty == VersionCertainty::Unknown || installed.as_deref().is_none_or(version::is_unknown);

    let major = match (has_update, installed.as_deref(), item.available_version.as_deref()) {
        (true, Some(from), Some(to)) if !version_unknown && version::is_major_jump(from, to) => {
            reasons.push(Reason::MajorUpgrade { from: from.to_string(), to: to.trim().to_string() });
            true
        }
        _ => false,
    };

    let exposed = exposure(&item.name, &item.id);
    if let Some(category) = exposed {
        reasons.push(Reason::Exposed { category });
    }

    let uncertain = if version_unknown {
        reasons.push(Reason::UnknownVersion);
        true
    } else {
        match item.version_certainty {
            VersionCertainty::Approximate => {
                reasons.push(Reason::ApproximateVersion);
                true
            }
            VersionCertainty::FromFile => {
                reasons.push(Reason::VersionFromFile { version: installed.clone().unwrap_or_default() });
                false
            }
            _ => false,
        }
    };

    if item.explicit_only {
        reasons.push(Reason::RequiresExplicit);
    }
    if item.pinned {
        reasons.push(Reason::Pinned);
    }
    let mut processes: Vec<String> = Vec::new();
    for process in &item.running {
        if !processes.iter().any(|p| p.eq_ignore_ascii_case(&process.name)) {
            processes.push(process.name.clone());
        }
    }
    if !processes.is_empty() {
        reasons.push(Reason::AppRunning { processes });
    }
    // Reasons computed elsewhere (scan integration) are kept.
    reasons.extend(item.assessment.reasons.iter().filter(|r| matches!(r, Reason::DuplicateManager { .. })).cloned());

    let priority = match item.kind {
        ItemKind::UpToDate => Priority::Normal,
        ItemKind::Unmanaged => {
            if kev_recent {
                Priority::Critical
            } else if kev_any || exposed.is_some() {
                Priority::Recommended
            } else {
                Priority::Normal
            }
        }
        ItemKind::Update | ItemKind::Manual => {
            if kev_recent || security_fix {
                Priority::Critical
            } else if major {
                Priority::Major
            } else if uncertain {
                Priority::Uncertain
            } else if kev_any || exposed.is_some() {
                Priority::Recommended
            } else {
                Priority::Normal
            }
        }
    };

    let preselect = item.kind == ItemKind::Update
        && !item.ignored
        && !item.pinned
        && matches!(priority, Priority::Critical | Priority::Recommended | Priority::Normal)
        && !version_unknown;

    Assessment { priority, preselect, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExposureCategory, RunningProcess, Source};

    fn item(name: &str, installed: &str, available: &str) -> UpdateItem {
        let mut item = UpdateItem::new(format!("winget:{name}"), Source::Winget, name.into(), name.into());
        item.kind = ItemKind::Update;
        item.installed_version = Some(installed.into());
        item.available_version = Some(available.into());
        item
    }

    fn kev(recent: bool) -> Option<Reason> {
        Some(Reason::KnownExploited {
            cves: vec!["CVE-2025-0411".into()],
            most_recent: if recent { "2025-02-06" } else { "2021-11-03" }.into(),
            recent,
            ransomware: false,
        })
    }

    fn has(assessment: &Assessment, pred: impl Fn(&Reason) -> bool) -> bool {
        assessment.reasons.iter().any(pred)
    }

    #[test]
    fn keywords_english_and_french() {
        assert_eq!(
            security_keywords("Fixes CVE-2024-12345 and cve-2023-0001; a Security issue (RCE) was fixed."),
            ["CVE-2024-12345", "cve-2023-0001", "Security", "RCE"]
        );
        assert_eq!(
            security_keywords("Correctif de sécurité. Corrige une faille et deux vulnérabilités."),
            ["Correctif de sécurité", "faille", "vulnérabilités"]
        );
        assert_eq!(
            security_keywords("Heap overflow, use-after-free and remote code execution; XSS; privilege escalation"),
            ["Heap overflow", "use-after-free", "remote code execution", "XSS", "privilege escalation"]
        );
        assert_eq!(security_keywords("Actively exploited vulnerability"), ["exploited", "vulnerability"]);
        assert_eq!(security_keywords("buffer overflow"), ["buffer overflow"]);
        // Distinct, case-insensitively; at most 6.
        assert_eq!(security_keywords("security SECURITY Security"), ["security"]);
        let many = "CVE-2024-0001 CVE-2024-0002 CVE-2024-0003 CVE-2024-0004 CVE-2024-0005 CVE-2024-0006 CVE-2024-0007";
        assert_eq!(security_keywords(many).len(), 6);
    }

    #[test]
    fn keywords_avoid_false_matches() {
        for text in [
            "Improved cybersecurity dashboard",
            "Unexploited potential, insecurely named variable, xssfoo, force",
            "Added support for security keys (FIDO2)",
            "Integration with Windows Security",
            "Prise en charge des clés de sécurité",
            "Bug fixes and performance improvements",
            "CVE list viewer",
        ] {
            assert!(security_keywords(text).is_empty(), "{text}: {:?}", security_keywords(text));
        }
        // Still found elsewhere in the same text.
        assert_eq!(security_keywords("Security keys supported. Fixed a security bug."), ["security"]);
    }

    #[test]
    fn critical_from_recent_kev() {
        let a = assess(&item("7-Zip", "24.08", "24.09"), kev(true), None);
        assert_eq!(a.priority, Priority::Critical);
        assert!(a.preselect);
        assert!(has(&a, |r| matches!(r, Reason::KnownExploited { recent: true, .. })));
        assert!(has(&a, |r| matches!(r, Reason::Exposed { category: ExposureCategory::Archiver })));
    }

    #[test]
    fn critical_from_release_notes() {
        let a = assess(&item("Some Tool", "1.2", "1.3"), None, Some("This release fixes a vulnerability."));
        assert_eq!(a.priority, Priority::Critical);
        assert!(a.preselect);
        assert!(has(&a, |r| *r == Reason::SecurityFix { keywords: vec!["vulnerability".into()] }));
    }

    #[test]
    fn critical_and_major_keeps_major_reason() {
        let a = assess(&item("Some Tool", "3.9", "4.0"), kev(true), None);
        assert_eq!(a.priority, Priority::Critical);
        assert!(a.preselect);
        assert!(has(&a, |r| *r == Reason::MajorUpgrade { from: "3.9".into(), to: "4.0".into() }));
    }

    #[test]
    fn major_upgrade() {
        let a = assess(&item("Some Tool", "3.9.1", "4.0.0"), None, Some("New UI"));
        assert_eq!(a.priority, Priority::Major);
        assert!(!a.preselect);
        // Major beats exposure.
        let b = assess(&item("VLC media player", "2.2.8", "3.0.21"), None, None);
        assert_eq!(b.priority, Priority::Major);
        assert!(has(&b, |r| matches!(r, Reason::Exposed { .. })));
        // Calendar versions and browsers are not major jumps.
        assert_eq!(assess(&item("Some Tool", "2025.4", "2026.1"), None, None).priority, Priority::Normal);
    }

    #[test]
    fn recommended() {
        let a = assess(&item("Mozilla Firefox (x64 fr)", "130.0", "131.0"), None, None);
        assert_eq!(a.priority, Priority::Recommended);
        assert!(a.preselect);
        let b = assess(&item("TeamViewer Desktop thing", "15.1", "15.2"), kev(false), None);
        assert_eq!(b.priority, Priority::Recommended);
        assert!(has(&b, |r| matches!(r, Reason::KnownExploited { recent: false, .. })));
    }

    #[test]
    fn normal() {
        let a = assess(&item("Audacity", "3.7.1", "3.7.3"), None, Some("Bug fixes."));
        assert_eq!(a.priority, Priority::Normal);
        assert!(a.preselect);
        assert!(a.reasons.is_empty());
    }

    #[test]
    fn uncertain_versions() {
        let mut unknown = item("Some Tool", "Unknown", "2.0");
        unknown.version_certainty = VersionCertainty::Unknown;
        let a = assess(&unknown, None, None);
        assert_eq!(a.priority, Priority::Uncertain);
        assert!(!a.preselect);
        assert!(has(&a, |r| *r == Reason::UnknownVersion));

        // Exposed but unknown: still uncertain, never preselected.
        let mut browser = item("Google Chrome", "", "130.0");
        browser.installed_version = None;
        let b = assess(&browser, None, None);
        assert_eq!(b.priority, Priority::Uncertain);
        assert!(!b.preselect);
        assert!(has(&b, |r| matches!(r, Reason::Exposed { category: ExposureCategory::Browser })));

        let mut approx = item("Some Tool", "< 1.5", "1.6");
        approx.version_certainty = VersionCertainty::Approximate;
        let c = assess(&approx, None, None);
        assert_eq!(c.priority, Priority::Uncertain);
        assert!(!c.preselect);
        assert!(has(&c, |r| *r == Reason::ApproximateVersion));

        // Version read from the executable: trusted.
        let mut from_file = item("Some Tool", "1.5.2", "1.6");
        from_file.version_certainty = VersionCertainty::FromFile;
        let d = assess(&from_file, None, None);
        assert_eq!(d.priority, Priority::Normal);
        assert!(d.preselect);
        assert!(has(&d, |r| *r == Reason::VersionFromFile { version: "1.5.2".into() }));

        // Critical wins over uncertainty, but an unknown version is not preselected.
        let e = assess(&unknown, kev(true), None);
        assert_eq!(e.priority, Priority::Critical);
        assert!(!e.preselect);
        let f = assess(&approx, kev(true), None);
        assert_eq!(f.priority, Priority::Critical);
        assert!(f.preselect);
    }

    #[test]
    fn manual_pinned_ignored_and_informative_reasons() {
        let mut manual = item("Some Tool", "1.0", "1.1");
        manual.kind = ItemKind::Manual;
        let a = assess(&manual, kev(true), None);
        assert_eq!(a.priority, Priority::Critical);
        assert!(!a.preselect);

        let mut pinned = item("Some Tool", "1.0", "1.1");
        pinned.pinned = true;
        pinned.explicit_only = true;
        pinned.running = vec![
            RunningProcess { pid: 1, name: "tool.exe".into(), exe: "C:\\t\\tool.exe".into(), has_window: true },
            RunningProcess { pid: 2, name: "TOOL.exe".into(), exe: "C:\\t\\tool.exe".into(), has_window: false },
            RunningProcess { pid: 3, name: "helper.exe".into(), exe: "C:\\t\\helper.exe".into(), has_window: false },
        ];
        pinned.assessment.reasons = vec![Reason::DuplicateManager { other: Source::Chocolatey }, Reason::Pinned];
        let b = assess(&pinned, None, None);
        assert_eq!(b.priority, Priority::Normal);
        assert!(!b.preselect);
        assert_eq!(
            b.reasons,
            vec![
                Reason::RequiresExplicit,
                Reason::Pinned,
                Reason::AppRunning { processes: vec!["tool.exe".into(), "helper.exe".into()] },
                Reason::DuplicateManager { other: Source::Chocolatey },
            ]
        );

        let mut ignored = item("Some Tool", "1.0", "1.1");
        ignored.ignored = true;
        assert!(!assess(&ignored, None, None).preselect);
    }

    #[test]
    fn other_kinds() {
        let mut up_to_date = item("Google Chrome", "130.0", "130.0");
        up_to_date.kind = ItemKind::UpToDate;
        up_to_date.available_version = None;
        let a = assess(&up_to_date, kev(true), Some("security fix"));
        assert_eq!(a.priority, Priority::Normal);
        assert!(!a.preselect);
        assert!(has(&a, |r| matches!(r, Reason::KnownExploited { .. })));
        assert!(!has(&a, |r| matches!(r, Reason::SecurityFix { .. })));

        let mut unmanaged = item("WinRAR 6.02 (64-bit)", "6.02", "");
        unmanaged.kind = ItemKind::Unmanaged;
        unmanaged.available_version = None;
        let b = assess(&unmanaged, kev(true), None);
        assert_eq!(b.priority, Priority::Critical);
        assert!(!b.preselect);
        assert_eq!(assess(&unmanaged, None, None).priority, Priority::Recommended);
    }
}
