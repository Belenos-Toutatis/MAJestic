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
| `app_info` | – | `{ appVersion, uiLanguage, elevated, adminMember, dataDir, updatedFrom, scheduleNotice, autoUpdatable }` (`autoUpdatable`: false for a debug build or an exe under cargo's `target` folder) (`updatedFrom`: previous version right after a self-update; `scheduleNotice`: `missing` when the scheduled check was turned off because its task disappeared, `moved` when the task runs another copy; the task itself is never rewritten at start) |
| `get_settings` | – | `Settings` |
| `save_settings` | `{ settings }` | `Settings` (`schedule` ignored: kept as stored) |
| `export_inventory` | `{ text: { title, filterName, fileName } }` | `{ path, packages, manual } \| null` ("Save as" dialog in Documents, then writes the `majestic-inventory` JSON: packages of winget, the Store, Chocolatey (`choco list -r`) and Scoop (`apps` folder), plus the other applications as `manual` entries with their link; error `no_scan`) |
| `open_self_update` | `{ key }` | opens the page where a browser updates itself (`self_update` alternative: `msedge.exe edge://settings/help`, Chrome, Brave); error `elevated` when MAJestic runs as administrator |
| `reveal_export` | – | shows the last exported file in Explorer |
| `import_inventory` | `{ text: { title, filterName } }` | `{ file, plan: { createdAt, toInstall, installed, manual, manualInstalled } } \| null` ("Open" dialog, file ≤ 5 MB, compared with the last scan by key, and by name without version for `manual`; errors `no_scan`, `invalid_file`) |
| `leftover_downloads` | – | `{ bytes, entries }` (installers left in `%TEMP%\WinGet\<id>.<version>`, `%TEMP%\chocolatey\<id>` and Scoop's `cache`; winget's own `cache` folder and loose log files are kept; entries younger than 10 min are skipped) |
| `clean_downloads` | – | `{ freed, removed, failed }`; error `busy` while a batch runs or `winget.exe` / `choco.exe` / `msiexec.exe` is running |
| `describe_item` | `{ request: { source, id, name, publisher, versions, lang } }` | `{ summary, description, locale, origin, url } \| null` (what the program does: MAJestic's own text for known programs, local Scoop manifest, winget-pkgs manifest in `fr-FR` when published else the default locale, Chocolatey catalog; online sources only when allowed) |
| `schedule_status` | – | `{ schedule, task, taskCommand, exe, locationWarning, nextRun, lastCheck }` (`task`: `ok`, `missing`, `other` (runs another copy), `unreadable`, `none`; `locationWarning`: `downloads` / `temp`; `nextRun`: local `YYYY-MM-DDTHH:MM:SS`; `lastCheck`: `{ at, updates, important, windowsSecurity, notified }` of the last `--check`) |
| `set_schedule` | `{ schedule: Schedule \| null }` | same as `schedule_status`; creates, updates or deletes the task for this copy: the only place where the task is written, always from a button |
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
| `run_uninstaller` | `{ key }` | `null`; starts the program's own uninstaller (registry entry `UpdateItem.uninstallKey`, read again from the registry; MSI `/I` becomes `/X`); errors `not_installed`, `no_uninstaller` |
| `open_installed_apps` | `{ classic }` | `null`; Settings › Installed apps, or the classic "Programs and Features" |
| `open_windows_update` | `{ page }` | `null`; opens Settings › Windows Update and starts a search (`check`), or the optional updates (`optional`) |
| `check_app_update` | – | `{ version, pageUrl, notes, size } \| null` (newer MAJestic on GitHub; `null` when none or online sources are off; error string when GitHub could not be asked) |
| `install_app_update` | – | never returns on success: the new version starts and this one exits; error `{ kind, message }` (`busy`, `not_checked`, `network`, `hash_mismatch`, `version_mismatch`, `write_denied`, `swap_failed`, `restart_failed`) |

### Events

| Event | Payload |
|---|---|
| `scan-progress` | `ScanProgress` |
| `item-updated` | `UpdateItem` (replace the item with the same `key`) |
| `job-progress` | `JobEvent` |
| `queue-progress` | `QueueProgress` |
| `updates-done` | `{ rebootRequired: bool, elevationRefused: bool, results: { key, state: JobState, failedAttempts? }[] }` (`failedAttempts`: after a failure, failed attempts at that version since the package's last success; from 3 (`history::FAILURE_LIMIT`) the scan adds the reason `repeated_failures { count }` and no longer preselects it) |
| `windows-updates` | `WindowsUpdates` (`updates`, `rebootPending`, `lastSearch`, `lastInstall`, `online`, `error`, `checkedAt`); `scan-progress` reports the step `windows_update` meanwhile |
| `app-update-progress` | `{ phase: "download" \| "install" \| "restart", downloaded, total }` |

### Building a `JobRequest` from an `UpdateItem`

```js
{ key, source, id, name,
  action: { type: "upgrade" },            // or {type:"install_version", version}, {type:"force_reinstall"}, {type:"interactive"}, {type:"uninstall"}, {type:"install"} (package not on this PC, from an imported list)
  fromVersion: item.installedVersion, toVersion: item.availableVersion,
  scope: item.scope,
  includeUnknown: item.versionCertainty !== "exact",
  includePinned: item.pinned,
  downloadSize: item.downloadSize, installerType: item.installerType }
```

`toVersion` also lets `run_job` retry a winget upgrade that ended in
`UPDATE_NOT_APPLICABLE` (`0x8A15002B`): when every installer of that version in
the winget-pkgs manifest has the same `InstallerLocale`, the command runs once
more with `--locale <it>`. winget reads the installed program's `Language`; an
MSI that sets it to 0 is taken for the user's language (fr-FR) and an
English-only installer is then rejected (Kieirra.Murmure 2.0.3). Only when
online sources are allowed; the manifest is usually already cached by the scan.

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
from this repository). The window calls it at start, then again when the last
check is 6 hours old (timer every 10 minutes, so a PC woken from sleep
catches up), and from Settings › About (« Rechercher une mise à jour »), which
shows the result and opens the update dialog when a version is found. `install_app_update` downloads `MAJestic.exe` next to
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

## Scheduled check

Turned on only by the user, from Settings (`set_schedule`): MAJestic never
creates or rewrites its task by itself, since Microsoft Defender treats an
unsigned program that installs a task restarting it as persistence. The task
"MAJestic UpdateCheck" (XML in UTF-16 LE, `StartWhenAvailable`) runs
`MAJestic.exe --check` daily, weekly on `Schedule.weekday` (1 = Monday) or
monthly on `Schedule.day` (1–28). The check scans without online lookups,
shows a toast if worth it, and writes its result to `last-check.json`, shown
in the settings' summary with the next run (`scheduler::next_run`). At start,
a vanished task turns the option off (`scheduleNotice: missing`).
