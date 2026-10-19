//! Version information of executable files.
//!
//! OWNER: system agent.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};

use crate::system::registry::{norm_path, parse_icon_location, InstalledProgram, SharedLocations};
use crate::system::wide;

/// `ProductVersion` (falling back to `FileVersion`) of a PE file, cleaned
/// (`1, 2, 3, 0` → `1.2.3.0`).
pub fn file_version(path: &Path) -> Option<String> {
    let block = VersionBlock::read(path)?;
    let translations = block.translations();
    for field in ["ProductVersion", "FileVersion"] {
        for (lang, codepage) in &translations {
            let query = format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\{field}");
            if let Some(version) = block.string(&query).as_deref().and_then(clean_version) {
                return Some(version);
            }
        }
    }
    block.fixed_version()
}

/// Best guess of the program's main executable: `DisplayIcon` when it is an
/// .exe (not an uninstaller), else an .exe in `InstallLocation` whose name
/// resembles the display name.
pub fn main_executable(p: &InstalledProgram) -> Option<PathBuf> {
    if let Some((icon, _)) = p.display_icon.as_deref().and_then(parse_icon_location) {
        let in_shared_dir = icon
            .parent()
            .is_none_or(|dir| SharedLocations::get().contains(&norm_path(&dir.to_string_lossy())));
        if is_exe(&icon) && !is_helper_exe(&icon) && !in_shared_dir && icon.is_file() {
            return Some(icon);
        }
    }

    let wanted = NameKey::new(&p.display_name);
    if wanted.compact.is_empty() {
        return None;
    }
    // `install_dirs` starts with InstallLocation, then the folders of
    // DisplayIcon and UninstallString (useful when InstallLocation is empty).
    let dirs = p.install_dirs();
    let mut best: Option<(u32, PathBuf)> = None;
    for dir in &dirs {
        consider_dir(dir, &wanted, &mut best);
    }
    if best.is_none() {
        for dir in &dirs {
            for sub in subdirectories(dir) {
                consider_dir(&sub, &wanted, &mut best);
            }
        }
    }
    best.map(|(_, exe)| exe)
}

/// Installed version from the main executable, when the registry has none.
pub fn detect_version(p: &InstalledProgram) -> Option<String> {
    main_executable(p).and_then(|exe| file_version(&exe))
}

/// Cleans a version string: `1, 2, 3, 0` → `1.2.3.0`,
/// `10.0.22621.1 (WinBuild.160101.0800)` → `10.0.22621.1`, `v2.1` → `2.1`.
/// `None` when no digit is found or the version is all zeros.
pub(crate) fn clean_version(raw: &str) -> Option<String> {
    let s = raw.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut out = String::new();
    let mut chars = s[start..].chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '0'..='9' | '.' => out.push(c),
            ',' => {
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                if !chars.peek().is_some_and(char::is_ascii_digit) {
                    break;
                }
                out.push('.');
            }
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '+' | '_') => out.push(c),
            _ => break,
        }
    }
    let out = out.trim_end_matches(['.', '-', '+', '_']);
    let meaningful = out.chars().any(|c| c.is_ascii_digit() && c != '0');
    meaningful.then(|| out.to_string())
}

// ---------------------------------------------------------------------------
// Version resource
// ---------------------------------------------------------------------------

struct VersionBlock {
    /// `u64` storage keeps the block aligned for the structures inside it.
    data: Vec<u64>,
}

