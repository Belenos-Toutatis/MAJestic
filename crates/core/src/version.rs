//! Version parsing and comparison, following winget's rules.
//!
//! A version is split on `.`; each part is a leading integer followed by an
//! optional suffix (`1.2.3-beta` → `[1, 2, "3-beta"]`). Parts compare by integer
//! first, then a part *without* suffix is greater than one with a suffix
//! (pre-release), then suffixes compare case-insensitively. Trailing zero parts
//! are ignored, so `1.0` == `1.0.0`.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Part {
    number: u64,
    suffix: String,
}

impl Part {
    fn parse(s: &str) -> Part {
        let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
        let number = digits.parse::<u64>().unwrap_or(0);
        let suffix = s[digits.len()..].trim().to_lowercase();
        Part { number, suffix }
    }

    fn is_zero(&self) -> bool {
        self.number == 0 && self.suffix.is_empty()
    }
}

impl Ord for Part {
    fn cmp(&self, other: &Self) -> Ordering {
        self.number.cmp(&other.number).then_with(|| {
            match (self.suffix.is_empty(), other.suffix.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.suffix.cmp(&other.suffix),
            }
        })
    }
}

impl PartialOrd for Part {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A parsed, comparable version.
#[derive(Debug, Clone)]
pub struct Version {
    raw: String,
    parts: Vec<Part>,
}

impl Version {
    /// Parses a version string. Leading `v`/`V` and surrounding spaces are
    /// ignored. Returns `None` for empty strings and for values that are not
    /// versions at all (`Unknown`, `latest`, ...).
    pub fn parse(s: &str) -> Option<Version> {
        let raw = s.trim();
        let body = raw.strip_prefix(['v', 'V']).unwrap_or(raw);
        if body.is_empty() || !body.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut parts: Vec<Part> = body.split('.').map(Part::parse).collect();
        while parts.len() > 1 && parts.last().is_some_and(Part::is_zero) {
            parts.pop();
        }
        Some(Version { raw: raw.to_string(), parts })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// First numeric component.
    pub fn major(&self) -> u64 {
        self.parts.first().map(|p| p.number).unwrap_or(0)
    }

    /// Second numeric component (0 when absent).
    pub fn minor(&self) -> u64 {
        self.parts.get(1).map(|p| p.number).unwrap_or(0)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let zero = Part { number: 0, suffix: String::new() };
        let len = self.parts.len().max(other.parts.len());
        for i in 0..len {
            let a = self.parts.get(i).unwrap_or(&zero);
            let b = other.parts.get(i).unwrap_or(&zero);
            match a.cmp(b) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares two version strings. Unparsable versions sort before parsable
/// ones; two unparsable versions compare as equal.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (Version::parse(a), Version::parse(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

/// `candidate` is strictly newer than `installed` (both must parse).
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    match (Version::parse(candidate), Version::parse(installed)) {
        (Some(c), Some(i)) => c > i,
        _ => false,
    }
}

/// winget prints `< 1.2.3` or `> 1.2.3` when it only knows a bound.
/// Returns the bare version and whether it was approximate.
pub fn strip_approx(s: &str) -> (String, bool) {
    let t = s.trim();
    for prefix in ["<", ">"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return (rest.trim().to_string(), true);
        }
    }
    (t.to_string(), false)
}

/// The installed version string means "unknown" (winget prints `Unknown`,
/// localized builds may print other words, registry may hold nothing).
pub fn is_unknown(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || Version::parse(strip_approx(t).0.as_str()).is_none()
}

fn is_year_like(n: u64) -> bool {
    (1990..=2100).contains(&n)
}

/// Is going from `from` to `to` a *major* evolution worth flagging?
///
/// Rules: the first component must increase; calendar versions (2025 → 2026)
/// and rapid-release schemes (first component ≥ 20, e.g. browsers) are not
/// flagged; `0.x → 0.y` is not flagged.
pub fn is_major_jump(from: &str, to: &str) -> bool {
    let (Some(a), Some(b)) = (Version::parse(from), Version::parse(to)) else {
        return false;
    };
    let (ma, mb) = (a.major(), b.major());
    if mb <= ma {
        return false;
    }
    if is_year_like(ma) || is_year_like(mb) {
        return false;
    }
    if ma >= 20 {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_ordering() {
        assert!(is_newer("1.142.0", "1.141.2"));
        assert!(is_newer("2026.3.0", "2026.2.0"));
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(is_newer("1.0.1", "1.0"));
        assert!(is_newer("10.0", "9.9.9"));
        assert!(is_newer("v2.0", "1.9"));
        assert_eq!(compare("1.0.0.0", "1"), Ordering::Equal);
    }

    #[test]
    fn prerelease_suffix_is_lower() {
        assert!(is_newer("1.0.0", "1.0.0-beta"));
        assert!(is_newer("1.0.0-rc2", "1.0.0-rc1"));
        assert!(is_newer("2.4.1b", "2.4.1a"));
    }

    #[test]
    fn unknown_and_approx() {
        assert!(is_unknown("Unknown"));
        assert!(is_unknown(""));
        assert!(!is_unknown("< 1.2"));
        assert_eq!(strip_approx("< 1.2.3"), ("1.2.3".to_string(), true));
        assert_eq!(strip_approx("4.5"), ("4.5".to_string(), false));
        assert!(!is_newer("Unknown", "1.0"));
    }

    #[test]
    fn major_jumps() {
        assert!(is_major_jump("3.7.9", "4.0.1"));
        assert!(is_major_jump("8.0.412", "21.0.2"));
        assert!(!is_major_jump("1.141.2", "1.142.0"));
        assert!(!is_major_jump("2025.4.0", "2026.1.0"));
        assert!(!is_major_jump("140.0.1", "141.0"));
        assert!(!is_major_jump("0.9", "0.10"));
        assert!(!is_major_jump("Unknown", "2.0"));
    }
}
