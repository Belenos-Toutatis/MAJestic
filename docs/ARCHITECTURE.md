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
| `app_info` | – | `{ appVersion, uiLanguage, elevated, adminMember, dataDir }` |
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

### Events

| Event | Payload |
|---|---|
| `scan-progress` | `ScanProgress` |
| `item-updated` | `UpdateItem` (replace the item with the same `key`) |
| `job-progress` | `JobEvent` |
| `queue-progress` | `QueueProgress` |
| `updates-done` | `{ rebootRequired: bool, elevationRefused: bool, results: { key, state: JobState }[] }` |

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
