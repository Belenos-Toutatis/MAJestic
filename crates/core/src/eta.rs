//! Time estimates for installations.
//!
//! Up front, each job gets a download estimate (installer size ÷ bandwidth)
//! and an install estimate (this PC's own past installs of the package, else
//! a model by installer type and size, corrected by how this PC compared to
//! the model in the past). While a batch runs, [`BatchTracker`] refines them
//! with the measured download speed and a calibration factor learned from the
//! jobs already finished, and smooths the result so the displayed time left
//! decreases calmly instead of jumping around.

use std::collections::HashMap;
use std::time::Instant;

use crate::model::{HistoryEntry, JobAction, JobEstimate, JobRequest, Source};

const DEFAULT_BANDWIDTH: f64 = 4.0 * 1024.0 * 1024.0;
/// Download size assumed when the installer size is unknown.
const DEFAULT_SIZE: f64 = 40.0 * 1024.0 * 1024.0;
/// winget/choco start-up, source opening, hash verification.
const TOOL_OVERHEAD: f64 = 6.0;
const MB: f64 = 1024.0 * 1024.0;

/// Install time model: `base + per_mb × size`, seconds.
fn type_model(installer_type: Option<&str>) -> (f64, f64) {
    match installer_type.map(|t| t.to_ascii_lowercase()).as_deref() {
        Some("msi") | Some("wix") => (20.0, 0.25),
        Some("burn") => (40.0, 0.30),
        Some("inno") => (8.0, 0.15),
        Some("nullsoft") => (6.0, 0.12),
        Some("msix") | Some("appx") => (6.0, 0.08),
        Some("zip") | Some("portable") => (3.0, 0.05),
        _ => (15.0, 0.20),
    }
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 { (values[mid - 1] + values[mid]) / 2.0 } else { values[mid] })
}

/// Estimates learned from this PC's history.
#[derive(Debug, Clone)]
pub struct EtaModel {
    /// Bytes per second.
    pub bandwidth: f64,
    /// Median measured install seconds per job key.
    per_key: HashMap<String, f64>,
    /// Median of (measured ÷ modelled) install time over the history.
    correction: f64,
}

impl Default for EtaModel {
    fn default() -> Self {
        EtaModel { bandwidth: DEFAULT_BANDWIDTH, per_key: HashMap::new(), correction: 1.0 }
    }
}

impl EtaModel {
    pub fn from_history(history: &[HistoryEntry]) -> EtaModel {
        let ok = history.iter().filter(|h| h.result == "succeeded");

        let mut speeds: Vec<f64> = Vec::new();
        let mut per_key: HashMap<String, Vec<f64>> = HashMap::new();
        let mut ratios: Vec<f64> = Vec::new();
        for h in ok {
            if let (Some(bytes), Some(secs)) = (h.download_bytes, h.download_secs) {
                if bytes as f64 > MB && secs > 0.5 {
                    speeds.push(bytes as f64 / secs);
                }
            }
            if let Some(secs) = h.install_secs.filter(|s| *s > 0.0) {
                per_key.entry(h.key.clone()).or_default().push(secs);
                let (base, per_mb) = type_model(h.installer_type.as_deref());
                let size = h.download_bytes.map(|b| b as f64).unwrap_or(DEFAULT_SIZE) / MB;
                ratios.push(secs / (base + per_mb * size));
            }
        }

        EtaModel {
            bandwidth: median(&mut speeds).unwrap_or(DEFAULT_BANDWIDTH),
            per_key: per_key.into_iter().filter_map(|(k, mut v)| median(&mut v).map(|m| (k, m))).collect(),
            correction: median(&mut ratios).map(|r| r.clamp(0.4, 3.0)).unwrap_or(1.0),
        }
    }