impl VersionBlock {
    fn read(path: &Path) -> Option<VersionBlock> {
        let wpath = wide(path);
        // SAFETY: `wpath` is NUL-terminated and outlives the call.
        let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(wpath.as_ptr()), None) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: `data` provides at least `size` writable bytes.
        unsafe { GetFileVersionInfoW(PCWSTR(wpath.as_ptr()), None, size, data.as_mut_ptr().cast()) }.ok()?;
        Some(VersionBlock { data })
    }

    /// Raw `VerQueryValueW`: pointer into the block and length (characters
    /// for strings, bytes otherwise).
    fn query(&self, sub_block: &str) -> Option<(*const c_void, u32)> {
        let sub = wide(sub_block);
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: the block was filled by GetFileVersionInfoW and outlives the
        // returned pointer's use (callers copy the data out immediately).
        let ok = unsafe { VerQueryValueW(self.data.as_ptr().cast(), PCWSTR(sub.as_ptr()), &mut ptr, &mut len) };
        (ok.as_bool() && !ptr.is_null() && len > 0).then_some((ptr.cast_const(), len))
    }

    fn string(&self, sub_block: &str) -> Option<String> {
        let (ptr, len) = self.query(sub_block)?;
        let ptr = ptr.cast::<u16>();
        let units: Vec<u16> = (0..len as usize)
            // SAFETY: VerQueryValueW guarantees `len` characters at `ptr`;
            // read_unaligned does not assume any alignment.
            .map(|i| unsafe { ptr.add(i).read_unaligned() })
            .take_while(|&unit| unit != 0)
            .collect();
        let s = String::from_utf16_lossy(&units);
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_string())
    }

    /// `(language, codepage)` pairs: the declared translations first, then
    /// the usual suspects (US English / neutral, Unicode / Windows-1252).
    fn translations(&self) -> Vec<(u16, u16)> {
        let mut list = Vec::new();
        if let Some((ptr, len)) = self.query("\\VarFileInfo\\Translation") {
            let ptr = ptr.cast::<u16>();
            for i in 0..(len as usize / 4) {
                // SAFETY: the value holds `len` bytes, i.e. len / 4 pairs of u16.
                let pair = unsafe { (ptr.add(2 * i).read_unaligned(), ptr.add(2 * i + 1).read_unaligned()) };
                list.push(pair);
            }
        }
        for fallback in [
            (0x0409, 0x04b0),
            (0x0409, 0x04e4),
            (0x0409, 0x0000),
            (0x0000, 0x04b0),
            (0x0000, 0x04e4),
        ] {
            if !list.contains(&fallback) {
                list.push(fallback);
            }
        }
        list
    }

    fn fixed_version(&self) -> Option<String> {
        let (ptr, len) = self.query("\\")?;
        if (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }
        // SAFETY: the value is a VS_FIXEDFILEINFO of `len` >= size_of bytes.
        let info = unsafe { ptr.cast::<VS_FIXEDFILEINFO>().read_unaligned() };
        if info.dwSignature != 0xFEEF_04BD {
            return None;
        }
        let format = |ms: u32, ls: u32| format!("{}.{}.{}.{}", ms >> 16, ms & 0xffff, ls >> 16, ls & 0xffff);
        clean_version(&format(info.dwProductVersionMS, info.dwProductVersionLS))
            .or_else(|| clean_version(&format(info.dwFileVersionMS, info.dwFileVersionLS)))
    }
}

// ---------------------------------------------------------------------------
// Main executable heuristics
// ---------------------------------------------------------------------------

