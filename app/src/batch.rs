//! Runs a batch of installations from the window process.
//!
//! Jobs run one at a time, in the order chosen by the UI. Jobs that need
//! administrator rights go to the elevated worker (one UAC prompt for the
//! whole batch), the others run here. Programs the user agreed to close are
//! closed right before their own update and relaunched right after it, from
//! this non-elevated process. A ticker re-emits progress every second so the
//! time left keeps counting down between output lines.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use majestic_core::eta::{BatchTracker, EtaModel, Phase};
use majestic_core::model::*;
use majestic_core::system::{elevation, procs};
use majestic_core::{history, jobs};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::AppState;
use crate::worker::{StartError, Worker};

const CLOSE_GRACE: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(500);

pub struct BatchControl {
    cancel: Arc<AtomicBool>,
}

impl BatchControl {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobResult {
    key: String,
    state: JobState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchDone {
    reboot_required: bool,
    elevation_refused: bool,
    results: Vec<JobResult>,
}

/// Starts a batch in the background. Fails if one is already running.
pub fn start(app: AppHandle, jobs: Vec<JobRequest>, close_apps: Vec<String>, allow_elevation: bool) -> Result<(), String> {
    if jobs.is_empty() {
        return Err("nothing to update".into());
    }
    let state = app.state::<AppState>();
    let mut slot = state.batch.lock().map_err(|e| e.to_string())?;
    if slot.is_some() {
        return Err("updates are already running".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *slot = Some(BatchControl { cancel: cancel.clone() });
    drop(slot);

    std::thread::spawn(move || {
        let done = run(&app, jobs, close_apps, allow_elevation, &cancel);
        if let Ok(mut slot) = app.state::<AppState>().batch.lock() {
            *slot = None;
        }
        let _ = app.emit("updates-done", done);
    });
    Ok(())
}

struct Shared {
    tracker: BatchTracker,
    /// Job in progress, to watch its download folder.
    job: Option<JobRequest>,
    /// Last event of the job in progress (re-emitted by the ticker).
    last_event: Option<JobEvent>,
    done: usize,
    succeeded: usize,
    failed: usize,
    total: usize,
    current: Option<String>,
}

impl Shared {
    /// Feeds the tracker with a raw event and fills progress/speed/ETA.
    fn enrich(&mut self, mut ev: JobEvent, job: &JobRequest) -> JobEvent {
        match ev.state {
            JobState::Downloading => {
                // Percent-only progress (choco...): derive bytes from the size.
                if ev.downloaded.is_none() {
                    if let (Some(f), Some(size)) = (ev.progress, job.download_size) {
                        ev.downloaded = Some((f as f64 * size as f64) as u64);
                        ev.total_bytes = Some(size);
                    }
                }
                ev.speed = self.tracker.download_progress(ev.downloaded, ev.total_bytes);
            }
            JobState::Installing => self.tracker.install_started(),
            _ => {}
        }
        if !ev.state.is_final() {
            if let Some((fraction, left)) = self.tracker.current_job() {
                ev.progress = Some(fraction);
                ev.eta_secs = Some(left);
            }
            // Keep the last message and byte counts when an event has none.
            if let Some(prev) = &self.last_event {
                if ev.message.is_none() {
                    ev.message = prev.message.clone();
                }
                if ev.state == JobState::Downloading && ev.total_bytes.is_none() {
                    ev.downloaded = prev.downloaded;
                    ev.total_bytes = prev.total_bytes;
                }
            }
            self.last_event = Some(ev.clone());
        } else {
            ev.progress = Some(1.0);
            ev.eta_secs = Some(0.0);
            self.last_event = None;
        }
        ev
    }

    fn queue(&mut self) -> QueueProgress {
        let (fraction, eta_secs) = self.tracker.queue();
        QueueProgress {
            done: self.done,
            total: self.total,
            succeeded: self.succeeded,
            failed: self.failed,
            fraction,
            elapsed_secs: self.tracker.elapsed(),
            eta_secs,
            current: self.current.clone(),
        }
    }
}

fn emit_job(app: &AppHandle, ev: &JobEvent) {
    let _ = app.emit("job-progress", ev);
}

fn emit_queue(app: &AppHandle, shared: &Mutex<Shared>) {
    if let Ok(mut s) = shared.lock() {
        let q = s.queue();
        let _ = app.emit("queue-progress", q);
    }
}

/// Executables that should not be relaunched by hand (helpers, updaters).
fn is_helper(exe: &std::path::Path) -> bool {
    let name = exe.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    ["helper", "crash", "update", "webengine", "renderer", "gpu", "broker", "uninst", "service", "elevat", "notifier"]
        .iter()
        .any(|w| name.contains(w))
}

/// Closes the item's running processes; returns the executables to relaunch.
fn close_programs(dirs: &[PathBuf]) -> Vec<PathBuf> {
    if dirs.is_empty() {
        return Vec::new();
    }
    let snapshot = procs::snapshot();
    let running = procs::running_under(dirs, &snapshot);
    if running.is_empty() {
        return Vec::new();
    }
    let pids: Vec<u32> = running.iter().map(|p| p.pid).collect();
    let survivors = procs::close(&pids, CLOSE_GRACE, true);
    if !survivors.is_empty() {
        log::warn!("processes still running after close: {survivors:?}");
    }
    let mut exes: Vec<PathBuf> = Vec::new();
    for p in running {
        let exe = PathBuf::from(&p.exe);
        if !is_helper(&exe) && !exes.iter().any(|e| e.as_os_str().eq_ignore_ascii_case(exe.as_os_str())) {
            exes.push(exe);
        }
    }
    exes
}

fn relaunch(exes: &[PathBuf]) {
    for exe in exes {
        if !exe.exists() {
            log::info!("not relaunching {}: file no longer exists", exe.display());
            continue;
        }
        if let Err(e) = procs::launch_unelevated(exe) {
            log::warn!("cannot relaunch {}: {e:#}", exe.display());
        }
    }
}

fn result_label(state: &JobState) -> &'static str {
    match state {
        JobState::Succeeded { .. } => "succeeded",
        JobState::Failed { .. } => "failed",
        JobState::Cancelled => "cancelled",
        JobState::Skipped { .. } => "skipped",
        _ => "unknown",
    }
}

fn record(job: &JobRequest, state: &JobState, t: majestic_core::eta::Timings) {
    let (failure, code, reboot_required) = match state {
        JobState::Failed { code, kind, .. } => (Some(*kind), *code, false),
        JobState::Succeeded { reboot_required } => (None, Some(0), *reboot_required),
        _ => (None, None, false),
    };
    history::append(&HistoryEntry {
        date: chrono::Local::now().to_rfc3339(),
        key: job.key.clone(),
        source: job.source,
        id: job.id.clone(),
        name: job.name.clone(),
        action: job.action.clone(),
        from_version: job.from_version.clone(),
        to_version: match &job.action {
            JobAction::InstallVersion { version } => Some(version.clone()),
            _ => job.to_version.clone(),
        },
        result: result_label(state).into(),
        failure,
        code,
        reboot_required,
        duration_secs: t.total_secs,
        download_secs: t.download_secs,
        install_secs: t.install_secs,
        download_bytes: t.download_bytes.or(job.download_size),
        installer_type: job.installer_type.clone(),
    });
}

fn run(app: &AppHandle, jobs: Vec<JobRequest>, close_apps: Vec<String>, allow_elevation: bool, cancel: &AtomicBool) -> BatchDone {
    let model = EtaModel::from_history(&history::load());
    let estimates = jobs.iter().map(|j| model.estimate(j)).collect();
    let shared = Arc::new(Mutex::new(Shared {
        tracker: BatchTracker::new(estimates, model.bandwidth),
        job: None,
        last_event: None,
        done: 0,
        succeeded: 0,
        failed: 0,
        total: jobs.len(),
        current: None,
    }));

    for job in &jobs {
        emit_job(app, &JobEvent::state(&job.key, JobState::Queued));
    }
    emit_queue(app, &shared);

    // Folders of each program, from the last scan (to find its processes).
    let dirs_of = |key: &str| -> Vec<PathBuf> {
        app.state::<AppState>()
            .last_scan
            .lock()
            .ok()
            .and_then(|s| s.as_ref().and_then(|s| s.items.iter().find(|i| i.key == key).map(|i| i.install_dirs.clone())))
            .unwrap_or_default()
    };

    // Ticker: measures the download (winget prints no progress when its
    // output is redirected) and keeps the time left moving between lines.
    let stop = Arc::new(AtomicBool::new(false));
    let ticker = {
        let (app, shared, stop) = (app.clone(), shared.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(TICK);
                let job = shared.lock().ok().and_then(|s| s.job.clone());
                let downloading = shared.lock().ok().and_then(|s| s.tracker.phase()) == Some(Phase::Downloading);
                let bytes = job.as_ref().filter(|_| downloading).and_then(jobs::downloaded_bytes);
                if let Ok(mut s) = shared.lock() {
                    if let (Some(bytes), Some(job)) = (bytes, job.as_ref()) {
                        let total = job.download_size.filter(|t| *t >= bytes);
                        let speed = s.tracker.download_progress(Some(bytes), total);
                        if let Some(ev) = s.last_event.as_mut() {
                            ev.downloaded = Some(bytes);
                            ev.total_bytes = total;
                            ev.speed = speed;
                        }
                    }
                    if let Some(mut ev) = s.last_event.clone() {
                        if let Some((fraction, left)) = s.tracker.current_job() {
                            ev.progress = Some(fraction);
                            ev.eta_secs = Some(left);
                        }
                        emit_job(&app, &ev);
                    }
                    let q = s.queue();
                    let _ = app.emit("queue-progress", q);
                }
            }
        })
    };

