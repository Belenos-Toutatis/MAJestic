//! What kind of program is this (for programs no package manager handles)?
//!
//! OWNER: system agent.

use crate::model::Category;
use crate::system::registry::{command_exe, InstalledProgram};

/// Classifies an installed program: game launcher titles (Steam
/// `steam://uninstall`, Epic, GOG, Ubisoft, EA, Battle.net), browser web apps
/// (Edge/Chrome PWA uninstall strings), self-updating apps (Squirrel
/// `Update.exe`, browsers, Office Click-to-Run, Teams, Zoom, Discord...),
/// drivers and vendor utilities (NVIDIA, AMD, Intel, Realtek...), system
/// components (runtimes, redistributables, SDKs, Windows components).
/// Everything else is `Category::Application`.
pub fn classify(p: &InstalledProgram) -> Category {
    let c = Facts::new(p);
    if let Some(launcher) = game_launcher(&c) {
        return Category::Game {
            launcher: launcher.to_string(),
        };
    }
    if let Some(browser) = web_app_browser(&c) {
        return Category::WebApp { browser };
    }
    if is_driver(&c) {
        return Category::Driver;
    }
    if p.system_component || p.is_update || is_system_component(&c) {
        return Category::SystemComponent;
    }
    if let Some(by) = auto_updater(&c) {
        return Category::AutoUpdating { by };
    }
    Category::Application
}

/// Normalized registry values the rules look at.
struct Facts<'a> {
    program: &'a InstalledProgram,
    /// Display name: lowercase, ® ™ (R) (TM) and punctuation runs turned into
    /// single spaces.
    name: String,
    /// Publisher, normalized like `name`.
    publisher: String,
    /// Raw lowercase `UninstallString`.
    uninstall: String,
    /// Raw lowercase `DisplayIcon`.
    icon: String,
    /// Raw lowercase `InstallLocation`.
    location: String,
}

