//! Curated list of Internet-exposed software families.
//!
//! OWNER: intel agent.

use crate::model::ExposureCategory;

/// One family. Name patterns are normalized words (lowercase, punctuation
/// replaced by spaces) that must appear contiguously in the program name:
/// `a b` anywhere, `^a b` at the start, `=a b` the whole name (ignoring
/// words containing digits). Id patterns match a winget id or one of its
/// dotted prefixes, case-insensitively (`Mozilla.Firefox` matches
/// `Mozilla.Firefox.ESR`, not `Mozilla.FirefoxPWA`).
struct Rule {
    category: ExposureCategory,
    names: &'static [&'static str],
    ids: &'static [&'static str],
    /// Words that disqualify this rule (updaters, helpers...).
    unless: &'static [&'static str],
}

use ExposureCategory as C;

/// Order matters: the first matching rule wins, so specific families come
/// before broad ones (`Chrome Remote Desktop` is remote access, not a browser).
const RULES: &[Rule] = &[
    Rule {
        category: C::RemoteAccess,
        names: &[
            "chrome remote desktop", "remote desktop", "teamviewer", "anydesk", "rustdesk", "^parsec", "splashtop",
            "realvnc", "vnc viewer", "vnc server", "tightvnc", "ultravnc", "tigervnc", "remote utilities", "dwservice",
            "dwagent", "screenconnect", "connectwise control", "logmein", "gotomypc", "goto resolve", "nomachine",
            "remotepc", "zoho assist", "^supremo", "ammyy", "radmin viewer", "radmin server", "mremoteng", "royal ts",
            "jump desktop", "simplehelp", "beyondtrust", "bomgar", "meshagent", "meshcentral", "quick assist",
            "ultraviewer", "iperius remote", "aeroadmin", "getscreen", "x2go", "=moonlight", "=sunshine", "steam link",
        ],
        ids: &["Google.ChromeRemoteDesktopHost", "TeamViewer", "AnyDesk", "RustDesk", "Parsec", "Splashtop"],
        unless: &[],
    },
    Rule {
        category: C::Vpn,
        names: &[
            "vpn", "openvpn", "wireguard", "nordvpn", "expressvpn", "protonvpn", "surfshark", "cyberghost",
            "private internet access", "windscribe", "forticlient", "globalprotect", "anyconnect",
            "cisco secure client", "pulse secure", "ivanti secure access", "zscaler", "tailscale", "zerotier",
            "softether", "hotspot shield", "tunnelbear", "ipvanish", "hide me", "pritunl", "netbird", "openconnect",
            "netextender", "cloudflare warp", "hamachi", "mullvad",
        ],
        ids: &["WireGuard", "OpenVPNTechnologies", "Tailscale", "ZeroTier", "Cloudflare.Warp"],
        unless: &[],
    },
    Rule {
        category: C::PasswordManager,
        names: &[
            "keepass", "keepassxc", "bitwarden", "1password", "lastpass", "dashlane", "enpass", "roboform", "nordpass",
            "keeper password manager", "keeper desktop", "proton pass", "password safe", "password manager",
            "sticky password", "zoho vault", "psono", "buttercup", "passbolt",
        ],
        ids: &["Bitwarden", "AgileBits.1Password", "DominikReichl.KeePass", "KeePassXCTeam.KeePassXC"],
        unless: &[],
    },
    Rule {
        category: C::Browser,
        names: &[
            "google chrome", "chromium", "firefox", "microsoft edge", "=brave", "=brave browser", "=brave beta",
            "=brave nightly", "^opera", "vivaldi", "waterfox", "librewolf", "tor browser", "pale moon", "palemoon",
            "seamonkey", "yandex browser", "=yandex", "floorp", "zen browser", "thorium", "maxthon", "cent browser",
            "slimjet", "epic privacy browser", "duckduckgo", "avast secure browser", "avg secure browser",
            "ccleaner browser", "srware iron", "midori", "k meleon", "basilisk", "mullvad browser",
            "ungoogled chromium", "ladybird",
        ],
        ids: &[
            "Google.Chrome", "Mozilla.Firefox", "Microsoft.Edge", "Brave.Brave", "Opera.Opera", "Opera.OperaGX",
            "Vivaldi.Vivaldi", "TheBrowserCompany.Arc", "Hibbiki.Chromium", "LibreWolf.LibreWolf", "Waterfox.Waterfox",
            "TorProject.TorBrowser", "Zen-Team.Zen-Browser", "Floorp.Floorp", "DuckDuckGo.DesktopBrowser",
        ],
        unless: &["update", "updater", "helper", "maintenance", "webview2", "webview", "remote", "driver", "extension"],
    },
    Rule {
        category: C::DocumentReader,
        names: &[
            "adobe acrobat", "acrobat reader", "adobe reader", "adobe digital editions", "foxit", "sumatrapdf",
            "sumatra pdf", "pdf xchange", "pdf reader", "pdf viewer", "pdf editor", "pdf architect", "nitro pdf",
            "nitro pro", "pdf24", "pdfgear", "pdfelement", "pdfsam", "xodo", "okular", "calibre", "kindle",
            "stdu viewer", "djvu",
        ],
        ids: &["Adobe.Acrobat", "SumatraPDF", "Foxit", "PDFgear", "geeksoftwareGmbH.PDF24Creator"],
        unless: &[],
    },
    Rule {
        category: C::Office,
        names: &[
            "microsoft office", "microsoft 365", "libreoffice", "openoffice", "wps office", "onlyoffice", "freeoffice",
            "softmaker office", "polaris office", "hancom office", "microsoft word", "microsoft excel",
            "microsoft powerpoint", "microsoft onenote", "microsoft visio", "microsoft project", "microsoft access",
            "microsoft publisher", "microsoft outlook",
        ],
        ids: &["TheDocumentFoundation.LibreOffice", "Apache.OpenOffice", "ONLYOFFICE", "Kingsoft.WPSOffice"],
        unless: &["proofing", "language", "mui", "extensibility"],
    },
    Rule {
        category: C::Communication,
        names: &[
            "zoom", "microsoft teams", "^teams", "skype", "slack", "discord", "telegram", "whatsapp", "^signal",
            "thunderbird", "betterbird", "mailbird", "em client", "the bat", "claws mail", "postbox", "outlook",
            "webex", "gotomeeting", "goto meeting", "jitsi", "viber", "wechat", "tencent qq", "kakaotalk",
            "ringcentral", "mattermost", "rocket chat", "ferdium", "franz", "rambox", "pidgin", "hexchat", "mirc",
            "mumble", "teamspeak", "beeper", "threema", "messenger", "trillian", "zulip", "jabber", "bluejeans",
            "lync", "=element", "=wire", "=line", "guilded",
        ],
        ids: &[
            "Zoom", "Microsoft.Teams", "SlackTechnologies.Slack", "Discord", "Telegram", "Readdle.Spark",
            "Element.Element", "Wire.Wire", "LINE.LINE", "OpenWhisperSystems.Signal", "Mozilla.Thunderbird",
        ],
        unless: &["player"],
    },
    Rule {
        category: C::Archiver,
        names: &[
            "7 zip", "7zip", "winrar", "unrar", "winzip", "peazip", "bandizip", "nanazip", "izarc", "jzip",
            "powerarchiver", "ashampoo zip", "free archiver", "archiver", "zipware", "explzh", "lhaplus",
        ],
        ids: &["7zip.7zip", "RARLab.WinRAR", "Giorgiotani.Peazip", "Bandisoft.Bandizip", "M2Team.NanaZip"],
        unless: &[],
    },
    Rule {
        category: C::MediaPlayer,
        names: &[
            "vlc", "media player", "mpc hc", "mpc be", "k lite", "codec pack", "lav filters", "potplayer", "kmplayer",
            "gom player", "zoom player", "^mpv", "smplayer", "aimp", "winamp", "foobar2000", "musicbee", "itunes",
            "quicktime", "realplayer", "real player", "kodi", "plex", "plexamp", "jellyfin", "spotify", "deezer",
            "tidal", "amazon music", "audacious", "clementine", "strawberry music player", "mediamonkey",
        ],
        ids: &["VideoLAN.VLC", "clsid2.mpc-hc", "Daum.PotPlayer", "CodecGuide.K-LiteCodecPack", "Spotify.Spotify"],
        unless: &[],
    },
    Rule {
        category: C::CloudSync,
        names: &[
            "onedrive", "dropbox", "google drive", "nextcloud", "owncloud", "syncthing", "synctrayzor", "icloud",
            "box drive", "box sync", "box tools", "megasync", "pcloud", "sync com", "tresorit", "seafile",
            "resilio sync", "insync", "proton drive", "sharefile", "egnyte", "koofr", "^filen", "icedrive",
            "jottacloud", "yandex disk", "goodsync", "synology drive", "qsync", "cryptomator", "spideroak", "idrive",
            "backblaze", "carbonite",
        ],
        ids: &[
            "Microsoft.OneDrive", "Dropbox.Dropbox", "Google.GoogleDrive", "Nextcloud.NextcloudDesktop",
            "Syncthing.Syncthing",
        ],
        unless: &[],
    },
    Rule {
        category: C::Network,
        names: &[
            "wireshark", "npcap", "winpcap", "nmap", "zenmap", "ngrok", "angry ip scanner", "advanced ip scanner",
            "advanced port scanner", "fiddler", "burp suite", "mitmproxy", "qbittorrent", "utorrent", "µtorrent",
            "bittorrent", "=transmission", "transmission qt", "deluge", "vuze", "tixati", "frostwire", "emule",
            "jdownloader", "free download manager", "internet download manager", "motrix", "eagleget",
            "filezilla server", "apache http server", "nginx", "xampp", "wampserver", "laragon", "bonjour",
            "netlimiter", "glasswire", "portmaster", "simplewall",
        ],
        ids: &["WiresharkFoundation.Wireshark", "Insecure.Npcap", "qBittorrent.qBittorrent"],
        unless: &[],
    },
    Rule {
        category: C::DevTool,
        names: &[
            "git", "github desktop", "github cli", "gitkraken", "sourcetree", "smartgit", "tortoisegit", "tortoisesvn",
            "tortoisehg", "mercurial", "node js", "nodejs", "nvm for windows", "^deno", "putty", "^kitty", "winscp",
            "filezilla", "cyberduck", "mobaxterm", "termius", "securecrt", "bitvise", "openssh", "postman", "insomnia",
            "docker desktop", "podman desktop", "rancher desktop", "vagrant", "visual studio code", "vscodium",
            "visual studio", "windsurf", "android studio", "intellij", "pycharm", "webstorm", "phpstorm", "^rider",
            "clion", "goland", "rubymine", "datagrip", "rustrover", "jetbrains", "notepad++", "aws cli",
            "aws command line", "azure cli", "google cloud sdk", "terraform", "heroku",
        ],
        ids: &[
            "Git.Git", "GitHub", "OpenJS.NodeJS", "PuTTY.PuTTY", "WinSCP.WinSCP", "TimKosse.FileZilla",
            "Microsoft.VisualStudioCode", "Fork.Fork", "Anysphere.Cursor", "Docker.DockerDesktop",
        ],
        unless: &[],
    },
    Rule {
        category: C::Runtime,
        names: &[
            "^java", "jdk", "jre", "openjdk", "temurin", "corretto", "zulu", "liberica", "graalvm", "semeru",
            "net runtime", "net desktop runtime", "windows desktop runtime", "net core", "net sdk", "net framework",
            "asp net", "dotnet", "python", "anaconda", "anaconda3", "miniconda", "miniconda3", "pypy", "ruby",
            "strawberry perl", "activeperl", "activestate perl", "^perl", "^php", "erlang", "powershell", "webview2",
            "adobe air", "flash player", "silverlight", "shockwave",
        ],
        ids: &[
            "Microsoft.DotNet", "Oracle.JavaRuntimeEnvironment", "Oracle.JDK", "EclipseAdoptium", "Amazon.Corretto",
            "Azul.Zulu", "Python.Python", "Microsoft.PowerShell", "Microsoft.EdgeWebView2Runtime",
        ],
        unless: &[],
    },
];

