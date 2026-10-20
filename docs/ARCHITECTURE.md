# MAJestic — architecture

MAJestic is a portable Windows 11 application (one `.exe`, no installer) that
finds and installs updates for the software on the PC.

```
MAJ/
├─ crates/core/   majestic-core: the engine (pure Rust library, no GUI)
│  ├─ model.rs        shared data model (serialized to the UI, camelCase JSON)
│  ├─ version.rs      winget-compatible version comparison
│  ├─ exec.rs         running external tools without console windows
│  ├─ winget/         winget CLI: table parser, commands, exit codes
│  ├─ pkgmgr/         Chocolatey, Scoop, Microsoft Store
│  ├─ system/         registry inventory, classification, file versions,
│  │                  icons, processes, elevation, shell helpers
│  ├─ intel/          CISA KEV, winget-pkgs manifests, exposure list,
│  │                  assessment (priority), lookups for unmanaged programs
│  ├─ scan.rs         orchestration → ScanResult
│  ├─ eta.rs          time estimates (installer size, bandwidth, history)
│  ├─ jobs.rs         runs one installation, records history
│  ├─ scheduler.rs    scheduled background check (Task Scheduler)
│  ├─ settings.rs / history.rs / paths.rs
├─ app/           majestic: Tauri 2 shell (commands, events, elevated worker)
└─ ui/            static web UI (HTML/CSS/vanilla JS modules, no build step)
```

Data lives in `%LOCALAPPDATA%\MAJestic` (settings, history, caches, logs).

## Privileges

The window runs **without** administrator rights. When updates start, jobs
that need elevation (machine-wide winget packages, Chocolatey, Store scan) are
sent to a second instance of the same exe started with UAC
(`MAJestic.exe --worker <pipe> <parent-pid>`), which talks to the window over a
named pipe (JSON lines). Per-user packages, Store apps and Scoop run in the
window process, never elevated. One UAC prompt per batch at most.

Programs that must be closed are closed and relaunched by the *window* process,
so they never come back elevated.

## UI ↔ backend API (Tauri 2)

The UI uses `window.__TAURI__.core.invoke(name, args)` and
`window.__TAURI__.event.listen(name, cb)` (`withGlobalTauri: true`). All
payloads follow `crates/core/src/model.rs` and `settings.rs` (camelCase).
Argument names are camelCase on the JS side.

### Commands

| Command | Arguments | Returns |
|---|---|---|
| `app_info` | – | `{ appVersion, uiLanguage, elevated, adminMember, dataDir, updatedFrom }` (`updatedFrom`: previous version right after a self-update) |
| `get_settings` | – | `Settings` |
| `save_settings` | `{ settings }` | `Settings` (schedule applied; error string on failure) |
| `scan` | – | `ScanResult` (emits `scan-progress` meanwhile; then starts the lookup phase in background, which emits `item-updated` and `scan-progress` with step `lookup`) |
| `get_icon` | `{ hint }` | `string \| null` (data URL, 32 px) |
| `list_versions` | `{ id, source }` | `string[]` newest first |
| `search_winget` | `{ query }` | `{ name, id, version, source }[]` |
| `running_apps` | `{ keys }` | `{ key, name, processes: RunningProcess[] }[]` (fresh check) |
| `estimate_jobs` | `{ jobs: JobRequest[] }` | `JobEstimate[]` |
| `start_updates` | `{ jobs: JobRequest[], closeApps: string[], allowElevation: bool }` | `null` immediately; progress through events |
| `cancel_updates` | – | `null` (stops after the current installation) |
| `get_history` | – | `HistoryEntry[]` most recent first |
| `clear_history` | – | `null` |
| `open_url` | `{ url }` | `null` |
| `open_store_updates` | – | `null` |
| `open_logs` | – | `null` |
| `get_windows_updates` | – | `WindowsUpdates \| null` (last answer of Windows Update, searched in the background after each scan) |
| `open_windows_update` | `{ page }` | `null`; opens Settings › Windows Update and starts a search (`check`), or the optional updates (`optional`) |
| `check_app_update` | – | `{ version, pageUrl, notes, size } \| null` (newer MAJestic on GitHub; `null` when offline or online sources are off) |
| `install_app_update` | – | never returns on success: the new version starts and this one exits; error `{ kind, message }` (`busy`, `not_checked`, `network`, `hash_mismatch`, `version_mismatch`, `write_denied`, `swap_failed`, `restart_failed`) |

### Events

| Event | Payload |
|---|---|
| `scan-progress` | `ScanProgress` |
| `item-updated` | `UpdateItem` (replace the item with the same `key`) |
| `job-progress` | `JobEvent` |
| `queue-progress` | `QueueProgress` |
| `updates-done` | `{ rebootRequired: bool, elevationRefused: bool, results: { key, state: JobState }[] }` |
| `windows-updates` | `WindowsUpdates` (`updates`, `rebootPending`, `lastSearch`, `lastInstall`, `online`, `error`, `checkedAt`); `scan-progress` reports the step `windows_update` meanwhile |
| `app-update-progress` | `{ phase: "download" \| "install" \| "restart", downloaded, total }` |

### Building a `JobRequest` from an `UpdateItem`

```js
{ key, source, id, name,
  action: { type: "upgrade" },            // or {type:"install_version", version}, {type:"force_reinstall"}, {type:"interactive"}
  fromVersion: item.installedVersion, toVersion: item.availableVersion,
  scope: item.scope,
  includeUnknown: item.versionCertainty !== "exact",
  includePinned: item.pinned,
  downloadSize: item.downloadSize, installerType: item.installerType }
```

## Time estimates

Before starting, `estimate_jobs` returns per-job download/install estimates:
download = installer size (HTTP HEAD at scan time) ÷ bandwidth (median of
past downloads on this PC, 4 MB/s by default); install = median of this
package's past installs on this PC, else a model by installer type and size.
During the batch, the backend refines them with the measured speed and a
calibration factor (actual ÷ estimated of finished jobs) and emits smoothed
`etaSecs` in `job-progress` and `queue-progress`.

## Self-update

`check_app_update` reads the latest release of `Belenos-Toutatis/MAJestic`
from the GitHub API (drafts and pre-releases ignored; assets accepted only
from this repository). `install_app_update` downloads `MAJestic.exe` next to
the running file as `MAJestic.exe.new`, and keeps it only if its SHA-256
equals the published `MAJestic.exe.sha256` and its file version is the
announced one. It then renames the running file to `MAJestic.exe.old`
(Windows allows it), the new one to `MAJestic.exe`, starts it with
`--after-update <pid> <previous version>` and exits. The new process waits
for the old one to exit (single-instance lock), deletes the `.old` file and
the UI announces the update. Settings and history live in
`%LOCALAPPDATA%\MAJestic` and are untouched. Until releases are code-signed,
the hash only protects against corrupted or swapped downloads, not against a
compromised GitHub account.

## Windows Update

After each scan (setting `windowsUpdate`), a background thread asks the
Windows Update Agent API (`wuapi.dll`, no administrator rights) for
`IsInstalled=0 and IsHidden=0` updates, online first and from Windows' cached
results when that fails (`system/windows_update.rs`). Classification uses the
stable category ids (security, critical, definition, driver, feature...),
`BrowseOnly` marks optional updates, `ISystemInformation.RebootRequired` a
pending restart. MAJestic never installs them: the UI's button opens
`ms-settings:windowsupdate-action` (or `-optionalupdates`), and Windows does the
rest. The scheduled check counts pending security updates as important.