    // One UAC prompt for the whole batch, only if needed.
    let mut worker: Option<Worker> = None;
    let mut elevation_refused = false;
    let need_worker = allow_elevation && !elevation::is_elevated() && jobs.iter().any(|j| j.needs_elevation());
    if need_worker {
        match Worker::start() {
            Ok(w) => worker = Some(w),
            Err(StartError::Refused) => {
                log::info!("elevation refused by the user");
                elevation_refused = true;
            }
            Err(StartError::Other(e)) => log::warn!("elevated helper unavailable, running locally: {e}"),
        }
    }

    let mut results = Vec::new();
    let mut reboot_required = false;
    for job in &jobs {
        if cancel.load(Ordering::SeqCst) {
            let ev = JobEvent::state(&job.key, JobState::Cancelled);
            emit_job(app, &ev);
            record(job, &JobState::Cancelled, Default::default());
            results.push(JobResult { key: job.key.clone(), state: JobState::Cancelled });
            continue;
        }

        if let Ok(mut s) = shared.lock() {
            s.current = Some(job.key.clone());
            s.job = Some(job.clone());
            s.tracker.job_started(&job.key);
            s.last_event = None;
        }
        emit_queue(app, &shared);

        let mut to_relaunch = Vec::new();
        if close_apps.contains(&job.key) {
            let ev = JobEvent::state(&job.key, JobState::ClosingApps);
            let ev = shared.lock().map(|mut s| s.enrich(ev.clone(), job)).unwrap_or(ev);
            emit_job(app, &ev);
            to_relaunch = close_programs(&dirs_of(&job.key));
        }

        let mut on_event = |ev: JobEvent| {
            let ev = shared.lock().map(|mut s| s.enrich(ev.clone(), job)).unwrap_or(ev);
            emit_job(app, &ev);
        };

        let state = if job.needs_elevation() && need_worker {
            match worker.as_mut() {
                Some(w) => {
                    let state = w.run(job, &mut on_event);
                    if !w.is_alive() {
                        log::warn!("elevated helper stopped");
                        worker = None;
                    }
                    state
                }
                None if elevation_refused => JobState::Failed {
                    code: None,
                    kind: FailureKind::ElevationRefused,
                    log_excerpt: String::new(),
                },
                None => jobs::run_job(job, &mut on_event),
            }
        } else {
            jobs::run_job(job, &mut on_event)
        };

        let success = matches!(state, JobState::Succeeded { .. });
        if let JobState::Succeeded { reboot_required: true } = state {
            reboot_required = true;
        }
        let timings = match shared.lock() {
            Ok(mut s) => {
                let t = s.tracker.job_finished(&job.key, success);
                s.done += 1;
                if success {
                    s.succeeded += 1;
                } else {
                    s.failed += 1;
                }
                t
            }
            Err(_) => Default::default(),
        };
        let final_ev = shared
            .lock()
            .map(|mut s| s.enrich(JobEvent::state(&job.key, state.clone()), job))
            .unwrap_or_else(|_| JobEvent::state(&job.key, state.clone()));
        emit_job(app, &final_ev);
        emit_queue(app, &shared);
        record(job, &state, timings);
        relaunch(&to_relaunch);
        results.push(JobResult { key: job.key.clone(), state });
    }

    if let Some(w) = worker.take() {
        w.quit();
    }
    stop.store(true, Ordering::SeqCst);
    let _ = ticker.join();
    if let Ok(mut s) = shared.lock() {
        s.current = None;
    }
    emit_queue(app, &shared);

    BatchDone { reboot_required, elevation_refused, results }
}
