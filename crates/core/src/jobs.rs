//! Executes one installation job.
//!
//! The events emitted here are *raw*: `progress` is the download fraction
//! reported by the tool. The batch runner (window process) rewrites
//! `progress`/`speed`/`eta_secs` with its [`crate::eta::BatchTracker`], since
//! jobs may run in the elevated worker process.

use std::time::{Duration, Instant};

use crate::exec;
use crate::model::{FailureKind, JobEvent, JobRequest, JobState};
use crate::pkgmgr;
use crate::winget::codes::{self, Outcome};

const JOB_TIMEOUT: Duration = Duration::from_secs(45 * 60);
/// At most this many progress events per second.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const LOG_EXCERPT_LINES: usize = 15;

fn looks_like_url(line: &str) -> bool {
    line.contains("http://") || line.contains("https://")
}

/// Runs `job` to completion, emitting progress events. Never panics.
pub fn run_job(job: &JobRequest, emit: &mut dyn FnMut(JobEvent)) -> JobState {
    emit(JobEvent::state(&job.key, JobState::Starting));

    let spec = match pkgmgr::build_command(job) {
        Ok(spec) => spec,
        Err(e) => {
            log::warn!("job {}: cannot build command: {e:#}", job.key);
            return JobState::Failed { code: None, kind: FailureKind::PackageNotFound, log_excerpt: format!("{e:#}") };
        }
    };
    log::info!("job {}: {} {}", job.key, spec.program, spec.args.join(" "));

    let mut phase = JobState::Starting;
    let mut last_progress = Instant::now() - PROGRESS_INTERVAL;
    let mut saw_progress = false;

    let mut on_line = |line: &str| {
        if let Some(p) = exec::parse_progress(line) {
            saw_progress = true;
            if phase != JobState::Downloading {
                phase = JobState::Downloading;
            }
            let finished = p.fraction >= 0.999;
            if finished || last_progress.elapsed() >= PROGRESS_INTERVAL {
                last_progress = Instant::now();
                emit(JobEvent {
                    progress: Some(p.fraction),
                    downloaded: p.done_bytes,
                    total_bytes: p.total_bytes,
                    ..JobEvent::state(&job.key, JobState::Downloading)
                });
            }
            return;
        }

        // A non-progress line: download starts with the installer URL; any
        // other line after progress lines means the installer is running.
        let next = if phase == JobState::Starting && looks_like_url(line) {
            JobState::Downloading
        } else if (phase == JobState::Downloading && (saw_progress || !looks_like_url(line)))
            || (phase == JobState::Starting && saw_progress)
        {
            JobState::Installing
        } else {
            phase.clone()
        };
        if next != phase {
            phase = next;
        }
        emit(JobEvent { message: Some(line.to_string()), ..JobEvent::state(&job.key, phase.clone()) });
    };

    let (code, lines) = match exec::run_streaming(&spec, JOB_TIMEOUT, &mut on_line) {
        Ok(result) => result,
        Err(e) => {
            let text = format!("{e:#}");
            log::warn!("job {}: {text}", job.key);
            let kind = if text.to_lowercase().contains("timed out") || text.to_lowercase().contains("timeout") {
                FailureKind::Timeout
            } else {
                FailureKind::Unknown
            };
            return JobState::Failed { code: None, kind, log_excerpt: text };
        }
    };

    let outcome = codes::classify(job.source, code);
    log::info!("job {}: exit code {code:#x} → {outcome:?}", job.key);
    match outcome {
        Outcome::Success { reboot_required } => JobState::Succeeded { reboot_required },
        Outcome::Failure(kind) => {
            let start = lines.len().saturating_sub(LOG_EXCERPT_LINES);
            JobState::Failed { code: Some(code), kind, log_excerpt: lines[start..].join("\n") }
        }
    }
}