fn is_exe(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

/// Uninstallers, updaters, installers, crash reporters and other helpers that
/// ship next to the real program.
fn is_helper_exe(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    const PREFIXES: [&str; 4] = ["unins", "uninst", "setup", "install"];
    const PARTS: [&str; 15] = [
        "uninstall",
        "update",
        "crash",
        "bugreport",
        "errorreport",
        "reporter",
        "helper",
        "notification",
        "elevat",
        "_proxy",
        "squirrel",
        "installer",
        "service",
        "subprocess",
        "webengineprocess",
    ];
    PREFIXES.iter().any(|p| stem.starts_with(p))
        || PARTS.iter().any(|p| stem.contains(p))
        || matches!(stem.as_str(), "vc_redist.x64" | "vc_redist.x86" | "dotnet")
}

/// What an executable's file stem is compared with.
struct NameKey {
    /// Meaningful words of the display name (no versions, architectures...).
    tokens: Vec<String>,
    /// The tokens joined: `VLC media player` → `vlcmediaplayer`.
    compact: String,
}

impl NameKey {
    fn new(display_name: &str) -> NameKey {
        const NOISE: [&str; 22] = [
            "x64",
            "x86",
            "x32",
            "arm64",
            "64",
            "32",
            "bit",
            "64bit",
            "32bit",
            "edition",
            "version",
            "the",
            "for",
            "and",
            "of",
            "by",
            "user",
            "machine",
            "inc",
            "llc",
            "ltd",
            "corporation",
        ];
        let lower = display_name.to_lowercase();
        let tokens: Vec<String> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .enumerate()
            // Version numbers end the useful part of most names (`GIMP 2.10`),
            // but a name may start with a digit (`7-Zip`).
            .take_while(|(i, t)| *i == 0 || !t.starts_with(|c: char| c.is_ascii_digit()))
            .map(|(_, t)| t)
            .filter(|t| !NOISE.contains(t))
            .map(str::to_string)
            .collect();
        let compact = tokens.concat();
        NameKey { tokens, compact }
    }

    /// 0 (unrelated) to 100 (same name).
    fn resemblance(&self, stem: &str) -> u32 {
        let s: String = stem.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
        if s.len() < 2 || self.compact.is_empty() {
            return 0;
        }
        if s == self.compact {
            return 100;
        }
        if self.tokens.contains(&s) {
            return 80;
        }
        if s.len() >= 3 && (self.compact.starts_with(&s) || s.starts_with(&self.compact)) {
            return 70;
        }
        let token_inside = self
            .tokens
            .iter()
            .any(|t| (t.len() >= 4 && s.contains(t.as_str())) || (t.len() >= 3 && s.starts_with(t.as_str())));
        let stem_inside = s.len() >= 4 && self.tokens.iter().any(|t| t.contains(&s));
        if token_inside || stem_inside {
            return 50;
        }
        let acronym: String = self.tokens.iter().filter_map(|t| t.chars().next()).collect();
        if self.tokens.len() >= 2 && acronym == s {
            return 40;
        }
        0
    }
}

fn consider_dir(dir: &Path, wanted: &NameKey, best: &mut Option<(u32, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_exe(&path) || is_helper_exe(&path) || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let score = wanted.resemblance(&stem);
        if score > 0 && best.as_ref().is_none_or(|(top, _)| score > *top) {
            *best = Some((score, path));
        }
    }
}

fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::registry::Hive;

    #[test]
    fn cleans_versions() {
        let c = |s: &str| clean_version(s);
        assert_eq!(c("1, 2, 3, 0").as_deref(), Some("1.2.3.0"));
        assert_eq!(c("1,2,3,4").as_deref(), Some("1.2.3.4"));
        assert_eq!(
            c("10.0.22621.1 (WinBuild.160101.0800)").as_deref(),
            Some("10.0.22621.1")
        );
        assert_eq!(
            c("4.8.9037.0 built by: NET481REL1LAST_C").as_deref(),
            Some("4.8.9037.0")
        );
        assert_eq!(c(" v2.3.1 ").as_deref(), Some("2.3.1"));
        assert_eq!(c("2.3.1-beta").as_deref(), Some("2.3.1-beta"));
        assert_eq!(c("3.0.21 (x64)").as_deref(), Some("3.0.21"));
        assert_eq!(c("1.2.").as_deref(), Some("1.2"));
        assert_eq!(c("0.0.0.0"), None);
        assert_eq!(c("unknown"), None);
        assert_eq!(c(""), None);
    }

    #[test]
    fn reads_system_file_versions() {
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        for file in ["explorer.exe", r"System32\notepad.exe", r"System32\kernel32.dll"] {
            let version = file_version(&Path::new(&windir).join(file));
            let version = version.unwrap_or_else(|| panic!("no version for {file}"));
            assert!(
                version.starts_with("10.") || version.starts_with("11."),
                "{file}: {version}"
            );
            assert!(version.split('.').count() >= 3, "{file}: {version}");
        }
        assert_eq!(
            file_version(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), r"\Cargo.toml"))),
            None
        );
        assert_eq!(file_version(Path::new(r"C:\does\not\exist.exe")), None);
    }

    #[test]
    fn name_resemblance() {
        let vlc = NameKey::new("VLC media player");
        assert_eq!(vlc.resemblance("vlc"), 80);
        assert_eq!(vlc.resemblance("vlc-cache-gen"), 50);
        assert_eq!(NameKey::new("Notepad++ (64-bit x64)").resemblance("notepad++"), 100);
        assert_eq!(NameKey::new("FileZilla 3.69.3").resemblance("filezilla"), 100);
        assert_eq!(NameKey::new("GIMP 2.10.38").resemblance("gimp-2.10"), 70);
        assert_eq!(NameKey::new("Visual Studio Code").resemblance("vsc"), 40);
        assert_eq!(NameKey::new("Mozilla Firefox (x64 fr)").resemblance("firefox"), 80);
        assert_eq!(NameKey::new("Mozilla Firefox").resemblance("pingsender"), 0);
        assert_eq!(NameKey::new("7-Zip 24.08 (x64)").compact, "7zip");
    }

    #[test]
    fn helpers_are_not_main_executables() {
        for name in [
            "unins000.exe",
            "Uninstall.exe",
            "Update.exe",
            "crashpad_handler.exe",
            "setup.exe",
            "elevation_service.exe",
            "chrome_proxy.exe",
        ] {
            assert!(is_helper_exe(Path::new(name)), "{name}");
        }
        for name in ["vlc.exe", "firefox.exe", "Discord.exe", "notepad++.exe"] {
            assert!(!is_helper_exe(Path::new(name)), "{name}");
        }
    }

    /// Scratch folder inside the target directory (temp folders are shared
    /// locations, ignored by `install_dirs`).
    fn scratch_dir(name: &str) -> PathBuf {
        let exe = std::env::current_exe().expect("test executable path");
        let target = exe.parent().and_then(Path::parent).expect("target directory");
        let dir = target.join(format!("majestic-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn program(name: &str) -> InstalledProgram {
        InstalledProgram {
            hive: Hive::Hkcu,
            key_name: "key".into(),
            display_name: name.into(),
            display_version: None,
            publisher: None,
            install_location: None,
            display_icon: None,
            uninstall_string: None,
            url_info_about: None,
            url_update_info: None,
            help_link: None,
            install_date: None,
            system_component: false,
            windows_installer: false,
            is_update: false,
        }
    }

    #[test]
    fn finds_main_executables() {
        let dir = scratch_dir("mainexe");
        for file in ["MyApp.exe", "unins000.exe", "Update.exe", "other.exe", "MyApp.dll"] {
            std::fs::write(dir.join(file), b"").expect("write");
        }
        std::fs::create_dir_all(dir.join("bin")).expect("mkdir");
        std::fs::write(dir.join("bin").join("myapp-cli.exe"), b"").expect("write");

        // DisplayIcon points to the uninstaller: look in InstallLocation.
        let mut p = program("My App 1.2.3 (x64)");
        p.display_icon = Some(format!("\"{}\",0", dir.join("unins000.exe").display()));
        p.install_location = Some(dir.display().to_string());
        assert_eq!(main_executable(&p), Some(dir.join("MyApp.exe")));

        // A usable DisplayIcon wins.
        let mut q = program("Something Else");
        q.display_icon = Some(format!("{},0", dir.join("other.exe").display()));
        assert_eq!(main_executable(&q), Some(dir.join("other.exe")));

        // One level deep when nothing matches at the top.
        let mut r = program("MyApp CLI");
        r.install_location = Some(dir.display().to_string());
        std::fs::remove_file(dir.join("MyApp.exe")).expect("remove");
        assert_eq!(main_executable(&r), Some(dir.join("bin").join("myapp-cli.exe")));

        // Nothing resembles the name.
        let mut s = program("Totally Unrelated");
        s.install_location = Some(dir.display().to_string());
        assert_eq!(main_executable(&s), None);

        // System executables are never the main executable of a program.
        let mut t = program("Explorer");
        t.display_icon = Some(r"%SystemRoot%\explorer.exe,0".into());
        assert_eq!(main_executable(&t), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