    pub fn estimate(&self, job: &JobRequest) -> JobEstimate {
        if job.action == JobAction::StoreUpdateScan {
            return JobEstimate { key: job.key.clone(), download_secs: 0.0, install_secs: 60.0, basis: "default".into() };
        }
        let size = job.download_size.map(|b| b as f64);
        let download_secs = size.unwrap_or(DEFAULT_SIZE) / self.bandwidth;

        let (install_secs, basis) = if let Some(secs) = self.per_key.get(&job.key) {
            (*secs, "history")
        } else if job.source == Source::Chocolatey || job.source == Source::Scoop {
            (30.0 * self.correction, "default")
        } else {
            let (base, per_mb) = type_model(job.installer_type.as_deref());
            let basis = if job.installer_type.is_some() || size.is_some() { "type" } else { "default" };
            ((base + per_mb * size.unwrap_or(DEFAULT_SIZE) / MB) * self.correction, basis)
        };

        JobEstimate {
            key: job.key.clone(),
            download_secs: download_secs + TOOL_OVERHEAD,
            install_secs,
            basis: basis.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Downloading,
    Installing,
}

struct Current {
    index: usize,
    started: Instant,
    phase: Phase,
    download_started: Option<Instant>,
    install_started: Option<Instant>,
    downloaded: u64,
    total: Option<u64>,
    /// Smoothed speed of this download, bytes/s.
    speed: Option<f64>,
    last_sample: Option<(Instant, u64)>,
}

/// Measured timings of a finished job, for the history.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timings {
    pub download_secs: Option<f64>,
    pub install_secs: Option<f64>,
    pub download_bytes: Option<u64>,
}

/// Live progress and time left of a batch.
pub struct BatchTracker {
    keys: Vec<String>,
    estimates: Vec<JobEstimate>,
    finished: Vec<bool>,
    started: Instant,
    /// Bandwidth the estimates were computed with.
    initial_bandwidth: f64,
    bandwidth: f64,
    /// actual ÷ estimated over finished jobs (smoothed).
    calibration: f64,
    current: Option<Current>,
    smoothed_eta: Option<(Instant, f64)>,
}

impl BatchTracker {
    pub fn new(estimates: Vec<JobEstimate>, bandwidth: f64) -> BatchTracker {
        BatchTracker {
            keys: estimates.iter().map(|e| e.key.clone()).collect(),
            finished: vec![false; estimates.len()],
            estimates,
            started: Instant::now(),
            initial_bandwidth: bandwidth,
            bandwidth,
            calibration: 1.0,
            current: None,
            smoothed_eta: None,
        }
    }