/// Family of a program from its name and (winget) id, if it is one of the
/// high-exposure families: browsers, PDF/document readers, Java/.NET/Python
/// runtimes, archivers, media players, mail/chat/visio, remote access, VPN,
/// password managers, office suites, dev tools that handle remote content
/// (Git, Node.js, PuTTY, WinSCP, FileZilla...), cloud sync clients, network
/// tools.
pub fn exposure(name: &str, id: &str) -> Option<ExposureCategory> {
    let words = words(name);
    let id = id.trim();
    RULES
        .iter()
        .find(|rule| {
            if rule.unless.iter().any(|u| words.iter().any(|w| w == u)) {
                return false;
            }
            rule.names.iter().any(|p| name_matches(&words, p)) || rule.ids.iter().any(|p| id_matches(id, p))
        })
        .map(|rule| rule.category)
}

/// Lowercase words; `®`, `™`, apostrophes removed; other punctuation splits.
fn words(s: &str) -> Vec<String> {
    let mut normalized = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '®' | '™' | '©' | '\'' | '’' => {}
            c if c.is_alphanumeric() || c == '+' => normalized.extend(c.to_lowercase()),
            _ => normalized.push(' '),
        }
    }
    normalized.split_whitespace().map(str::to_string).collect()
}

fn name_matches(words: &[String], pattern: &str) -> bool {
    let (mode, phrase) = match pattern.as_bytes().first() {
        Some(b'^') => ('^', &pattern[1..]),
        Some(b'=') => ('=', &pattern[1..]),
        _ => (' ', pattern),
    };
    let phrase: Vec<&str> = phrase.split(' ').filter(|p| !p.is_empty()).collect();
    if phrase.is_empty() {
        return false;
    }
    let same = |slice: &[String]| slice.iter().zip(&phrase).all(|(w, p)| w == p);
    match mode {
        '^' => words.len() >= phrase.len() && same(&words[..phrase.len()]),
        '=' => {
            let significant: Vec<String> =
                words.iter().filter(|w| !w.chars().any(|c| c.is_ascii_digit())).cloned().collect();
            significant.len() == phrase.len() && same(&significant)
        }
        _ => words.windows(phrase.len()).any(same),
    }
}

