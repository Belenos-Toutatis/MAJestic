//! Executes one installation job.
//!
//! The events emitted here are *raw*: `progress` is the download fraction
//! reported by the tool. The batch runner (window process) rewrites
//! `progress`/`speed`/`eta_secs` with its [`crate::eta::BatchTracker`], since
//! jobs may run in the elevated worker process.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{exec, version, winget};
use crate::model::{FailureKind, JobAction, JobEvent, JobRequest, JobState, Source};
use crate::pkgmgr;
use crate::winget::codes::{self, Outcome};

const JOB_TIMEOUT: Duration = Duration::from_secs(45 * 60);
/// At most this many progress events per second.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
const LOG_EXCERPT_LINES: usize = 15;

fn looks_like_url(line: &str) -> bool {
    line.contains("http://") || line.contains("https://")
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Bytes downloaded so far for a job, read from the tool's download folder.
///
/// winget prints no progress when its output is redirected, but downloads
/// into `%TEMP%\WinGet\<id>.<version>\`; Chocolatey into
/// `%TEMP%\chocolatey\<id>\`. The elevated worker runs as the same user, so
/// the window process can watch these folders for both.
pub fn downloaded_bytes(job: &JobRequest) -> Option<u64> {
    let temp = std::env::temp_dir();
    let dir: PathBuf = match job.source {
        Source::Winget | Source::Registry => {
            let base = temp.join("WinGet");
            let exact = job
                .to_version
                .as_ref()
                .or(match &job.action {
                    JobAction::InstallVersion { version } => Some(version),
                    _ => None,
                })
                .map(|v| base.join(format!("{}.{v}", job.id)));
            match exact.filter(|d| d.is_dir()) {
                Some(d) => d,
                // Version unknown: the most recent folder of this package.
                None => {
                    let prefix = format!("{}.", job.id.to_lowercase());
                    std::fs::read_dir(&base)
                        .ok()?
                        .flatten()
                        .filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with(&prefix))
                        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?
                        .path()
                }
            }
        }
        Source::Chocolatey => temp.join("chocolatey").join(&job.id),
        _ => return None,
    };
    dir.is_dir().then(|| dir_size(&dir)).filter(|s| *s > 0)
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
            let kind = if e.downcast_ref::<exec::TimedOut>().is_some() {
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
        Outcome::Success { reboot_required } => {
            // After a reboot the version may well change: no check then.
            match if reboot_required { None } else { unchanged_version(job) } {
                Some(still) => {
                    log::warn!("job {}: the installer succeeded but the installed version is still {still}", job.key);
                    JobState::Failed {
                        code: Some(code),
                        kind: FailureKind::NoEffect,
                        log_excerpt: format!("The installer finished without error, but Windows still reports version {still}."),
                    }
                }
                None => JobState::Succeeded { reboot_required },
            }
        }
        Outcome::Failure(kind) => {
            let start = lines.len().saturating_sub(LOG_EXCERPT_LINES);
            JobState::Failed { code: Some(code), kind, log_excerpt: lines[start..].join("\n") }
        }
    }
}

/// Waits before each check of [`unchanged_version`]: some installers hand the
/// work to a background process and return before the version is updated.
const VERIFY_WAITS: [Duration; 3] = [Duration::ZERO, Duration::from_secs(5), Duration::from_secs(15)];

/// After a successful winget upgrade or version change: the version still
/// installed when the job changed nothing (see `FailureKind::NoEffect`).
/// `None` when the version changed or cannot be checked.
fn unchanged_version(job: &JobRequest) -> Option<String> {
    if job.source != Source::Winget {
        return None;
    }
    let target = match &job.action {
        JobAction::Upgrade => job.to_version.as_deref()?,
        JobAction::InstallVersion { version } => version.as_str(),
        _ => return None,
    };
    let from = job.from_version.as_deref().filter(|v| !version::is_unknown(v))?;
    if version::compare(target, from) == std::cmp::Ordering::Equal {
        return None;
    }
    for wait in VERIFY_WAITS {
        std::thread::sleep(wait);
        let installed = winget::installed_versions(&job.id)
            .map_err(|e| log::debug!("job {}: cannot check the installed version: {e:#}", job.key))
            .ok()?;
        if !all_still(&installed, from) {
            return None;
        }
    }
    Some(from.to_string())
}

/// Every installation of the package still reports `from`.
fn all_still(installed: &[String], from: &str) -> bool {
    !installed.is_empty()
        && installed.iter().all(|v| version::compare(&version::strip_approx(v).0, from) == std::cmp::Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_when_every_installation_kept_the_old_version() {
        let v = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(all_still(&v(&["1.141.2"]), "1.141.2"));
        assert!(all_still(&v(&["1.141.2.0"]), "1.141.2"));
        assert!(!all_still(&v(&["1.142.0"]), "1.141.2"));
        assert!(!all_still(&v(&["1.141.2", "1.142.0"]), "1.141.2"));
        assert!(!all_still(&[], "1.141.2"));
    }
}