    fn index(&self, key: &str) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    fn est_total(&self, i: usize) -> f64 {
        let e = &self.estimates[i];
        e.download_secs + e.install_secs
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn job_started(&mut self, key: &str) {
        if let Some(index) = self.index(key) {
            self.current = Some(Current {
                index,
                started: Instant::now(),
                phase: Phase::Preparing,
                download_started: None,
                install_started: None,
                downloaded: 0,
                total: None,
                speed: None,
                last_sample: None,
            });
        }
    }

    pub fn phase(&self) -> Option<Phase> {
        self.current.as_ref().map(|c| c.phase)
    }

    /// Download progress of the current job. Returns the smoothed speed.
    pub fn download_progress(&mut self, done: Option<u64>, total: Option<u64>) -> Option<f64> {
        let cur = self.current.as_mut()?;
        let now = Instant::now();
        if cur.phase != Phase::Downloading {
            cur.phase = Phase::Downloading;
            cur.download_started.get_or_insert(now);
        }
        if total.is_some() {
            cur.total = total;
        }
        if let Some(done) = done {
            if let Some((t, prev)) = cur.last_sample {
                let dt = now.duration_since(t).as_secs_f64();
                if dt >= 0.25 && done >= prev {
                    let inst = (done - prev) as f64 / dt;
                    cur.speed = Some(match cur.speed {
                        Some(s) => 0.75 * s + 0.25 * inst,
                        None => inst,
                    });
                    cur.last_sample = Some((now, done));
                }
            } else {
                cur.last_sample = Some((now, done));
            }
            cur.downloaded = done;
        }
        cur.speed
    }

    pub fn install_started(&mut self) {
        if let Some(cur) = self.current.as_mut() {
            if cur.phase != Phase::Installing {
                cur.phase = Phase::Installing;
                cur.install_started = Some(Instant::now());
            }
        }
    }

    /// Ends the current job; learns from it. Returns its measured timings.
    pub fn job_finished(&mut self, key: &str, success: bool) -> Timings {
        let Some(index) = self.index(key) else { return Timings::default() };
        self.finished[index] = true;
        let Some(cur) = self.current.take().filter(|c| c.index == index) else {
            return Timings::default();
        };
        let now = Instant::now();
        let download_secs = cur
            .download_started
            .map(|d| cur.install_started.unwrap_or(now).duration_since(d).as_secs_f64());
        let install_secs = cur.install_started.map(|i| now.duration_since(i).as_secs_f64());
        let bytes = cur.total.or((cur.downloaded > 0).then_some(cur.downloaded));

        if success {
            let actual = now.duration_since(cur.started).as_secs_f64();
            let ratio = (actual / self.est_total(index).max(1.0)).clamp(0.3, 4.0);
            self.calibration = 0.6 * self.calibration + 0.4 * ratio;
            if let (Some(b), Some(s)) = (bytes, download_secs) {
                if b as f64 > MB && s > 0.5 {
                    self.bandwidth = 0.5 * self.bandwidth + 0.5 * (b as f64 / s);
                }
            }
        }
        Timings { download_secs, install_secs, download_bytes: bytes }
    }

    /// Remaining time of a queued job, using what we learned so far: the
    /// download part is rescaled to the measured bandwidth, the install part
    /// by the calibration factor.
    fn queued_remaining(&self, i: usize) -> f64 {
        let e = &self.estimates[i];
        let transfer = (e.download_secs - TOOL_OVERHEAD).max(0.0) * self.initial_bandwidth / self.bandwidth.max(1.0);
        transfer + TOOL_OVERHEAD + e.install_secs * self.calibration
    }

    /// (fraction of the current job done, seconds left for it).
    pub fn current_job(&self) -> Option<(f32, f64)> {
        let cur = self.current.as_ref()?;
        let e = &self.estimates[cur.index];
        let now = Instant::now();
        let est_install = e.install_secs * self.calibration;
        let est_download = e.download_secs;

        let install_left = |elapsed: f64| -> f64 {
            if elapsed < est_install {
                (est_install - elapsed).max(2.0)
            } else {
                // Longer than expected: be honest and grow slowly.
                (elapsed * 0.25).max(3.0)
            }
        };

        let (fraction, left) = match cur.phase {
            Phase::Preparing => {
                let elapsed = now.duration_since(cur.started).as_secs_f64();
                let left = (est_download - elapsed).max(2.0) + est_install;
                (elapsed / (elapsed + left), left)
            }
            Phase::Downloading => {
                let dl_left = match (cur.total, cur.speed) {
                    (Some(total), Some(speed)) if speed > 0.0 => {
                        total.saturating_sub(cur.downloaded) as f64 / speed
                    }
                    (Some(total), None) => total.saturating_sub(cur.downloaded) as f64 / self.bandwidth,
                    _ => {
                        let elapsed = cur.download_started.map(|d| now.duration_since(d).as_secs_f64()).unwrap_or(0.0);
                        (est_download - elapsed).max(2.0)
                    }
                };
                let left = dl_left + est_install;
                let elapsed = now.duration_since(cur.started).as_secs_f64();
                (elapsed / (elapsed + left), left)
            }
            Phase::Installing => {
                let in_install = cur.install_started.map(|i| now.duration_since(i).as_secs_f64()).unwrap_or(0.0);
                let left = install_left(in_install);
                let elapsed = now.duration_since(cur.started).as_secs_f64();
                (elapsed / (elapsed + left), left)
            }
        };
        Some((fraction.clamp(0.0, 0.99) as f32, left))
    }

    /// (weighted fraction of the batch, smoothed seconds left).
    pub fn queue(&mut self) -> (f32, Option<f64>) {
        let n = self.estimates.len();
        if n == 0 {
            return (1.0, Some(0.0));
        }
        let mut total = 0.0;
        let mut done = 0.0;
        let mut left = 0.0;
        let current = self.current_job();
        for i in 0..n {
            let weight = self.est_total(i);
            total += weight;
            if self.finished[i] {
                done += weight;
            } else if self.current.as_ref().is_some_and(|c| c.index == i) {
                if let Some((f, l)) = current {
                    done += weight * f as f64;
                    left += l;
                }
            } else {
                left += self.queued_remaining(i);
            }
        }
        let fraction = if total > 0.0 { (done / total).clamp(0.0, 1.0) as f32 } else { 0.0 };
        if self.finished.iter().all(|f| *f) {
            self.smoothed_eta = None;
            return (1.0, Some(0.0));
        }

        // Smooth: the previous value counts down in real time, new
        // measurements pull it gently.
        let now = Instant::now();
        let eta = match self.smoothed_eta {
            Some((t, prev)) => {
                let predicted = (prev - now.duration_since(t).as_secs_f64()).max(0.0);
                let blended = 0.8 * predicted + 0.2 * left;
                // Never show less than what is obviously still needed.
                blended.max(left * 0.6)
            }
            None => left,
        };
        self.smoothed_eta = Some((now, eta));
        (fraction, Some(eta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(key: &str, size: Option<u64>, kind: Option<&str>) -> JobRequest {
        JobRequest {
            key: key.into(),
            source: Source::Winget,
            id: key.into(),
            name: key.into(),
            action: JobAction::Upgrade,
            from_version: None,
            to_version: None,
            scope: None,
            include_unknown: false,
            include_pinned: false,
            download_size: size,
            installer_type: kind.map(String::from),
        }
    }

    fn entry(key: &str, install: f64, bytes: u64, dl: f64) -> HistoryEntry {
        HistoryEntry {
            date: "2026-10-01T10:00:00Z".into(),
            key: key.into(),
            source: Source::Winget,
            id: key.into(),
            name: key.into(),
            action: JobAction::Upgrade,
            from_version: None,
            to_version: None,
            result: "succeeded".into(),
            failure: None,
            code: Some(0),
            reboot_required: false,
            download_secs: Some(dl),
            install_secs: Some(install),
            download_bytes: Some(bytes),
            installer_type: Some("inno".into()),
        }
    }

    #[test]
    fn estimate_without_history_uses_type_model() {
        let m = EtaModel::default();
        let e = m.estimate(&job("a", Some(100 * 1024 * 1024), Some("msi")));
        assert_eq!(e.basis, "type");
        assert!((e.install_secs - 45.0).abs() < 0.01);
        assert!((e.download_secs - (25.0 + TOOL_OVERHEAD)).abs() < 0.01);
    }

    #[test]
    fn history_drives_bandwidth_and_per_key_time() {
        let h = vec![entry("a", 12.0, 50 * 1024 * 1024, 5.0), entry("a", 14.0, 50 * 1024 * 1024, 5.0)];
        let m = EtaModel::from_history(&h);
        assert!((m.bandwidth - 10.0 * MB).abs() < 1.0);
        let e = m.estimate(&job("a", None, None));
        assert_eq!(e.basis, "history");
        assert!((e.install_secs - 13.0).abs() < 0.01);
    }

    #[test]
    fn queue_fraction_and_eta() {
        let m = EtaModel::default();
        let estimates = vec![m.estimate(&job("a", Some(10 * 1024 * 1024), Some("inno"))), m.estimate(&job("b", None, None))];
        let mut t = BatchTracker::new(estimates, m.bandwidth);
        let (f0, eta0) = t.queue();
        assert_eq!(f0, 0.0);
        assert!(eta0.unwrap() > 0.0);
        t.job_started("a");
        t.download_progress(Some(5 * 1024 * 1024), Some(10 * 1024 * 1024));
        t.install_started();
        t.job_finished("a", true);
        let (f1, eta1) = t.queue();
        assert!(f1 > 0.0 && f1 < 1.0);
        assert!(eta1.unwrap() > 0.0);
        t.job_started("b");
        t.job_finished("b", false);
        let (f2, eta2) = t.queue();
        assert_eq!(f2, 1.0);
        assert_eq!(eta2, Some(0.0));
    }
}
