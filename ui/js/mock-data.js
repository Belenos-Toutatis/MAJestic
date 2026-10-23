// Demo dataset for the mock backend (same JSON shapes as crates/core/src/model.rs).

const MB = 1024 * 1024;

function item(o) {
  return {
    key: o.key,
    source: 'winget',
    id: o.id,
    name: o.name,
    publisher: null,
    installedVersion: null,
    versionCertainty: 'exact',
    availableVersion: null,
    kind: 'up_to_date',
    category: { type: 'application' },
    scope: null,
    explicitOnly: false,
    pinned: false,
    installLocation: null,
    iconHint: null,
    links: { homepage: null, download: null, releaseNotes: null, support: null },
    releaseNotes: null,
    downloadSize: null,
    installerType: null,
    assessment: { priority: 'normal', preselect: false, reasons: [] },
    alternatives: [],
    running: [],
    ignored: false,
    // Registry entry with an uninstaller (every mock program has one).
    uninstallKey: `HKLM64\\${o.id}`,
    ...o,
  };
}

const PF = 'C:\\Program Files';
const LOCAL = 'C:\\Users\\demo\\AppData\\Local';

export function buildDataset() {
  return [
    // ------------------------------------------------------------ updates
    item({
      key: 'winget:Google.Chrome', id: 'Google.Chrome', name: 'Google Chrome', publisher: 'Google LLC',
      installedVersion: '140.0.7339.208', availableVersion: '141.0.7390.123', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\Google\\Chrome\\Application`, iconHint: `${PF}\\Google\\Chrome\\Application\\chrome.exe,0`,
      links: { homepage: 'https://www.google.com/chrome/', download: 'https://www.google.com/chrome/', releaseNotes: 'https://chromereleases.googleblog.com/', support: 'https://support.google.com/chrome' },
      releaseNotes: 'Stable channel update for desktop.\n\nThis release contains 9 security fixes. Google is aware that an exploit for CVE-2025-6554 exists in the wild.\n\n• [High] Type confusion in V8\n• [High] Use after free in Media\n• [Medium] Inappropriate implementation in Autofill\n\nVarious fixes from internal audits, fuzzing and other initiatives.',
      downloadSize: 118 * MB, installerType: 'wix',
      assessment: {
        priority: 'critical', preselect: true, reasons: [
          { type: 'known_exploited', cves: ['CVE-2025-6554', 'CVE-2025-5419', 'CVE-2025-4664', 'CVE-2025-2783', 'CVE-2024-7971'], mostRecent: '2025-07-02', recent: true, ransomware: true },
          { type: 'exposed', category: 'browser' },
        ],
      },
      alternatives: [{ type: 'install_version' }],
    }),
    // Installed by Windows with its own setup.exe: winget only has the MSI.
    item({
      key: 'winget:Microsoft.Edge', id: 'Microsoft.Edge', name: 'Microsoft Edge', publisher: 'Microsoft Corporation',
      installedVersion: '155.0.4283.62', availableVersion: '156.0.4314.42', kind: 'update', scope: 'machine',
      installLocation: `${PF} (x86)\\Microsoft\\Edge\\Application`,
      links: { homepage: 'https://www.microsoft.com/edge', download: null, releaseNotes: null, support: null },
      downloadSize: 182 * MB, installerType: 'msi',
      assessment: {
        priority: 'recommended', preselect: false, reasons: [
          { type: 'exposed', category: 'browser' },
          { type: 'technology_mismatch' },
        ],
      },
      alternatives: [{ type: 'install_version' }, { type: 'self_update', app: 'Microsoft Edge' }],
    }),
    item({
      key: 'winget:7zip.7zip', id: '7zip.7zip', name: '7-Zip', publisher: 'Igor Pavlov',
      installedVersion: '24.08', availableVersion: '25.01', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\7-Zip`, iconHint: `${PF}\\7-Zip\\7zFM.exe,0`,
      links: { homepage: 'https://www.7-zip.org/', download: 'https://www.7-zip.org/download.html', releaseNotes: 'https://www.7-zip.org/history.txt', support: null },
      releaseNotes: '25.01\n- The code for handling symbolic links was changed to provide better security when extracting archives.\n- Fixed a vulnerability (CVE-2025-0411): Mark-of-the-Web was not propagated to files extracted from nested archives.\n- Some bugs were fixed.',
      downloadSize: Math.round(1.9 * MB), installerType: 'msi',
      assessment: {
        priority: 'critical', preselect: true, reasons: [
          { type: 'security_fix', keywords: ['CVE-2025-0411', 'Mark-of-the-Web', 'vulnerability'] },
          { type: 'exposed', category: 'archiver' },
        ],
      },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:Audacity.Audacity', id: 'Audacity.Audacity', name: 'Audacity', publisher: 'Audacity Team',
      installedVersion: '3.7.9', availableVersion: '4.0.1', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\Audacity`, iconHint: `${PF}\\Audacity\\Audacity.exe,0`,
      links: { homepage: 'https://www.audacityteam.org/', download: 'https://www.audacityteam.org/download/', releaseNotes: 'https://support.audacityteam.org/additional-resources/changelog', support: 'https://support.audacityteam.org/' },
      releaseNotes: 'Audacity 4 is a major release: redesigned interface, a new effects panel and non-destructive clip editing.\n\nSome older plug-ins and custom toolbars are no longer supported; projects saved with Audacity 4 cannot be opened with 3.x.',
      downloadSize: 46 * MB, installerType: 'inno',
      assessment: { priority: 'major', preselect: false, reasons: [{ type: 'major_upgrade', from: '3.7.9', to: '4.0.1' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:VideoLAN.VLC', id: 'VideoLAN.VLC', name: 'VLC media player', publisher: 'VideoLAN',
      installedVersion: '3.0.21', availableVersion: '3.0.22', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\VideoLAN\\VLC`, iconHint: `${PF}\\VideoLAN\\VLC\\vlc.exe,0`,
      links: { homepage: 'https://www.videolan.org/vlc/', download: 'https://www.videolan.org/vlc/download-windows.html', releaseNotes: 'https://www.videolan.org/news.html', support: 'https://forum.videolan.org/' },
      releaseNotes: 'Bugfix release: improved playback of some MKV and MP4 files, updated codecs and libraries.',
      downloadSize: 42 * MB, installerType: 'wix',
      assessment: { priority: 'recommended', preselect: true, reasons: [{ type: 'exposed', category: 'media_player' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:Notepad++.Notepad++', id: 'Notepad++.Notepad++', name: 'Notepad++', publisher: 'Notepad++ Team',
      installedVersion: '8.7.1', availableVersion: '8.8.6', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\Notepad++`, iconHint: `${PF}\\Notepad++\\notepad++.exe,0`,
      links: { homepage: 'https://notepad-plus-plus.org/', download: 'https://notepad-plus-plus.org/downloads/', releaseNotes: 'https://notepad-plus-plus.org/news/', support: null },
      releaseNotes: 'Fix crash when opening very large files.\nUpdate Scintilla and Lexilla.\nImprove dark mode in dialogs.',
      downloadSize: Math.round(6.4 * MB), installerType: 'nullsoft',
      assessment: { priority: 'recommended', preselect: true, reasons: [{ type: 'exposed', category: 'dev_tool' }, { type: 'app_running', processes: ['notepad++.exe'] }] },
      alternatives: [{ type: 'install_version' }],
      running: [{ pid: 7720, name: 'notepad++.exe', exe: `${PF}\\Notepad++\\notepad++.exe`, hasWindow: true }],
    }),
    item({
      key: 'winget:Mozilla.Firefox', id: 'Mozilla.Firefox', name: 'Mozilla Firefox', publisher: 'Mozilla',
      installedVersion: '143.0.4', availableVersion: '144.0', kind: 'update', scope: 'machine', pinned: true,
      installLocation: `${PF}\\Mozilla Firefox`, iconHint: `${PF}\\Mozilla Firefox\\firefox.exe,0`,
      links: { homepage: 'https://www.mozilla.org/firefox/', download: 'https://www.mozilla.org/firefox/download/', releaseNotes: 'https://www.mozilla.org/firefox/releasenotes/', support: 'https://support.mozilla.org/' },
      downloadSize: 68 * MB, installerType: 'nullsoft',
      assessment: { priority: 'recommended', preselect: false, reasons: [{ type: 'exposed', category: 'browser' }, { type: 'pinned' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'msstore:9NKSQGP7F2NH', source: 'ms_store', id: '9NKSQGP7F2NH', name: 'WhatsApp', publisher: 'WhatsApp Inc.',
      installedVersion: '2.2547.3.0', availableVersion: '2.2550.2.0', kind: 'update', scope: 'user',
      iconHint: 'msix:5319275A.WhatsAppDesktop',
      links: { homepage: 'https://www.whatsapp.com/', download: null, releaseNotes: null, support: 'https://faq.whatsapp.com/' },
      downloadSize: 164 * MB, installerType: 'msstore',
      assessment: { priority: 'recommended', preselect: true, reasons: [{ type: 'exposed', category: 'communication' }] },
    }),
    item({
      key: 'winget:Nextcloud.NextcloudDesktop', id: 'Nextcloud.NextcloudDesktop', name: 'Nextcloud', publisher: 'Nextcloud GmbH',
      installedVersion: '3.17.2', availableVersion: '3.17.3', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\Nextcloud`, iconHint: `${PF}\\Nextcloud\\nextcloud.exe,0`,
      links: { homepage: 'https://nextcloud.com/', download: 'https://nextcloud.com/install/#install-clients', releaseNotes: 'https://github.com/nextcloud/desktop/releases', support: null },
      releaseNotes: 'Fix synchronisation of files with special characters.\nFix a crash when the server is unreachable at start-up.',
      downloadSize: 121 * MB, installerType: 'msi',
      assessment: { priority: 'recommended', preselect: true, reasons: [{ type: 'exposed', category: 'cloud_sync' }, { type: 'app_running', processes: ['nextcloud.exe'] }] },
      alternatives: [{ type: 'install_version' }],
      running: [{ pid: 9124, name: 'nextcloud.exe', exe: `${PF}\\Nextcloud\\nextcloud.exe`, hasWindow: false }],
    }),
    item({
      key: 'winget:Microsoft.DotNet.DesktopRuntime.8', id: 'Microsoft.DotNet.DesktopRuntime.8', name: 'Microsoft .NET Desktop Runtime 8', publisher: 'Microsoft Corporation',
      installedVersion: '8.0.20', availableVersion: '8.0.21', kind: 'update', scope: 'machine',
      links: { homepage: 'https://dotnet.microsoft.com/', download: 'https://dotnet.microsoft.com/download/dotnet/8.0', releaseNotes: 'https://github.com/dotnet/core/blob/main/release-notes/8.0/README.md', support: null },
      releaseNotes: '.NET 8.0.21 includes security and reliability fixes.',
      downloadSize: 55 * MB, installerType: 'burn',
      assessment: { priority: 'recommended', preselect: true, reasons: [{ type: 'exposed', category: 'runtime' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:Microsoft.VisualStudioCode', id: 'Microsoft.VisualStudioCode', name: 'Microsoft Visual Studio Code', publisher: 'Microsoft Corporation',
      installedVersion: '1.104.3', availableVersion: '1.105.1', kind: 'update', scope: 'user',
      installLocation: `${LOCAL}\\Programs\\Microsoft VS Code`, iconHint: `${LOCAL}\\Programs\\Microsoft VS Code\\Code.exe,0`,
      links: { homepage: 'https://code.visualstudio.com/', download: 'https://code.visualstudio.com/download', releaseNotes: 'https://code.visualstudio.com/updates', support: null },
      releaseNotes: 'Improvements to the terminal, chat and source control views.',
      downloadSize: 105 * MB, installerType: 'inno',
      // A broken winget manifest that ships the installed version again.
      assessment: { priority: 'normal', preselect: false, reasons: [{ type: 'stale_installer', installerVersion: '1.104.3' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:Xodo.PDFStudio', id: 'Xodo.PDFStudio', name: 'Xodo PDF Studio', publisher: 'Xodo Technologies',
      installedVersion: '2025.0.2', availableVersion: '2025.1.0', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\PDF Studio 2025`,
      links: { homepage: 'https://www.qoppa.com/pdfstudio/', download: 'https://www.qoppa.com/pdfstudio/download/', releaseNotes: null, support: null },
      downloadSize: 328 * MB, installerType: 'exe',
      assessment: { priority: 'normal', preselect: true, reasons: [] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:Git.Git', id: 'Git.Git', name: 'Git', publisher: 'The Git Development Community',
      installedVersion: '< 2.47.0', versionCertainty: 'approximate', availableVersion: '2.51.1', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\Git`, iconHint: `${PF}\\Git\\git-bash.exe,0`,
      links: { homepage: 'https://git-scm.com/', download: 'https://git-scm.com/download/win', releaseNotes: 'https://github.com/git-for-windows/git/releases', support: null },
      downloadSize: 66 * MB, installerType: 'inno',
      assessment: { priority: 'normal', preselect: true, reasons: [{ type: 'approximate_version' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'winget:Microsoft.PowerToys', id: 'Microsoft.PowerToys', name: 'PowerToys', publisher: 'Microsoft Corporation',
      installedVersion: '0.94.2', availableVersion: '0.95.1', kind: 'update', scope: 'machine', explicitOnly: true,
      iconHint: `${PF}\\PowerToys\\PowerToys.exe,0`,
      links: { homepage: 'https://learn.microsoft.com/windows/powertoys/', download: null, releaseNotes: 'https://github.com/microsoft/PowerToys/releases', support: null },
      downloadSize: 252 * MB, installerType: 'burn',
      assessment: { priority: 'normal', preselect: true, reasons: [{ type: 'requires_explicit' }] },
      alternatives: [{ type: 'install_version' }],
    }),
    item({
      key: 'choco:python313', source: 'chocolatey', id: 'python313', name: 'Python 3.13', publisher: 'Python Software Foundation',
      installedVersion: '3.13.7', availableVersion: '3.13.9', kind: 'update', scope: 'machine',
      installLocation: 'C:\\Python313',
      links: { homepage: 'https://www.python.org/', download: 'https://www.python.org/downloads/windows/', releaseNotes: 'https://docs.python.org/3.13/whatsnew/changelog.html', support: null },
      downloadSize: 28 * MB, installerType: 'exe',
      assessment: { priority: 'normal', preselect: true, reasons: [{ type: 'duplicate_manager', other: 'winget' }] },
    }),
    item({
      key: 'scoop:yt-dlp', source: 'scoop', id: 'yt-dlp', name: 'yt-dlp', publisher: null,
      installedVersion: '2025.09.26', availableVersion: '2025.10.22', kind: 'update', scope: 'user',
      installLocation: 'C:\\Users\\demo\\scoop\\apps\\yt-dlp\\current',
      links: { homepage: 'https://github.com/yt-dlp/yt-dlp', download: null, releaseNotes: 'https://github.com/yt-dlp/yt-dlp/releases', support: null },
      downloadSize: 18 * MB, installerType: 'portable',
      assessment: { priority: 'normal', preselect: true, reasons: [] },
    }),
    item({
      key: 'winget:Syncthing.Syncthing', id: 'Syncthing.Syncthing', name: 'Syncthing', publisher: 'The Syncthing Authors',
      installedVersion: null, versionCertainty: 'unknown', availableVersion: '2.0.10', kind: 'update', scope: 'user',
      links: { homepage: 'https://syncthing.net/', download: 'https://syncthing.net/downloads/', releaseNotes: 'https://github.com/syncthing/syncthing/releases', support: 'https://forum.syncthing.net/' },
      downloadSize: 12 * MB, installerType: 'zip',
      assessment: { priority: 'uncertain', preselect: false, reasons: [{ type: 'unknown_version' }] },
      alternatives: [{ type: 'force_reinstall' }, { type: 'install_version' }],
    }),
    item({
      key: 'winget:Greenshot.Greenshot', id: 'Greenshot.Greenshot', name: 'Greenshot', publisher: 'Greenshot',
      installedVersion: '1.2.10.6', versionCertainty: 'from_file', availableVersion: '1.3.301', kind: 'update', scope: 'machine',
      installLocation: `${PF}\\Greenshot`, iconHint: `${PF}\\Greenshot\\Greenshot.exe,0`,
      links: { homepage: 'https://getgreenshot.org/', download: 'https://getgreenshot.org/downloads/', releaseNotes: null, support: null },
      downloadSize: Math.round(3.1 * MB), installerType: 'inno',
      assessment: { priority: 'uncertain', preselect: false, reasons: [{ type: 'version_from_file', version: '1.2.10.6' }] },
      alternatives: [{ type: 'force_reinstall' }, { type: 'install_version' }],
    }),
    item({
      key: 'winget:Oracle.VirtualBox', id: 'Oracle.VirtualBox', name: 'Oracle VirtualBox', publisher: 'Oracle Corporation',
      installedVersion: '7.1.12', availableVersion: '7.2.4', kind: 'update', scope: 'machine', ignored: true,
      links: { homepage: 'https://www.virtualbox.org/', download: 'https://www.virtualbox.org/wiki/Downloads', releaseNotes: 'https://www.virtualbox.org/wiki/Changelog', support: null },
      downloadSize: 118 * MB, installerType: 'exe',
      assessment: { priority: 'normal', preselect: false, reasons: [] },
      alternatives: [{ type: 'install_version' }],
    }),

    // ------------------------------------------------------------ manual
    item({
      key: 'arp:HKLM\\IrfanView64', source: 'registry', id: 'IrfanView64', name: 'IrfanView 64', publisher: 'Irfan Skiljan',
      installedVersion: '4.67', kind: 'manual', scope: 'machine', installLocation: `${PF}\\IrfanView`,
      iconHint: `${PF}\\IrfanView\\i_view64.exe,0`,
      links: { homepage: 'https://www.irfanview.com/', download: null, releaseNotes: null, support: null },
      alternatives: [
        { type: 'latest_elsewhere', version: '4.72', origin: 'chocolatey', url: 'https://community.chocolatey.org/packages/irfanview' },
        { type: 'open_url', purpose: 'download', url: 'https://www.irfanview.com/64bit.htm' },
      ],
    }),

    // ------------------------------------------------------------ unmanaged
    item({ key: 'arp:HKLM\\HandBrake', source: 'registry', id: 'HandBrake', name: 'HandBrake', publisher: 'The HandBrake Team', installedVersion: '1.8.2', kind: 'unmanaged', scope: 'machine', installLocation: `${PF}\\HandBrake`, iconHint: `${PF}\\HandBrake\\HandBrake.exe,0` }),
    item({ key: 'arp:HKLM\\calibre', source: 'registry', id: 'calibre 64bit', name: 'calibre 64bit', publisher: 'Kovid Goyal', installedVersion: '7.26.0', kind: 'unmanaged', scope: 'machine', iconHint: `${PF}\\Calibre2\\calibre.exe,0` }),
    item({ key: 'arp:HKLM\\{CANON-IJSCAN}', source: 'registry', id: '{CANON-IJSCAN}', name: 'Canon IJ Scan Utility', publisher: 'Canon Inc.', installedVersion: '2.3.11', kind: 'unmanaged', scope: 'machine' }),
    item({ key: 'arp:HKLM\\PSP9', source: 'registry', id: 'PSP9', name: 'Jasc Paint Shop Pro 9', publisher: 'Jasc Software', installedVersion: '9.01', kind: 'unmanaged', scope: 'machine' }),
    item({ key: 'arp:HKCU\\Steam App 730', source: 'registry', id: 'Steam App 730', name: 'Counter-Strike 2', publisher: 'Valve', installedVersion: null, versionCertainty: 'unknown', kind: 'unmanaged', category: { type: 'game', launcher: 'Steam' }, alternatives: [{ type: 'managed_elsewhere', by: 'Steam' }] }),
    item({ key: 'arp:HKCU\\Steam App 1145350', source: 'registry', id: 'Steam App 1145350', name: 'Hades II', publisher: 'Supergiant Games', installedVersion: null, versionCertainty: 'unknown', kind: 'unmanaged', category: { type: 'game', launcher: 'Steam' }, alternatives: [{ type: 'managed_elsewhere', by: 'Steam' }] }),
    item({ key: 'arp:HKLM\\Fortnite', source: 'registry', id: 'Fortnite', name: 'Fortnite', publisher: 'Epic Games, Inc.', installedVersion: '37.40', kind: 'unmanaged', category: { type: 'game', launcher: 'Epic Games Launcher' }, alternatives: [{ type: 'managed_elsewhere', by: 'Epic Games Launcher' }] }),
    item({ key: 'arp:HKCU\\msedge-outlook', source: 'registry', id: 'msedge-outlook', name: 'Outlook (PWA)', publisher: 'Microsoft Edge', installedVersion: '1.0', kind: 'unmanaged', scope: 'user', category: { type: 'web_app', browser: 'Microsoft Edge' }, alternatives: [{ type: 'managed_elsewhere', by: 'Microsoft Edge' }] }),
    item({ key: 'arp:HKCU\\chrome-ytmusic', source: 'registry', id: 'chrome-ytmusic', name: 'YouTube Music', publisher: 'Google Chrome', installedVersion: '1.0', kind: 'unmanaged', scope: 'user', category: { type: 'web_app', browser: 'Google Chrome' }, alternatives: [{ type: 'managed_elsewhere', by: 'Google Chrome' }] }),
    item({ key: 'arp:HKCU\\Discord', source: 'registry', id: 'Discord', name: 'Discord', publisher: 'Discord Inc.', installedVersion: '1.0.9211', kind: 'unmanaged', scope: 'user', category: { type: 'auto_updating', by: 'Discord' }, iconHint: `${LOCAL}\\Discord\\app.ico`, alternatives: [{ type: 'managed_elsewhere', by: 'Discord' }] }),
    item({ key: 'arp:HKCU\\Spotify', source: 'registry', id: 'Spotify', name: 'Spotify', publisher: 'Spotify AB', installedVersion: '1.2.74.477', kind: 'unmanaged', scope: 'user', category: { type: 'auto_updating', by: 'Spotify' }, alternatives: [{ type: 'managed_elsewhere', by: 'Spotify' }] }),
    item({ key: 'arp:HKLM\\O365HomePremRetail', source: 'registry', id: 'O365HomePremRetail', name: 'Microsoft 365 Apps', publisher: 'Microsoft Corporation', installedVersion: '16.0.19231.20156', kind: 'unmanaged', scope: 'machine', category: { type: 'auto_updating', by: 'Microsoft Office Click-to-Run' }, alternatives: [{ type: 'managed_elsewhere', by: 'Microsoft Office Click-to-Run' }] }),
    item({ key: 'arp:HKLM\\{NVIDIA-DISPLAY}', source: 'registry', id: '{NVIDIA-DISPLAY}', name: 'NVIDIA Graphics Driver 581.57', publisher: 'NVIDIA Corporation', installedVersion: '581.57', kind: 'unmanaged', scope: 'machine', category: { type: 'driver' }, alternatives: [{ type: 'managed_elsewhere', by: 'NVIDIA App' }, { type: 'open_url', purpose: 'download', url: 'https://www.nvidia.com/Download/index.aspx' }] }),
    item({ key: 'arp:HKLM\\{REALTEK-HDA}', source: 'registry', id: '{REALTEK-HDA}', name: 'Realtek High Definition Audio Driver', publisher: 'Realtek Semiconductor Corp.', installedVersion: '6.0.9579.1', kind: 'unmanaged', scope: 'machine', category: { type: 'driver' }, alternatives: [{ type: 'managed_elsewhere', by: 'Windows Update' }] }),
    item({ key: 'arp:HKLM\\{INTEL-BT}', source: 'registry', id: '{INTEL-BT}', name: 'Intel® Wireless Bluetooth®', publisher: 'Intel Corporation', installedVersion: '23.100.0.4', kind: 'unmanaged', scope: 'machine', category: { type: 'driver' }, alternatives: [{ type: 'managed_elsewhere', by: 'Windows Update' }] }),
    item({ key: 'arp:HKLM\\{VCREDIST-X64}', source: 'registry', id: '{VCREDIST-X64}', name: 'Microsoft Visual C++ 2015-2022 Redistributable (x64)', publisher: 'Microsoft Corporation', installedVersion: '14.44.35211.0', kind: 'unmanaged', scope: 'machine', category: { type: 'system_component' }, alternatives: [{ type: 'managed_elsewhere', by: 'Windows Update' }] }),
    item({ key: 'arp:HKLM\\{VCREDIST-X86}', source: 'registry', id: '{VCREDIST-X86}', name: 'Microsoft Visual C++ 2015-2022 Redistributable (x86)', publisher: 'Microsoft Corporation', installedVersion: '14.44.35211.0', kind: 'unmanaged', scope: 'machine', category: { type: 'system_component' }, alternatives: [{ type: 'managed_elsewhere', by: 'Windows Update' }] }),
    item({ key: 'arp:HKLM\\WebView2', source: 'registry', id: 'Microsoft EdgeWebView', name: 'Microsoft Edge WebView2 Runtime', publisher: 'Microsoft Corporation', installedVersion: '141.0.3537.85', kind: 'unmanaged', scope: 'machine', category: { type: 'system_component' }, alternatives: [{ type: 'managed_elsewhere', by: 'Microsoft Edge Update' }] }),
    item({ key: 'msix:Microsoft.Windows.Photos', source: 'msix', id: 'Microsoft.Windows.Photos_8wekyb3d8bbwe', name: 'Microsoft Photos', publisher: 'Microsoft Corporation', installedVersion: '2025.11090.11001.0', kind: 'unmanaged', scope: 'user', category: { type: 'store_app' }, alternatives: [{ type: 'managed_elsewhere', by: 'Microsoft Store' }, { type: 'store_update' }] }),
    item({ key: 'msix:Microsoft.WindowsTerminal', source: 'msix', id: 'Microsoft.WindowsTerminal_8wekyb3d8bbwe', name: 'Windows Terminal', publisher: 'Microsoft Corporation', installedVersion: '1.23.12811.0', kind: 'unmanaged', scope: 'user', category: { type: 'store_app' }, alternatives: [{ type: 'managed_elsewhere', by: 'Microsoft Store' }, { type: 'store_update' }] }),
    item({ key: 'arp:HKCU\\LogiGHUB', source: 'registry', id: 'LogiGHUB', name: 'Logitech G HUB', publisher: 'Logitech', installedVersion: '2025.7.768359', kind: 'unmanaged', scope: 'machine', category: { type: 'auto_updating', by: 'Logitech G HUB' }, alternatives: [{ type: 'managed_elsewhere', by: 'Logitech G HUB' }] }),

    // ------------------------------------------------------------ up to date
    item({ key: 'winget:Microsoft.PowerShell', id: 'Microsoft.PowerShell', name: 'PowerShell 7', publisher: 'Microsoft Corporation', installedVersion: '7.5.3', kind: 'up_to_date', scope: 'machine', installerType: 'wix', alternatives: [{ type: 'install_version' }], links: { homepage: 'https://github.com/PowerShell/PowerShell', download: null, releaseNotes: null, support: null } }),
    item({ key: 'winget:KeePassXCTeam.KeePassXC', id: 'KeePassXCTeam.KeePassXC', name: 'KeePassXC', publisher: 'KeePassXC Team', installedVersion: '2.7.10', kind: 'up_to_date', scope: 'machine', installerType: 'wix', alternatives: [{ type: 'install_version' }] }),
    item({ key: 'winget:voidtools.Everything', id: 'voidtools.Everything', name: 'Everything', publisher: 'voidtools', installedVersion: '1.4.1.1028', kind: 'up_to_date', scope: 'machine', installerType: 'nullsoft', alternatives: [{ type: 'install_version' }] }),
    item({ key: 'winget:OBSProject.OBSStudio', id: 'OBSProject.OBSStudio', name: 'OBS Studio', publisher: 'OBS Project', installedVersion: '32.0.1', kind: 'up_to_date', scope: 'machine', installerType: 'nullsoft', alternatives: [{ type: 'install_version' }] }),
    item({ key: 'winget:Obsidian.Obsidian', id: 'Obsidian.Obsidian', name: 'Obsidian', publisher: 'Obsidian', installedVersion: '1.9.14', kind: 'up_to_date', scope: 'user', installerType: 'nullsoft', alternatives: [{ type: 'install_version' }] }),
    item({ key: 'winget:Mozilla.Thunderbird', id: 'Mozilla.Thunderbird', name: 'Mozilla Thunderbird', publisher: 'Mozilla', installedVersion: '144.0', kind: 'up_to_date', scope: 'machine', installerType: 'nullsoft', alternatives: [{ type: 'install_version' }] }),
  ];
}

/** Item changes emitted by the background lookup phase (`item-updated`). */
export function lookupUpdates(items) {
  const byKey = (k) => items.find((i) => i.key === k);
  const out = [];
  const hb = byKey('arp:HKLM\\HandBrake');
  if (hb) {
    out.push({
      ...hb, kind: 'manual',
      links: { ...hb.links, homepage: 'https://handbrake.fr/' },
      alternatives: [
        { type: 'latest_elsewhere', version: '1.10.2', origin: 'github', url: 'https://github.com/HandBrake/HandBrake/releases/latest' },
        { type: 'winget_candidate', id: 'HandBrake.HandBrake', name: 'HandBrake', version: '1.10.2' },
        { type: 'open_url', purpose: 'download', url: 'https://handbrake.fr/downloads.php' },
      ],
    });
  }
  const cal = byKey('arp:HKLM\\calibre');
  if (cal) {
    out.push({ ...cal, alternatives: [{ type: 'winget_candidate', id: 'calibre.calibre', name: 'calibre', version: '8.12.0' }] });
  }
  return out;
}

export const WINGET_CATALOG = [
  { name: 'HandBrake', id: 'HandBrake.HandBrake', version: '1.10.2', source: 'winget' },
  { name: 'calibre', id: 'calibre.calibre', version: '8.12.0', source: 'winget' },
  { name: 'IrfanView', id: 'IrfanSkiljan.IrfanView', version: '4.72', source: 'winget' },
  { name: 'IrfanView Plugins', id: 'IrfanSkiljan.IrfanView.PlugIns', version: '4.72', source: 'winget' },
  { name: 'Discord', id: 'Discord.Discord', version: '1.0.9211', source: 'winget' },
  { name: 'Spotify', id: 'Spotify.Spotify', version: '1.2.74.477', source: 'winget' },
  { name: 'Logitech G HUB', id: 'Logitech.GHUB', version: '2025.7.768359', source: 'winget' },
  { name: 'Canon IJ Scan Utility Lite', id: 'Canon.IJScanUtilityLite', version: '3.1.0', source: 'winget' },
  { name: 'Paint.NET', id: 'dotPDN.PaintDotNet', version: '5.1.9', source: 'winget' },
  { name: 'Paint Shop Pro (Corel)', id: 'Corel.PaintShopPro', version: '2023', source: 'msstore' },
];

/** Icon tiles for the mock `get_icon` (generic glyphs on colored tiles; null = no icon). */
const ICON_STYLES = [
  ['chrome.exe', '#1A73E8', 'globe'],
  ['7zfm.exe', '#2B2B2B', 'box'],
  ['audacity.exe', '#1E3A8A', 'wave'],
  ['vlc.exe', '#F07C00', 'play'],
  ['notepad++.exe', '#7CB342', 'lines'],
  ['firefox.exe', '#E55B13', 'globe'],
  ['whatsapp', '#25A244', 'bubble'],
  ['nextcloud.exe', '#0082C9', 'cloud'],
  ['code.exe', '#1F8AD2', 'code'],
  ['git-bash.exe', '#E44C30', 'branch'],
  ['powertoys.exe', '#3B4FB8', 'bolt'],
  ['greenshot.exe', '#3E9B35', 'camera'],
  ['i_view64.exe', '#C62828', 'eye'],
  ['handbrake.exe', '#B8862B', 'play'],
  ['calibre.exe', '#6B4E2E', 'book'],
  ['discord', '#5865F2', 'bubble'],
];

const GLYPHS = {
  globe: '<circle cx="16" cy="16" r="8" fill="none" stroke="#fff" stroke-width="2"/><path d="M8 16h16M16 8c3 3 3 13 0 16M16 8c-3 3-3 13 0 16" fill="none" stroke="#fff" stroke-width="1.6"/>',
  box: '<path d="M9 12l7-4 7 4v8l-7 4-7-4z" fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round"/><path d="M9 12l7 4 7-4M16 16v8" fill="none" stroke="#fff" stroke-width="1.6"/>',
  wave: '<path d="M8 16h2l2-6 3 12 3-9 2 5 2-2h2" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>',
  play: '<path d="M13 10v12l10-6z" fill="#fff"/>',
  lines: '<rect x="10" y="8" width="12" height="16" rx="2" fill="none" stroke="#fff" stroke-width="2"/><path d="M13 13h6M13 17h6M13 21h4" stroke="#fff" stroke-width="1.6"/>',
  bubble: '<path d="M9 15a7 6 0 1 1 3 5l-4 2 1-4a6 6 0 0 1 0-3z" fill="none" stroke="#fff" stroke-width="2" stroke-linejoin="round"/>',
  cloud: '<path d="M11 21a4 4 0 0 1-.5-8 5.5 5.5 0 0 1 10.6 1.4A3.5 3.5 0 0 1 21 21z" fill="#fff"/>',
  code: '<path d="M13 11l-5 5 5 5M19 11l5 5-5 5" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>',
  branch: '<circle cx="12" cy="10" r="2.2" fill="#fff"/><circle cx="12" cy="22" r="2.2" fill="#fff"/><circle cx="21" cy="13" r="2.2" fill="#fff"/><path d="M12 12v8M21 15c0 3-4 3-8 5" fill="none" stroke="#fff" stroke-width="1.8"/>',
  bolt: '<path d="M18 7l-8 11h6l-2 7 8-11h-6z" fill="#fff"/>',
  camera: '<rect x="8" y="11" width="16" height="11" rx="2" fill="none" stroke="#fff" stroke-width="2"/><circle cx="16" cy="16.5" r="3" fill="#fff"/><path d="M13 11l1.5-2h3L19 11" fill="none" stroke="#fff" stroke-width="1.6"/>',
  eye: '<path d="M7 16c2-3.5 5-5.5 9-5.5s7 2 9 5.5c-2 3.5-5 5.5-9 5.5s-7-2-9-5.5z" fill="none" stroke="#fff" stroke-width="2"/><circle cx="16" cy="16" r="2.6" fill="#fff"/>',
  book: '<path d="M9 9h6a2 2 0 0 1 2 2v12a2 2 0 0 0-2-2H9zM23 9h-6v14" fill="none" stroke="#fff" stroke-width="1.8" stroke-linejoin="round"/>',
};

export function mockIcon(hint) {
  const h = String(hint || '').toLowerCase();
  const style = ICON_STYLES.find(([needle]) => h.includes(needle));
  if (!style) return null;
  const [, color, glyph] = style;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".22"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient></defs><rect width="32" height="32" rx="7" fill="${color}"/><rect width="32" height="32" rx="7" fill="url(#g)"/>${GLYPHS[glyph]}</svg>`;
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}

export function initialHistory() {
  const now = Date.now();
  const at = (days, hours = 0, minutes = 0) => new Date(now - days * 86400000 - hours * 3600000 - minutes * 60000).toISOString();
  const e = (o) => ({
    date: o.date, key: o.key, source: o.source || 'winget', id: o.id, name: o.name,
    action: o.action || { type: 'upgrade' }, fromVersion: o.from ?? null, toVersion: o.to ?? null,
    result: o.result || 'succeeded', failure: o.failure ?? null, code: o.code ?? null,
    rebootRequired: !!o.reboot, downloadSecs: o.dl ?? null, installSecs: o.inst ?? null,
    downloadBytes: o.bytes ?? null, installerType: o.type ?? null,
  });
  return [
    e({ date: at(2, 3, 12), key: 'winget:Google.Chrome', id: 'Google.Chrome', name: 'Google Chrome', from: '140.0.7339.186', to: '140.0.7339.208', dl: 11.2, inst: 24.5, bytes: 117 * MB, type: 'wix' }),
    e({ date: at(2, 3, 11), key: 'winget:Microsoft.VisualStudioCode', id: 'Microsoft.VisualStudioCode', name: 'Microsoft Visual Studio Code', from: '1.104.2', to: '1.104.3', dl: 9.8, inst: 18.1, bytes: 104 * MB, type: 'inno' }),
    e({ date: at(2, 3, 10), key: 'winget:Zoom.Zoom', id: 'Zoom.Zoom', name: 'Zoom Workplace', from: '6.5.11', to: '6.6.0', result: 'failed', failure: 'app_running', code: -1978334969, dl: 6.1, inst: 3.2, type: 'msi' }),
    e({ date: at(9, 1, 40), key: 'winget:Mozilla.Firefox', id: 'Mozilla.Firefox', name: 'Mozilla Firefox', from: '143.0.1', to: '143.0.4', dl: 7.5, inst: 15.9, bytes: 67 * MB, type: 'nullsoft' }),
    e({ date: at(9, 1, 38), key: 'winget:Microsoft.PowerToys', id: 'Microsoft.PowerToys', name: 'PowerToys', from: '0.93.0', to: '0.94.2', dl: 21.3, inst: 48.2, bytes: 250 * MB, type: 'burn' }),
    e({ date: at(9, 1, 35), key: 'winget:Microsoft.DotNet.DesktopRuntime.8', id: 'Microsoft.DotNet.DesktopRuntime.8', name: 'Microsoft .NET Desktop Runtime 8', from: '8.0.19', to: '8.0.20', reboot: true, dl: 5.2, inst: 31.7, bytes: 55 * MB, type: 'burn' }),
    e({ date: at(9, 1, 34), key: 'winget:Audacity.Audacity', id: 'Audacity.Audacity', name: 'Audacity', from: '3.7.9', to: '4.0.0', result: 'cancelled' }),
    e({ date: at(23, 5, 2), key: 'winget:Notepad++.Notepad++', id: 'Notepad++.Notepad++', name: 'Notepad++', from: '8.6.9', to: '8.7.1', dl: 1.2, inst: 6.4, bytes: 6 * MB, type: 'nullsoft' }),
    e({ date: at(23, 5, 0), key: 'winget:VideoLAN.VLC', id: 'VideoLAN.VLC', name: 'VLC media player', from: '3.0.20', to: '3.0.21', dl: 4.4, inst: 12.3, bytes: 41 * MB, type: 'wix' }),
    e({ date: at(23, 4, 58), key: 'choco:python313', source: 'chocolatey', id: 'python313', name: 'Python 3.13', from: '3.13.5', to: '3.13.7', dl: 3.1, inst: 40.2, bytes: 27 * MB, type: 'exe' }),
    e({ date: at(23, 4, 55), key: 'winget:Microsoft.PowerToys', id: 'Microsoft.PowerToys', name: 'PowerToys', action: { type: 'install_version', version: '0.93.0' }, from: '0.93.1', to: '0.93.0', dl: 19.9, inst: 45.0, bytes: 248 * MB, type: 'burn' }),
    e({ date: at(41, 2, 0), key: 'winget:Syncthing.Syncthing', id: 'Syncthing.Syncthing', name: 'Syncthing', action: { type: 'force_reinstall' }, from: null, to: '2.0.8', result: 'skipped' }),
  ].sort((a, b) => b.date.localeCompare(a.date));
}

/** Release lists for list_versions (newest first). */
export const VERSION_LISTS = {
  '7zip.7zip': ['25.01', '25.00', '24.09', '24.08', '24.07', '24.06', '24.05', '23.01', '22.01'],
  'Google.Chrome': ['141.0.7390.123', '141.0.7390.108', '141.0.7390.66', '140.0.7339.208', '140.0.7339.186', '140.0.7339.128', '139.0.7258.155'],
  'Audacity.Audacity': ['4.0.1', '4.0.0', '3.7.9', '3.7.8', '3.7.7', '3.7.5', '3.6.4'],
  'VideoLAN.VLC': ['3.0.22', '3.0.21', '3.0.20', '3.0.19', '3.0.18'],
  'Microsoft.PowerToys': ['0.95.1', '0.95.0', '0.94.2', '0.94.1', '0.94.0', '0.93.1', '0.93.0', '0.92.1'],
  'Microsoft.VisualStudioCode': ['1.105.1', '1.105.0', '1.104.3', '1.104.2', '1.104.1', '1.104.0', '1.103.2'],
};