/// `id` equals `pattern` or starts with `pattern.` (case-insensitive).
fn id_matches(id: &str, pattern: &str) -> bool {
    if id.len() < pattern.len() || !id.is_char_boundary(pattern.len()) {
        return false;
    }
    let (head, tail) = id.split_at(pattern.len());
    head.eq_ignore_ascii_case(pattern) && (tail.is_empty() || tail.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(cases: &[(&str, &str, Option<ExposureCategory>)]) {
        for (name, id, expected) in cases {
            assert_eq!(exposure(name, id), *expected, "{name} / {id}");
        }
    }

    #[test]
    fn browsers() {
        check(&[
            ("Google Chrome", "Google.Chrome", Some(C::Browser)),
            ("Google Chrome Beta", "", Some(C::Browser)),
            ("Mozilla Firefox (x64 fr)", "Mozilla.Firefox.fr", Some(C::Browser)),
            ("Firefox Developer Edition", "", Some(C::Browser)),
            ("Microsoft Edge", "Microsoft.Edge", Some(C::Browser)),
            ("Brave", "Brave.Brave", Some(C::Browser)),
            ("Opera GX Stable 112.0.5197.53", "", Some(C::Browser)),
            ("Vivaldi", "", Some(C::Browser)),
            ("Tor Browser", "", Some(C::Browser)),
            ("Arc", "TheBrowserCompany.Arc", Some(C::Browser)),
            ("Yandex", "", Some(C::Browser)),
            // Not browsers.
            ("Chrome Remote Desktop Host", "Google.ChromeRemoteDesktopHost", Some(C::RemoteAccess)),
            ("Microsoft Edge WebView2 Runtime", "Microsoft.EdgeWebView2Runtime", Some(C::Runtime)),
            ("Microsoft Edge Update", "", None),
            ("Mozilla Maintenance Service", "", None),
            ("Google Update Helper", "", None),
            ("Yandex Disk", "", Some(C::CloudSync)),
            ("Brave Frontier Launcher", "", None),
            ("DB Browser for SQLite", "DBBrowserForSQLite.DBBrowserForSQLite", None),
        ]);
    }

    #[test]
    fn other_families() {
        check(&[
            ("Adobe Acrobat Reader DC", "Adobe.Acrobat.Reader.64-bit", Some(C::DocumentReader)),
            ("Foxit PDF Reader", "", Some(C::DocumentReader)),
            ("SumatraPDF", "", Some(C::DocumentReader)),
            ("calibre 64bit", "calibre.calibre", Some(C::DocumentReader)),
            ("Java 8 Update 401 (64-bit)", "Oracle.JavaRuntimeEnvironment", Some(C::Runtime)),
            ("Eclipse Temurin JDK with Hotspot 21.0.4+7 (x64)", "", Some(C::Runtime)),
            ("Microsoft Windows Desktop Runtime - 8.0.10 (x64)", "Microsoft.DotNet.DesktopRuntime.8", Some(C::Runtime)),
            ("Python 3.13.5 (64-bit)", "Python.Python.3.13", Some(C::Runtime)),
            ("PowerShell 7-x64", "Microsoft.PowerShell", Some(C::Runtime)),
            ("Minecraft: Java Edition", "", None),
            ("7-Zip 24.08 (x64)", "7zip.7zip", Some(C::Archiver)),
            ("WinRAR 7.01 (64-bit)", "RARLab.WinRAR", Some(C::Archiver)),
            ("VLC media player", "VideoLAN.VLC", Some(C::MediaPlayer)),
            ("K-Lite Codec Pack 18.6.0 Full", "", Some(C::MediaPlayer)),
            ("Zoom Player MAX", "", Some(C::MediaPlayer)),
            ("Spotify", "", Some(C::MediaPlayer)),
            ("Zoom Workplace", "Zoom.Zoom", Some(C::Communication)),
            ("Microsoft Teams", "Microsoft.Teams", Some(C::Communication)),
            ("Discord", "", Some(C::Communication)),
            ("Mozilla Thunderbird (x64 fr)", "Mozilla.Thunderbird.fr", Some(C::Communication)),
            ("Signal 7.30.0", "", Some(C::Communication)),
            ("TeamViewer", "TeamViewer.TeamViewer", Some(C::RemoteAccess)),
            ("AnyDesk", "", Some(C::RemoteAccess)),
            ("Microsoft Remote Desktop", "", Some(C::RemoteAccess)),
            ("OpenVPN 2.6.12-I001 amd64", "", Some(C::Vpn)),
            ("Proton VPN", "", Some(C::Vpn)),
            ("WireGuard", "WireGuard.WireGuard", Some(C::Vpn)),
            ("Radmin VPN 1.4", "", Some(C::Vpn)),
            ("KeePass Password Safe 2.57", "DominikReichl.KeePass", Some(C::PasswordManager)),
            ("KeePassXC", "", Some(C::PasswordManager)),
            ("Bitwarden", "", Some(C::PasswordManager)),
            ("Microsoft Office Professional Plus 2019 - fr-fr", "", Some(C::Office)),
            ("Microsoft 365 Apps for enterprise - en-us", "", Some(C::Office)),
            ("Microsoft Office Proofing Tools 2016 - Deutsch", "", None),
            ("LibreOffice 24.8.2.1", "TheDocumentFoundation.LibreOffice", Some(C::Office)),
            ("Git", "Git.Git", Some(C::DevTool)),
            ("GitHub Desktop", "GitHub.GitHubDesktop", Some(C::DevTool)),
            ("Node.js", "OpenJS.NodeJS.LTS", Some(C::DevTool)),
            ("PuTTY release 0.81 (64-bit)", "PuTTY.PuTTY", Some(C::DevTool)),
            ("WinSCP 6.3.5", "WinSCP.WinSCP", Some(C::DevTool)),
            ("FileZilla 3.67.1", "TimKosse.FileZilla.Client", Some(C::DevTool)),
            ("FileZilla Server", "", Some(C::Network)),
            ("Microsoft Visual Studio Code", "Microsoft.VisualStudioCode", Some(C::DevTool)),
            ("Notepad++ (64-bit x64)", "Notepad++.Notepad++", Some(C::DevTool)),
            ("Docker Desktop", "Docker.DockerDesktop", Some(C::DevTool)),
            ("Microsoft OneDrive", "Microsoft.OneDrive", Some(C::CloudSync)),
            ("Nextcloud", "Nextcloud.NextcloudDesktop", Some(C::CloudSync)),
            ("Syncthing", "", Some(C::CloudSync)),
            ("Google Drive", "Google.GoogleDrive", Some(C::CloudSync)),
            ("Wireshark 4.4.1 x64", "WiresharkFoundation.Wireshark", Some(C::Network)),
            ("qBittorrent", "", Some(C::Network)),
        ]);
    }

    #[test]
    fn unexposed_programs() {
        check(&[
            ("Audacity 3.7.1", "Audacity.Audacity", None),
            ("GIMP 2.10.38", "GIMP.GIMP", None),
            ("Oracle VirtualBox 7.1.4", "Oracle.VirtualBox", None),
            ("Steam", "Valve.Steam", None),
            ("OBS Studio", "OBSProject.OBSStudio", None),
            ("Microsoft Visual C++ 2015-2022 Redistributable (x64)", "", None),
            ("NVIDIA Graphics Driver 560.94", "", None),
            ("Paint.NET", "dotPDN.PaintDotNet", None),
            ("ZoomIt", "", None),
            ("", "", None),
        ]);
    }

    #[test]
    fn id_prefixes() {
        assert!(id_matches("Mozilla.Firefox.ESR", "Mozilla.Firefox"));
        assert!(id_matches("mozilla.firefox", "Mozilla.Firefox"));
        assert!(!id_matches("Mozilla.FirefoxPWA", "Mozilla.Firefox"));
        assert!(!id_matches("Moz", "Mozilla.Firefox"));
        assert!(!id_matches("Mozilla.Firefoé", "Mozilla.Firefox"));
    }
}