impl<'a> Facts<'a> {
    fn new(p: &'a InstalledProgram) -> Facts<'a> {
        let lower = |v: &Option<String>| v.as_deref().unwrap_or_default().to_lowercase();
        Facts {
            program: p,
            name: normalize(&p.display_name),
            publisher: normalize(p.publisher.as_deref().unwrap_or_default()),
            uninstall: lower(&p.uninstall_string),
            icon: lower(&p.display_icon),
            location: lower(&p.install_location),
        }
    }

    fn name_has(&self, phrases: &[&str]) -> bool {
        phrases.iter().any(|phrase| has_phrase(&self.name, phrase))
    }
}

/// Lowercase, trademark signs removed, separators other than `.`, `+`, `-`
/// and `#` turned into single spaces: `Intel(R) Wi-Fi 6 | Driver` →
/// `intel wi-fi 6 driver`.
fn normalize(s: &str) -> String {
    let lower = s
        .to_lowercase()
        .replace("(r)", " ")
        .replace("(tm)", " ")
        .replace("(c)", " ");
    let mut out = String::with_capacity(lower.len());
    for ch in lower.chars() {
        let keep = ch.is_alphanumeric() || matches!(ch, '.' | '+' | '-' | '#');
        if keep {
            out.push(ch);
        } else if !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '+' || c == '#'
}

/// `phrase` occurs in `text` on word boundaries.
fn has_phrase(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(start, _)| {
        let end = start + phrase.len();
        let before_ok = text[..start].chars().next_back().is_none_or(|c| !is_word_char(c));
        let after_ok = text[end..].chars().next().is_none_or(|c| !is_word_char(c));
        before_ok && after_ok
    })
}

/// `text` starts with `phrase` followed by a word boundary.
fn starts_with_phrase(text: &str, phrase: &str) -> bool {
    text.strip_prefix(phrase)
        .is_some_and(|rest| rest.chars().next().is_none_or(|c| !is_word_char(c)))
}

// ---------------------------------------------------------------------------
// Games
// ---------------------------------------------------------------------------

fn game_launcher(c: &Facts) -> Option<&'static str> {
    let u = c.uninstall.as_str();
    if u.contains("steam://uninstall") || c.location.contains(r"\steamapps\common\") {
        return Some("Steam");
    }
    if u.contains("com.epicgames.launcher://") {
        return Some("Epic Games");
    }
    if u.contains("uplay://uninstall") {
        return Some("Ubisoft Connect");
    }
    if u.contains("link2ea://") || u.contains("origin2://") || u.contains("origin://") || u.contains(r"\eainstaller\") {
        return Some("EA app");
    }
    let blizzard =
        u.contains("blizzard uninstaller") || u.contains(r"\battle.net\agent\") || u.contains("battlenet://");
    if blizzard && !u.contains("--uid=battle.net") && !starts_with_phrase(&c.name, "battle.net") {
        return Some("Battle.net");
    }
    if u.contains("riotclientservices.exe") && u.contains("--uninstall-product") {
        return Some("Riot Client");
    }
    if u.contains("amazon game remover") {
        return Some("Amazon Games");
    }
    if u.contains(r"\rockstar games\launcher\") && u.contains("-uninstall=") {
        return Some("Rockstar Games Launcher");
    }
    let gog_folder = (u.contains(r"\gog games\") || c.location.contains(r"\gog games\")) && u.contains("unins");
    if !starts_with_phrase(&c.name, "gog galaxy") && (c.publisher == "gog.com" || gog_folder) {
        return Some("GOG");
    }
    None
}

// ---------------------------------------------------------------------------
// Browser web apps
// ---------------------------------------------------------------------------

fn web_app_browser(c: &Facts) -> Option<String> {
    let u = c.uninstall.as_str();
    let is_pwa = u.contains("--uninstall-app-id")
        || u.contains("_proxy.exe")
        || c.icon.contains(r"\web applications\")
        || c.icon.contains("_proxy.exe");
    if !is_pwa {
        return None;
    }
    let exe_stem = c
        .program
        .uninstall_string
        .as_deref()
        .and_then(command_exe)
        .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let known = |hint: &str| -> Option<&'static str> {
        const BROWSERS: [(&str, &str); 9] = [
            ("msedge", "Microsoft Edge"),
            (r"\microsoft\edge", "Microsoft Edge"),
            ("brave", "Brave"),
            ("vivaldi", "Vivaldi"),
            ("opera", "Opera"),
            ("chromium", "Chromium"),
            ("thorium", "Thorium"),
            ("chrome", "Google Chrome"),
            (r"\google\", "Google Chrome"),
        ];
        BROWSERS
            .iter()
            .find(|(needle, _)| hint.contains(needle))
            .map(|(_, name)| *name)
    };
    let browser = known(&exe_stem)
        .or_else(|| known(u))
        .or_else(|| known(&c.icon))
        .map(str::to_string)
        .unwrap_or_else(|| title_case(exe_stem.trim_end_matches("_proxy")));
    Some(if browser.is_empty() {
        "Browser".to_string()
    } else {
        browser
    })
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Drivers and vendor utilities
// ---------------------------------------------------------------------------

/// Hardware vendors: whatever they publish is a driver or a utility for
/// their hardware.
const HARDWARE_PUBLISHERS: &[&str] = &[
    "nvidia",
    "advanced micro devices",
    "amd",
    "intel",
    "realtek",
    "realtek semiconductor",
    "synaptics",
    "elan microelectronics",
    "alps",
    "alps alpine",
    "conexant",
    "cirrus logic",
    "logitech",
    "razer",
    "corsair",
    "steelseries",
    "qualcomm",
    "mediatek",
    "broadcom",
    "marvell",
    "asmedia",
    "via technologies",
    "rivet networks",
    "killer networking",
    "waves audio",
    "a-volute",
    "nahimic",
    "wacom",
    "tobii",
    "prolific",
    "ftdi",
    "silicon laboratories",
    "silicon labs",
];

/// Words that make a hardware vendor's product a software component rather
/// than a driver (`NVIDIA CUDA Toolkit`, `Intel oneAPI`...).
const NOT_HARDWARE: &[&str] = &[
    "sdk",
    "toolkit",
    "oneapi",
    "openvino",
    "compiler",
    "redistributable",
    "development kit",
    "plugin",
    "plugins",
];

/// Names of drivers and hardware utilities.
const DRIVER_PHRASES: &[&str] = &[
    "driver",
    "drivers",
    // Localized "Windows Driver Package - ..." entries (fr, de, es).
    "pilote",
    "pilotes",
    "treiber",
    "treiberpaket",
    "controlador",
    "controladores",
    "chipset",
    "firmware",
    "bios",
    "uefi",
    "thunderbolt",
    "bluetooth",
    "touchpad",
    "fingerprint",
    "card reader",
    "hd audio",
    "high definition audio",
    "management engine",
    "rapid storage",
    "serial io",
    "smart sound technology",
    "dynamic platform and thermal framework",
    "dynamic tuning",
    "wireless lan",
    "wlan",
    "basic device software",
    "mfl-pro",
    "supportassist",
    "support assistant",
    "lenovo vantage",
    "vantage service",
    "lenovo system update",
    "system interface foundation",
    "dell command update",
    "armoury crate",
    "myasus",
    "asus framework",
    "dragon center",
    "msi center",
    "aura sync",
    "icue",
    "logitech g hub",
    "logi options",
    "logi options+",
    "setpoint",
    "steelseries gg",
    "samsung magician",
    "care center",
    "nahimic",
    "maxxaudio",
    "sonic studio",
    "killer control center",
    "killer performance",
    "geforce experience",
    "nvidia app",
    "radeon",
    "ryzen master",
];

/// Names that contain a driver word but are applications or software
/// components (`Driver Booster`, `ODBC Driver for SQL Server`, `PDF printer`,
/// `Windows Driver Kit`...).
const NOT_DRIVER_PHRASES: &[&str] = &[
    "booster",
    "driver easy",
    "drivereasy",
    "driver genius",
    "driver talent",
    "driverpack",
    "drivermax",
    "snappy driver",
    "driver store",
    "updater",
    "kit",
    "odbc",
    "ole db",
    "oledb",
    "jdbc",
    "sql",
    "pdf",
];

/// PC makers whose system utilities are drivers in all but name.
const OEM_BRANDS: &[&str] = &[
    "hp", "dell", "lenovo", "asus", "acer", "msi", "gigabyte", "asrock", "toshiba", "dynabook", "fujitsu", "huawei",
];
const OEM_UTILITY_WORDS: &[&str] = &[
    "support",
    "update",
    "service",
    "services",
    "system",
    "hotkey",
    "hotkeys",
    "power",
    "battery",
    "framework",
    "control center",
    "audio",
    "camera",
    "display",
    "touchpad",
    "keyboard",
    "utility",
    "utilities",
    "center",
    "assistant",
];

fn is_driver(c: &Facts) -> bool {
    if c.name_has(NOT_DRIVER_PHRASES) {
        return false;
    }
    if HARDWARE_PUBLISHERS
        .iter()
        .any(|vendor| starts_with_phrase(&c.publisher, vendor))
        && !c.name_has(NOT_HARDWARE)
    {
        return true;
    }
    if c.name_has(DRIVER_PHRASES) || c.name.starts_with("windows driver package") {
        return true;
    }
    OEM_BRANDS.iter().any(|brand| starts_with_phrase(&c.name, brand)) && c.name_has(OEM_UTILITY_WORDS)
}

// ---------------------------------------------------------------------------
// System components
// ---------------------------------------------------------------------------

const SYSTEM_PHRASES: &[&str] = &[
    "redistributable",
    "redistributables",
    "redist",
    "vcredist",
    "runtime",
    "runtimes",
    "run time",
    "run-time",
    "framework",
    "sdk",
    "development kit",
    "driver kit",
    "deployment kit",
    "certification kit",
    "toolkit",
    "targeting pack",
    "windows kits",
    "directx",
    "webview2",
    "universal crt",
    "jre",
    "jdk",
    "openjdk",
    "winrt intellisense",
    "msi development tools",
    "application verifier",
    "kits configuration installer",
    "silverlight",
    "xna",
    "vulkan",
    "openal",
    "gameinput",
    "update helper",
    "maintenance service",
    "update health tools",
    "pc health check",
    "setup remediations",
    "policy platform",
    "access database engine",
    "application compatibility",
    "odbc",
    "ole db",
    "oledb",
    "jdbc",
];

const SQL_SERVER_COMPONENTS: &[&str] = &[
    "native client",
    "clr types",
    "management objects",
    "localdb",
    "compact",
    "language service",
    "support files",
    "vss writer",
    "browser",
    "setup",
];

fn is_system_component(c: &Facts) -> bool {
    let n = c.name.as_str();
    if c.name_has(SYSTEM_PHRASES) {
        return true;
    }
    let microsoft = starts_with_phrase(n, "microsoft") || starts_with_phrase(&c.publisher, "microsoft");
    if microsoft && (n.contains(".net") || n.starts_with("visual c++") || has_phrase(n, "visual c++")) {
        return true;
    }
    if has_phrase(n, "sql server") && SQL_SERVER_COMPONENTS.iter().any(|part| has_phrase(n, part)) {
        return true;
    }
    // `Java 8 Update 431`, `Java Auto Updater`, `Java SE Runtime`.
    if starts_with_phrase(n, "java") && ["update", "updater", "se", "runtime"].iter().any(|w| has_phrase(n, w)) {
        return true;
    }
    // Windows updates and hotfixes: `Update for Windows 10 (KB5001716)`.
    if n.contains(" kb")
        && n.split(' ')
            .any(|w| w.len() > 2 && w.starts_with("kb") && w[2..].chars().all(|c| c.is_ascii_digit()))
    {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Self-updating programs
// ---------------------------------------------------------------------------

/// Display-name prefixes of programs that keep themselves up to date, and
/// who updates them. `true` = the name must match exactly.
const SELF_UPDATERS: &[(&str, &str, bool)] = &[
    ("google chrome", "Google Chrome", false),
    ("microsoft edge", "Microsoft Edge", false),
    ("mozilla firefox", "Mozilla Firefox", false),
    ("firefox", "Mozilla Firefox", false),
    ("mozilla thunderbird", "Mozilla Thunderbird", false),
    ("brave", "Brave", false),
    ("opera", "Opera", false),
    ("opera gx", "Opera", false),
    ("vivaldi", "Vivaldi", false),
    ("microsoft 365", "Microsoft Office Click-to-Run", false),
    ("microsoft office 365", "Microsoft Office Click-to-Run", false),
    ("microsoft teams", "Microsoft Teams", false),
    ("teams machine-wide installer", "Microsoft Teams", false),
    ("zoom", "Zoom", false),
    ("zoom workplace", "Zoom", false),
    ("discord", "Discord", false),
    ("slack", "Slack", false),
    ("spotify", "Spotify", false),
    ("steam", "Steam", true),
    ("dropbox", "Dropbox", false),
    ("microsoft onedrive", "Microsoft OneDrive", false),
    ("onedrive", "Microsoft OneDrive", false),
    ("adobe creative cloud", "Adobe Creative Cloud", false),
    ("jetbrains toolbox", "JetBrains Toolbox", false),
    ("microsoft visual studio code", "Visual Studio Code", false),
    ("visual studio code", "Visual Studio Code", false),
    ("whatsapp", "WhatsApp", false),
    ("telegram desktop", "Telegram Desktop", false),
    ("signal", "Signal", true),
    ("notion", "Notion", true),
    ("obsidian", "Obsidian", true),
    ("figma", "Figma", false),
    ("postman", "Postman", false),
    ("docker desktop", "Docker Desktop", false),
    ("github desktop", "GitHub Desktop", false),
    ("google drive", "Google Drive", false),
    ("webex", "Webex", false),
    ("1password", "1Password", false),
    ("bitwarden", "Bitwarden", false),
    ("epic games launcher", "Epic Games Launcher", false),
    ("ubisoft connect", "Ubisoft Connect", false),
    ("ea app", "EA app", true),
    ("origin", "Origin", true),
    ("battle.net", "Battle.net", false),
    ("gog galaxy", "GOG GALAXY", false),
    ("riot client", "Riot Client", false),
];

fn auto_updater(c: &Facts) -> Option<String> {
    let u = c.uninstall.as_str();
    if u.contains("officeclicktorun.exe") {
        return Some("Microsoft Office Click-to-Run".into());
    }
    if u.contains(r"\adobe desktop common\") || u.contains("--sapcode=") {
        return Some("Adobe Creative Cloud".into());
    }
    if u.contains(r"\jetbrains\toolbox\") || c.location.contains(r"\jetbrains\toolbox\apps\") {
        return Some("JetBrains Toolbox".into());
    }
    for (prefix, by, exact) in SELF_UPDATERS {
        let matched = if *exact {
            c.name == *prefix
        } else {
            starts_with_phrase(&c.name, prefix)
        };
        if matched {
            return Some((*by).to_string());
        }
    }
    if starts_with_phrase(&c.publisher, "zoom video communications") {
        return Some("Zoom".into());
    }
    // Squirrel installs: `"%LOCALAPPDATA%\App\Update.exe" --uninstall`.
    let exe = c.program.uninstall_string.as_deref().and_then(command_exe);
    let squirrel = exe
        .as_ref()
        .and_then(|e| e.file_name())
        .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case("update.exe"));
    if squirrel && u.contains("--uninstall") {
        return Some(c.program.display_name.trim().to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::registry::Hive;

    fn prog(name: &str, publisher: &str, uninstall: &str) -> InstalledProgram {
        let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
        InstalledProgram {
            hive: Hive::Hklm64,
            key_name: "key".into(),
            display_name: name.into(),
            display_version: None,
            publisher: opt(publisher),
            install_location: None,
            display_icon: None,
            uninstall_string: opt(uninstall),
            url_info_about: None,
            url_update_info: None,
            help_link: None,
            install_date: None,
            system_component: false,
            windows_installer: false,
            is_update: false,
        }
    }

    fn game(launcher: &str) -> Category {
        Category::Game {
            launcher: launcher.into(),
        }
    }

    fn auto(by: &str) -> Category {
        Category::AutoUpdating { by: by.into() }
    }

    #[test]
    fn normalization() {
        assert_eq!(
            normalize("Intel(R) Wi-Fi 6 AX201 | Driver®"),
            "intel wi-fi 6 ax201 driver"
        );
        assert_eq!(
            normalize("Microsoft Visual C++ 2015-2022 Redistributable (x64)"),
            "microsoft visual c++ 2015-2022 redistributable x64"
        );
        assert!(has_phrase("zoom workplace", "zoom"));
        assert!(!has_phrase("zoomit", "zoom"));
        assert!(!has_phrase("notepad++", "notepad"));
        assert!(starts_with_phrase("steam", "steam"));
        assert!(!starts_with_phrase("steamvr", "steam"));
    }

    #[test]
    fn games() {
        let cases = [
            (
                prog(
                    "Portal 2",
                    "Valve",
                    r#""C:\Program Files (x86)\Steam\steam.exe" steam://uninstall/620"#,
                ),
                game("Steam"),
            ),
            (
                prog(
                    "Fortnite",
                    "Epic Games, Inc.",
                    r#""C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win32\EpicGamesLauncher.exe" com.epicgames.launcher://apps/Fortnite?action=uninstall"#,
                ),
                game("Epic Games"),
            ),
            (
                prog(
                    "The Witcher 3",
                    "GOG.com",
                    r#""D:\GOG Games\The Witcher 3\unins000.exe""#,
                ),
                game("GOG"),
            ),
            (
                prog("Some Game", "", r#""D:\GOG Games\Some Game\unins000.exe""#),
                game("GOG"),
            ),
            (
                prog(
                    "Far Cry 5",
                    "Ubisoft",
                    r#""C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\upc.exe" uplay://uninstall/5266"#,
                ),
                game("Ubisoft Connect"),
            ),
            (
                prog(
                    "Battlefield 4",
                    "Electronic Arts",
                    r#""C:\Program Files\Common Files\EAInstaller\Battlefield 4\Cleanup.exe" uninstall_game -autologging"#,
                ),
                game("EA app"),
            ),
            (
                prog(
                    "World of Warcraft",
                    "Blizzard Entertainment",
                    r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=enUS --uid=wow --displayname="World of Warcraft""#,
                ),
                game("Battle.net"),
            ),
            (
                prog(
                    "VALORANT",
                    "Riot Games, Inc",
                    r#""C:\Riot Games\Riot Client\RiotClientServices.exe" --uninstall-product=valorant --uninstall-patchline=live"#,
                ),
                game("Riot Client"),
            ),
        ];
        for (p, expected) in cases {
            assert_eq!(classify(&p), expected, "{}", p.display_name);
        }
        let mut located = prog("Half-Life", "Valve", "");
        located.install_location = Some(r"D:\SteamLibrary\steamapps\common\Half-Life".into());
        assert_eq!(classify(&located), game("Steam"));
    }

    #[test]
    fn launchers_themselves_update_themselves() {
        assert_eq!(
            classify(&prog(
                "Steam",
                "Valve Corporation",
                r"C:\Program Files (x86)\Steam\uninstall.exe"
            )),
            auto("Steam")
        );
        assert_eq!(
            classify(&prog(
                "GOG GALAXY",
                "GOG.com",
                r#""C:\Program Files (x86)\GOG Galaxy\unins000.exe""#
            )),
            auto("GOG GALAXY")
        );
        assert_eq!(
            classify(&prog(
                "Battle.net",
                "Blizzard Entertainment",
                r#""C:\ProgramData\Battle.net\Agent\Blizzard Uninstaller.exe" --lang=enUS --uid=battle.net --displayname="Battle.net""#
            )),
            auto("Battle.net")
        );
        assert_eq!(
            classify(&prog("EA app", "Electronic Arts", "MsiExec.exe /X{0}")),
            auto("EA app")
        );
        assert_eq!(
            classify(&prog("Origin 2024", "OriginLab Corporation", "MsiExec.exe /X{0}")),
            Category::Application
        );
    }

    #[test]
    fn web_apps() {
        let edge = prog(
            "Outlook (PWA)",
            "Microsoft Edge",
            r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --uninstall-app-id=faolnafnngnfdaknnbpnkhgohbobgegn --profile-directory=Default"#,
        );
        assert_eq!(
            classify(&edge),
            Category::WebApp {
                browser: "Microsoft Edge".into()
            }
        );
        let chrome = prog(
            "YouTube",
            "Google\\Chrome",
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --profile-directory=Default --uninstall-app-id=agimnkijcaahngcdmfeangaknmldooml"#,
        );
        assert_eq!(
            classify(&chrome),
            Category::WebApp {
                browser: "Google Chrome".into()
            }
        );
        let brave = prog(
            "Twitter",
            "",
            r#""C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe" --uninstall-app-id=abc"#,
        );
        assert_eq!(
            classify(&brave),
            Category::WebApp {
                browser: "Brave".into()
            }
        );
        let mut by_icon = prog("Some PWA", "", "");
        by_icon.display_icon = Some(
            r"C:\Users\me\AppData\Local\Microsoft\Edge\User Data\Default\Web Applications\_crx_abc\app.ico".into(),
        );
        assert_eq!(
            classify(&by_icon),
            Category::WebApp {
                browser: "Microsoft Edge".into()
            }
        );
    }

    #[test]
    fn self_updating() {
        let cases = [
            (
                prog(
                    "Google Chrome",
                    "Google LLC",
                    r#""C:\Program Files\Google\Chrome\Application\130.0\Installer\setup.exe" --uninstall"#,
                ),
                auto("Google Chrome"),
            ),
            (
                prog("Microsoft Edge", "Microsoft Corporation", "x"),
                auto("Microsoft Edge"),
            ),
            (
                prog(
                    "Mozilla Firefox (x64 fr)",
                    "Mozilla",
                    r#""C:\Program Files\Mozilla Firefox\uninstall\helper.exe""#,
                ),
                auto("Mozilla Firefox"),
            ),
            (
                prog(
                    "Microsoft 365 - fr-fr",
                    "Microsoft Corporation",
                    r#""C:\Program Files\Common Files\Microsoft Shared\ClickToRun\OfficeClickToRun.exe" scenario=install"#,
                ),
                auto("Microsoft Office Click-to-Run"),
            ),
            (
                prog(
                    "Discord",
                    "Discord Inc.",
                    r#""C:\Users\me\AppData\Local\Discord\Update.exe" --uninstall"#,
                ),
                auto("Discord"),
            ),
            (
                prog(
                    "Some Squirrel App",
                    "Acme",
                    r#""C:\Users\me\AppData\Local\acme\Update.exe" --uninstall"#,
                ),
                auto("Some Squirrel App"),
            ),
            (prog("Zoom Workplace", "Zoom", "x"), auto("Zoom")),
            (
                prog(
                    "Spotify",
                    "Spotify AB",
                    r"C:\Users\me\AppData\Roaming\Spotify\Spotify.exe /uninstall",
                ),
                auto("Spotify"),
            ),
            (
                prog(
                    "Adobe Photoshop 2025",
                    "Adobe Inc.",
                    r#""C:\Program Files (x86)\Common Files\Adobe\Adobe Desktop Common\HDBox\Uninstaller.exe" --uninstall=1 --sapCode=PHSP"#,
                ),
                auto("Adobe Creative Cloud"),
            ),
            (
                prog(
                    "IntelliJ IDEA Ultimate",
                    "JetBrains s.r.o.",
                    r#""C:\Users\me\AppData\Local\JetBrains\Toolbox\bin\jetbrains-toolbox.exe" uninstall --id=IDEA-U"#,
                ),
                auto("JetBrains Toolbox"),
            ),
            (
                prog("Microsoft Visual Studio Code (User)", "Microsoft Corporation", "x"),
                auto("Visual Studio Code"),
            ),
        ];
        for (p, expected) in cases {
            assert_eq!(classify(&p), expected, "{}", p.display_name);
        }
        assert_eq!(classify(&prog("ZoomIt", "Sysinternals", "x")), Category::Application);
    }

    #[test]
    fn drivers() {
        for (name, publisher) in [
            ("NVIDIA Graphics Driver 560.94", "NVIDIA Corporation"),
            ("NVIDIA PhysX System Software 9.23", "NVIDIA Corporation"),
            ("AMD Software", "Advanced Micro Devices, Inc."),
            ("Intel® Graphics Driver", "Intel Corporation"),
            ("Intel(R) Management Engine Components", "Intel Corporation"),
            ("Realtek High Definition Audio Driver", "Realtek Semiconductor Corp."),
            ("Synaptics Pointing Device Driver", "Synaptics Incorporated"),
            ("Logitech G HUB", "Logitech"),
            ("Razer Synapse", "Razer Inc."),
            ("AMD Chipset Software", ""),
            ("Windows Driver Package - Logitech (LGSHidFilt) HIDClass", "Logitech"),
            ("Dell SupportAssist", "Dell Inc."),
            ("HP Support Assistant", "HP Inc."),
            ("Samsung Magician", "Samsung Electronics"),
            ("Surface Dock Firmware Update", "Microsoft Corporation"),
            (
                "Package de pilotes Windows - Acme (acmeusb) USB  (01/02/2012 1.0)",
                "Acme",
            ),
            ("Windows-Treiberpaket - Acme (acmeusb) USB", "Acme"),
        ] {
            assert_eq!(classify(&prog(name, publisher, "x")), Category::Driver, "{name}");
        }
        for (name, publisher) in [
            ("Driver Booster 11", "IObit"),
            ("Microsoft ODBC Driver 18 for SQL Server", "Microsoft Corporation"),
            ("Windows Driver Kit - Windows 10.0.26100", "Microsoft Corporation"),
            ("NVIDIA CUDA Toolkit 12.4", "NVIDIA Corporation"),
            ("PDF24 Creator", "Geek Software GmbH"),
            ("OpenVINO AI Plugins for Some Editor", "Intel Corporation"),
        ] {
            assert_ne!(classify(&prog(name, publisher, "x")), Category::Driver, "{name}");
        }
    }

    #[test]
    fn system_components() {
        for (name, publisher) in [
            (
                "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.40.33810",
                "Microsoft Corporation",
            ),
            (
                "Microsoft Visual C++ 2013 x64 Minimum Runtime - 12.0.40664",
                "Microsoft Corporation",
            ),
            ("Microsoft .NET Host - 8.0.8 (x64)", "Microsoft Corporation"),
            (
                "Microsoft ASP.NET Core 8.0.8 Shared Framework (x64)",
                "Microsoft Corporation",
            ),
            (
                "Microsoft Windows Desktop Runtime - 8.0.8 (x64)",
                "Microsoft Corporation",
            ),
            (
                "Windows Software Development Kit - Windows 10.0.22621.3233",
                "Microsoft Corporation",
            ),
            ("Microsoft Edge WebView2 Runtime", "Microsoft Corporation"),
            ("DirectX for Managed Code Update (Summer 2004)", "Microsoft"),
            ("Java 8 Update 431 (64-bit)", "Oracle Corporation"),
            ("Java Auto Updater", "Oracle Corporation"),
            ("Eclipse Temurin JRE with Hotspot 21.0.4+7 (x64)", "Eclipse Adoptium"),
            ("Microsoft Update Health Tools", "Microsoft Corporation"),
            (
                "Update for Windows 10 for x64-based Systems (KB5001716)",
                "Microsoft Corporation",
            ),
            ("Mozilla Maintenance Service", "Mozilla"),
            ("Vulkan Run Time Libraries 1.0.54.0", "LunarG, Inc."),
            ("Microsoft ODBC Driver 18 for SQL Server", "Microsoft Corporation"),
            (
                "Microsoft System CLR Types for SQL Server 2019",
                "Microsoft Corporation",
            ),
        ] {
            assert_eq!(
                classify(&prog(name, publisher, "x")),
                Category::SystemComponent,
                "{name}"
            );
        }
        let mut hidden = prog(
            "Office 16 Click-to-Run Extensibility Component",
            "Microsoft Corporation",
            "MsiExec.exe /X{0}",
        );
        hidden.system_component = true;
        assert_eq!(classify(&hidden), Category::SystemComponent);
        let mut hotfix = prog("Security Update for Something (KB123)", "", "");
        hotfix.is_update = true;
        assert_eq!(classify(&hotfix), Category::SystemComponent);
    }

    #[test]
    fn applications() {
        for (name, publisher) in [
            ("7-Zip 24.08 (x64)", "Igor Pavlov"),
            ("VLC media player", "VideoLAN"),
            ("paint.net", "dotPDN LLC"),
            ("Notepad++ (64-bit x64)", "Notepad++ Team"),
            ("FileZilla 3.69.3", "Tim Kosse"),
            ("GIMP 2.10.38", "The GIMP Team"),
            ("Python 3.13.0 (64-bit)", "Python Software Foundation"),
            ("RustDesk", "RustDesk"),
        ] {
            assert_eq!(
                classify(&prog(name, publisher, r#""C:\Program Files\X\uninstall.exe""#)),
                Category::Application,
                "{name}"
            );
        }
    }

    /// Prints the classification of this PC's programs, for a manual sanity
    /// check: `cargo test -p majestic-core classify_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn classify_real_registry() {
        use std::collections::BTreeMap;
        let programs = crate::system::registry::installed_programs();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for p in &programs {
            let category = classify(p);
            let label = match &category {
                Category::AutoUpdating { by } => format!("auto_updating ({by})"),
                Category::Game { launcher } => format!("game ({launcher})"),
                Category::WebApp { browser } => format!("web_app ({browser})"),
                other => format!("{other:?}").to_lowercase(),
            };
            println!(
                "{:<55} {:<35} {}",
                p.display_name,
                p.publisher.as_deref().unwrap_or("-"),
                label
            );
            *counts
                .entry(label.split(' ').next().unwrap_or_default().to_string())
                .or_default() += 1;
        }
        println!("\n{} programs: {counts:?}", programs.len());
    }
}
