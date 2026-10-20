// Mock backend: same commands, events and payloads as the Tauri backend
// (docs/ARCHITECTURE.md), with realistic timings, so the UI runs in a browser.
//
// URL options: ?speed=4 (everything 4× faster), ?mock=empty (nothing to update),
// ?mock=scan-error, ?uac=refuse (the administrator prompt is refused).

import { buildDataset, lookupUpdates, WINGET_CATALOG, mockIcon, initialHistory, VERSION_LISTS } from './mock-data.js';

const params = new URLSearchParams(location.search);
const SPEED = Math.max(0.1, Number(params.get('speed')) || 1);
const SCENARIO = params.get('mock') || '';
const UAC_REFUSE = params.get('uac') === 'refuse';
const MB = 1024 * 1024;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms / SPEED));
const rand = (a, b) => a + Math.random() * (b - a);
const clone = (v) => JSON.parse(JSON.stringify(v));

// ------------------------------------------------------------------ events

const listeners = new Map();
let eventId = 0;

export function listen(event, handler) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event).add(handler);
  return Promise.resolve(() => listeners.get(event)?.delete(handler));
}

function emit(event, payload) {
  const data = { event, id: ++eventId, payload: clone(payload) };
  for (const h of [...(listeners.get(event) || [])]) {
    try { h(data); } catch (e) { console.error('[mock] listener', e); }
  }
}

// ------------------------------------------------------------------ persistent mock state

const DEFAULT_SETTINGS = {
  language: null,
  theme: 'system',
  scanOnStart: true,
  onlineIntel: true,
  lookupUnmanaged: true,
  includeUnknown: true,
  useChocolatey: true,
  useScoop: true,
  windowsUpdate: true,
  closeApps: 'ask',
  ignored: [{ key: 'winget:Oracle.VirtualBox', name: 'Oracle VirtualBox', version: '7.2.4' }],
  schedule: null,
};

function loadSettings() {
  try {
    const raw = localStorage.getItem('majestic.mock.settings');
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return clone(DEFAULT_SETTINGS);
}

let settings = loadSettings();
let dataset = buildDataset();
let history = initialHistory();
let xodoFailedOnce = false;
let running = null; // current batch
let lookupTimer = null;
let mockLeftovers = 642 * MB; // installers left in %TEMP%\WinGet…

// ?repeated=1: Xodo's update already failed 3 times (history).
if (params.get('repeated')) {
  dataset = dataset.map((i) => (i.key === 'winget:Xodo.PDFStudio'
    ? { ...i, assessment: { ...i.assessment, preselect: false, reasons: [...i.assessment.reasons, { type: 'repeated_failures', count: 3 }] } }
    : i));
}

if (SCENARIO === 'empty') {
  dataset = dataset.map((i) => (i.kind === 'update' || i.kind === 'manual')
    ? { ...i, kind: 'up_to_date', availableVersion: null, versionCertainty: 'exact', installedVersion: i.availableVersion || i.installedVersion, assessment: { priority: 'normal', preselect: false, reasons: [] }, running: [] }
    : i);
}

// ------------------------------------------------------------------ helpers

function isIgnored(item) {
  return (settings.ignored || []).some((r) => r.key === item.key && (r.version == null || r.version === item.availableVersion));
}

function cmpVersion(a, b) {
  const pa = String(a).split(/[.\-+]/).map((x) => parseInt(x, 10) || 0);
  const pb = String(b).split(/[.\-+]/).map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d) return d;
  }
  return 0;
}

const INSTALL_SECS = { msi: 9, wix: 9, burn: 14, inno: 7, nullsoft: 5, exe: 12, msstore: 8, msix: 6, zip: 2, portable: 2 };
const BANDWIDTH = 12 * MB; // what this demo "PC" usually gets

function estimateOf(job) {
  const size = Number(job.downloadSize) || 20 * MB;
  const type = job.installerType || null;
  const download = size / BANDWIDTH;
  const install = (INSTALL_SECS[type] ?? 10) + (size / (100 * MB)) * 3;
  const fromHistory = history.some((h) => h.key === job.key && h.result === 'succeeded' && h.installSecs);
  return { download, install, basis: fromHistory ? 'history' : type ? 'type' : 'default' };
}

function stats(items) {
  const visible = items.filter((i) => !i.ignored);
  const up = visible.filter((i) => i.kind === 'update');
  const count = (p) => up.filter((i) => i.assessment.priority === p).length;
  return {
    updates: up.length,
    critical: count('critical'),
    major: count('major'),
    recommended: count('recommended'),
    uncertain: count('uncertain'),
    manual: visible.filter((i) => i.kind === 'manual').length,
    unmanaged: visible.filter((i) => i.kind === 'unmanaged').length,
    upToDate: visible.filter((i) => i.kind === 'up_to_date').length,
    ignored: items.filter((i) => i.ignored).length,
    storeApps: 37,
  };
}

function versionsFor(id, source) {
  if (VERSION_LISTS[id]) return [...VERSION_LISTS[id]];
  const item = dataset.find((i) => i.id === id && (i.source === source || !source));
  const set = new Set();
  const latest = item?.availableVersion || item?.installedVersion || '1.0.0';
  if (item?.availableVersion) set.add(item.availableVersion);
  if (item?.installedVersion && !/^[<>]/.test(item.installedVersion)) set.add(item.installedVersion);
  for (const h of history) if (h.id === id) { if (h.fromVersion) set.add(h.fromVersion); if (h.toVersion) set.add(h.toVersion); }
  const parts = String(latest).replace(/^[<>]\s*/, '').split('.').map((x) => parseInt(x, 10) || 0);
  for (let i = 1; set.size < 9 && i < 12; i++) {
    const p = [...parts];
    const last = p.length - 1;
    if (p[last] - i >= 0) p[last] -= i;
    else if (last > 0) { p[last - 1] = Math.max(0, p[last - 1] - 1); p[last] = 9 - (i % 5); }
    const raw = String(latest).replace(/^[<>]\s*/, '').split('.');
    set.add(p.map((n, k) => String(n).padStart(raw[k]?.length || 1, '0')).join('.'));
  }
  return [...set].sort((a, b) => cmpVersion(b, a));
}

// ------------------------------------------------------------------ scan

async function step(stepName, { ms, detail = null, fractions = false, state = 'done', finalDetail }) {
  emit('scan-progress', { step: stepName, state: 'started', detail, fraction: fractions ? 0 : null });
  if (fractions) {
    const n = 6;
    for (let i = 1; i <= n; i++) {
      await sleep(ms / n);
      emit('scan-progress', { step: stepName, state: 'started', detail, fraction: i / n });
    }
  } else {
    await sleep(ms);
  }
  emit('scan-progress', { step: stepName, state, detail: finalDetail ?? detail, fraction: null });
}

const FR = () => String(settings.language || navigator.language || '').toLowerCase().startsWith('fr');
const D = {
  updates: (n) => (FR() ? `${n} mises à jour` : `${n} updates`),
  packages: (n) => (FR() ? `${n} paquets` : `${n} packages`),
  programs: (n) => (FR() ? `${n} programmes` : `${n} programs`),
  apps: (n) => (FR() ? `${n} applications` : `${n} apps`),
  entries: (n) => (FR() ? `${n} entrées` : `${n} entries`),
  manifests: (n) => (FR() ? `${n} notes lues` : `${n} notes read`),
  running: (n) => (FR() ? `${n} ouverts` : `${n} running`),
  found: (n) => (FR() ? `${n} trouvées` : `${n} found`),
};

async function scan() {
  if (lookupTimer) { clearTimeout(lookupTimer); lookupTimer = null; }
  await step('environment', { ms: 700, detail: 'winget v1.11.510 · Chocolatey 2.5.1 · Scoop', finalDetail: 'Windows 11 25H2 · winget v1.11.510' });
  if (SCENARIO === 'scan-error') {
    emit('scan-progress', { step: 'winget_upgrade', state: 'started', detail: null, fraction: null });
    await sleep(900);
    emit('scan-progress', { step: 'winget_upgrade', state: 'failed', detail: 'winget exited with code 0x8A15000F', fraction: null });
    throw 'winget upgrade failed: 0x8A15000F (APPINSTALLER_CLI_ERROR_SOURCE_DATA_MISSING)';
  }
  await step('winget_upgrade', { ms: 2400, fractions: true, finalDetail: D.updates(16) });
  await step('winget_list', { ms: 1500, finalDetail: D.packages(128) });
  await step('registry', { ms: 1000, fractions: true, finalDetail: D.programs(173) });
  if (settings.useChocolatey) await step('chocolatey', { ms: 900, finalDetail: D.packages(4) });
  else emit('scan-progress', { step: 'chocolatey', state: 'skipped', detail: null, fraction: null });
  if (settings.useScoop) await step('scoop', { ms: 700, finalDetail: D.apps(6) });
  else emit('scan-progress', { step: 'scoop', state: 'skipped', detail: null, fraction: null });
  if (settings.onlineIntel) {
    await step('security_catalog', { ms: 1100, finalDetail: D.entries(1452) });
    await step('release_notes', { ms: 1500, fractions: true, finalDetail: D.manifests(14) });
  } else {
    emit('scan-progress', { step: 'security_catalog', state: 'skipped', detail: null, fraction: null });
    emit('scan-progress', { step: 'release_notes', state: 'skipped', detail: null, fraction: null });
  }
  await step('processes', { ms: 500, finalDetail: D.running(2) });

  let items = clone(dataset)
    .filter((i) => settings.useChocolatey || i.source !== 'chocolatey')
    .filter((i) => settings.useScoop || i.source !== 'scoop')
    .filter((i) => settings.includeUnknown || i.versionCertainty !== 'unknown');
  if (!settings.onlineIntel) {
    items = items.map((i) => ({ ...i, releaseNotes: null, assessment: { ...i.assessment, reasons: i.assessment.reasons.filter((r) => !['known_exploited', 'security_fix'].includes(r.type)) } }));
  }
  items = items.map((i) => ({ ...i, ignored: isIgnored(i) }));

  const warnings = [];
  if (settings.useScoop) warnings.push({ code: 'scoop_failed', detail: "scoop status: bucket 'extras' could not be refreshed (network timeout)" });

  const result = {
    items,
    stats: stats(items),
    warnings,
    environment: { appVersion: '0.1.3', winget: '1.11.510', chocolatey: settings.useChocolatey ? '2.5.1' : null, scoop: settings.useScoop, elevated: false, adminMember: true, os: 'Windows 11 Home 25H2', online: true },
    scannedAt: new Date().toISOString(),
  };

  if (settings.lookupUnmanaged && settings.onlineIntel) {
    lookupTimer = setTimeout(() => runLookup(items), 400 / SPEED);
  }
  if (settings.windowsUpdate) setTimeout(runWindowsUpdate, 300 / SPEED);
  return result;
}

// Scheduled check: ?schedule=missing simulates a task removed by Defender.
function mockScheduleStatus() {
  const sc = settings.schedule;
  let nextRun = null;
  if (sc) {
    const d = new Date();
    d.setDate(d.getDate() + 3);
    const [hh, mm] = String(sc.time || '12:00').split(':');
    d.setHours(Number(hh) || 12, Number(mm) || 0, 0, 0);
    nextRun = new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 19);
  }
  const missing = params.get('schedule') === 'missing';
  return {
    schedule: sc || null,
    task: sc ? (missing ? 'missing' : 'ok') : 'none',
    taskCommand: sc && !missing ? 'C:\\Users\\demo\\Documents\\MAJestic\\MAJestic.exe' : null,
    exe: params.get('downloads') ? 'C:\\Users\\demo\\Downloads\\MAJestic.exe' : 'C:\\Users\\demo\\Documents\\MAJestic\\MAJestic.exe',
    locationWarning: params.get('downloads') ? 'downloads' : null,
    nextRun: missing ? null : nextRun,
    lastCheck: { at: new Date(Date.now() - 2 * 86400e3).toISOString(), updates: 9, important: 3, windowsSecurity: 1, notified: true },
  };
}

// ?wu=none | error | reboot changes what Windows Update answers.
let lastWindowsUpdates = null;
async function runWindowsUpdate() {
  emit('scan-progress', { step: 'windows_update', state: 'started', detail: null, fraction: null });
  await sleep(4000 / SPEED);
  const mode = params.get('wu');
  const now = new Date();
  const ago = (h) => new Date(now.getTime() - h * 3600e3).toISOString();
  const data = {
    updates: mode === 'none' || mode === 'error' ? [] : [
      { title: '2026-10 Mise à jour cumulative pour Windows 11 Version 25H2 pour les systèmes x64 (KB5066835)', kb: ['KB5066835'], kind: 'security', severity: 'Critical', optional: false, downloaded: true, size: 812 * MB, reboot: true, date: '2026-10-14', supportUrl: 'https://support.microsoft.com/help/5066835' },
      { title: 'Mise à jour de la veille de sécurité pour Microsoft Defender Antivirus - KB2267602 (version 1.439.120.0)', kb: ['KB2267602'], kind: 'definition', severity: null, optional: false, downloaded: false, size: 61 * MB, reboot: false, date: '2026-10-19', supportUrl: null },
      { title: 'Realtek Semiconductor Corp. - Extension - 10.0.26100.21', kb: [], kind: 'driver', severity: null, optional: true, downloaded: false, size: 2 * MB, reboot: false, date: '2026-09-02', supportUrl: null },
    ],
    rebootPending: mode === 'reboot',
    lastSearch: ago(5),
    lastInstall: ago(30),
    online: true,
    error: mode === 'error' ? '0x80070422 Le service ne peut pas être démarré.' : null,
    checkedAt: now.toISOString(),
  };
  lastWindowsUpdates = data;
  emit('scan-progress', { step: 'windows_update', state: data.error ? 'failed' : 'done', detail: null, fraction: null });
  emit('windows-updates', data);
}

async function runLookup(items) {
  lookupTimer = null;
  const changes = lookupUpdates(items);
  const unmanaged = items.filter((i) => i.kind === 'unmanaged').length;
  emit('scan-progress', { step: 'lookup', state: 'started', detail: D.programs(unmanaged), fraction: 0 });
  const n = 8;
  for (let i = 1; i <= n; i++) {
    await sleep(900);
    emit('scan-progress', { step: 'lookup', state: 'started', detail: D.programs(unmanaged), fraction: i / n });
    if (i === 3 && changes[0]) emit('item-updated', changes[0]);
    if (i === 6 && changes[1]) emit('item-updated', changes[1]);
  }
  // keep the mock dataset in sync for later scans
  for (const c of changes) {
    const idx = dataset.findIndex((d) => d.key === c.key);
    if (idx >= 0) dataset[idx] = { ...clone(c), ignored: false };
  }
  emit('scan-progress', { step: 'lookup', state: 'done', detail: D.found(changes.length), fraction: null });
}

// ------------------------------------------------------------------ installation

function jobEvent(key, state, extra = {}) {
  return { key, state, progress: null, downloaded: null, totalBytes: null, speed: null, etaSecs: null, message: null, ...extra };
}

function needsElevation(job) {
  if (job.action?.type === 'store_update_scan') return true;
  if (['scoop', 'ms_store', 'msstore', 'msix'].includes(job.source)) return false;
  if (job.source === 'chocolatey') return true;
  return job.scope !== 'user';
}

async function runBatch(jobs, closeApps, allowElevation) {
  const batch = {
    cancel: false,
    started: performance.now(),
    est: jobs.map(estimateOf),
    calib: [],
    etaSmooth: null,
    results: [],
    succeeded: 0,
    failed: 0,
    done: 0,
    rebootRequired: false,
    elevationRefused: false,
  };
  running = batch;
  const totalEst = batch.est.reduce((s, e) => s + e.download + e.install, 0) || 1;
  let doneEst = 0;

  const calibration = () => (batch.calib.length ? batch.calib.reduce((a, b) => a + b, 0) / batch.calib.length : 1);
  const queue = (current, jobFrac, currentRemaining) => {
    const i = jobs.findIndex((j) => j.key === current);
    const curEst = i >= 0 ? batch.est[i].download + batch.est[i].install : 0;
    const fraction = Math.min(1, (doneEst + curEst * (jobFrac || 0)) / totalEst);
    const queued = batch.cancel ? 0 : jobs.slice(i + 1).reduce((s, _, k) => s + batch.est[i + 1 + k].download + batch.est[i + 1 + k].install, 0) * calibration();
    const raw = (currentRemaining || 0) + queued;
    batch.etaSmooth = batch.etaSmooth === null ? raw : batch.etaSmooth * 0.85 + raw * 0.15;
    const noisy = Math.max(0, batch.etaSmooth * rand(0.98, 1.02));
    emit('queue-progress', {
      done: batch.done, total: jobs.length, succeeded: batch.succeeded, failed: batch.failed,
      fraction, elapsedSecs: (performance.now() - batch.started) / 1000,
      etaSecs: current ? noisy / SPEED : null, current: current || null,
    });
  };

  for (const job of jobs) emit('job-progress', jobEvent(job.key, { type: 'queued' }));
  queue(null, 0, 0);
  await sleep(400);

  for (let i = 0; i < jobs.length; i++) {
    const job = jobs[i];
    const est = batch.est[i];
    const item = dataset.find((d) => d.key === job.key);
    if (batch.cancel) {
      emit('job-progress', jobEvent(job.key, { type: 'cancelled' }));
      batch.results.push({ key: job.key, state: { type: 'cancelled' } });
      batch.done += 1;
      continue;
    }
    const t0 = performance.now();
    const calib = calibration();
    const plannedInstall = est.install * rand(0.8, 1.3);
    const jobTotal = est.download + est.install;
    const dlWeight = est.download / jobTotal;

    if (closeApps.includes(job.key) && item?.running?.length) {
      emit('job-progress', jobEvent(job.key, { type: 'closing_apps' }, { message: `Closing ${item.running.map((p) => p.name).join(', ')}`, etaSecs: (jobTotal * calib + 2) / SPEED }));
      queue(job.key, 0, jobTotal * calib + 2);
      await sleep(1800);
    }
    emit('job-progress', jobEvent(job.key, { type: 'starting' }, { progress: 0, etaSecs: (jobTotal * calib) / SPEED }));
    queue(job.key, 0, jobTotal * calib);
    await sleep(700);

    if (UAC_REFUSE && allowElevation && needsElevation(job)) {
      const state = { type: 'failed', code: 1223, kind: 'elevation_refused', logExcerpt: 'The operation was canceled by the user. (ERROR_CANCELLED, 1223)' };
      batch.elevationRefused = true;
      finish(job, state, t0, est, 0);
      continue;
    }

    // ---- download
    const totalBytes = Number(job.downloadSize) || Math.round(rand(8, 40) * MB);
    let downloaded = 0;
    let speed = null;
    const target = BANDWIDTH * rand(0.75, 1.35);
    while (downloaded < totalBytes) {
      await sleep(250);
      const instant = Math.max(0.2 * MB, target * rand(0.55, 1.45) * (downloaded === 0 ? 0.4 : 1));
      speed = speed === null ? instant : speed * 0.75 + instant * 0.25;
      downloaded = Math.min(totalBytes, downloaded + instant * 0.25);
      const dlFrac = downloaded / totalBytes;
      const remaining = (totalBytes - downloaded) / speed + est.install * calib;
      emit('job-progress', jobEvent(job.key, { type: 'downloading' }, {
        progress: dlFrac * dlWeight, downloaded: Math.round(downloaded), totalBytes, speed: speed * SPEED, etaSecs: remaining / SPEED,
      }));
      queue(job.key, dlFrac * dlWeight, remaining);
    }

    if (job.key === 'winget:Xodo.PDFStudio' && !xodoFailedOnce && job.action?.type !== 'interactive') {
      xodoFailedOnce = true;
      await sleep(500);
      finish(job, {
        type: 'failed', code: -1978335215, kind: 'hash_mismatch',
        logExcerpt: 'Installer hash does not match.\nExpected: 6f1c0e…a91b\nActual:   0b77d2…44e0\nThe publisher may have replaced the installer without updating the winget manifest.',
      }, t0, est, est.download);
      continue;
    }

    if (job.key === 'winget:Microsoft.VisualStudioCode' && job.action?.type === 'upgrade') {
      await sleep(1500 / SPEED);
      finish(job, {
        type: 'failed', code: 0, kind: 'no_effect',
        logExcerpt: `The installer finished without error, but Windows still reports version ${job.fromVersion}.`,
      }, t0, est, est.download);
      continue;
    }

    // ---- install
    const withFraction = ['msi', 'wix', 'burn', 'inno'].includes(job.installerType);
    const messages = ['Starting package install…', 'Extracting files…', 'Registering components…', 'Finishing…'];
    let elapsed = 0;
    emit('job-progress', jobEvent(job.key, { type: 'installing' }, { progress: withFraction ? dlWeight : null, etaSecs: (est.install * calib) / SPEED, message: messages[0] }));
    while (elapsed < plannedInstall) {
      await sleep(400);
      elapsed += 0.4;
      const instFrac = Math.min(0.99, elapsed / plannedInstall);
      const remaining = Math.max(1, est.install * calib - elapsed);
      emit('job-progress', jobEvent(job.key, { type: 'installing' }, {
        progress: withFraction ? dlWeight + instFrac * (1 - dlWeight) : null,
        etaSecs: remaining / SPEED,
        message: messages[Math.min(messages.length - 1, Math.floor(instFrac * messages.length))],
      }));
      queue(job.key, dlWeight + instFrac * (1 - dlWeight), remaining);
    }
    const reboot = job.key === 'winget:Microsoft.DotNet.DesktopRuntime.8';
    finish(job, { type: 'succeeded', rebootRequired: reboot }, t0, est, est.download);
  }

  function finish(job, state, t0, est, dlSecs) {
    const actual = ((performance.now() - t0) / 1000) * SPEED; // simulated seconds
    batch.calib.push(Math.max(0.3, Math.min(3, actual / (est.download + est.install))));
    doneEst += est.download + est.install;
    batch.done += 1;
    if (state.type === 'succeeded') batch.succeeded += 1;
    if (state.type === 'failed') batch.failed += 1;
    if (state.rebootRequired) batch.rebootRequired = true;
    emit('job-progress', jobEvent(job.key, state, { progress: state.type === 'succeeded' ? 1 : null }));
    // ?repeated=1: this version already failed twice before.
    batch.results.push({ key: job.key, state, ...(state.type === 'failed' && params.get('repeated') ? { failedAttempts: 3 } : {}) });
    history.unshift({
      date: new Date().toISOString(), key: job.key, source: job.source, id: job.id, name: job.name,
      action: job.action || { type: 'upgrade' }, fromVersion: job.fromVersion ?? null, toVersion: job.toVersion ?? null,
      result: state.type, failure: state.kind || null, code: state.code ?? null, rebootRequired: !!state.rebootRequired,
      downloadSecs: dlSecs || null, installSecs: Math.max(0, actual - (dlSecs || 0)), downloadBytes: job.downloadSize ?? null, installerType: job.installerType ?? null,
    });
    if (state.type === 'succeeded') applySuccess(job);
    const next = jobs[jobs.indexOf(job) + 1];
    queue(next && !batch.cancel ? next.key : null, 0, 0);
  }

  await sleep(500);
  queue(null, 1, 0);
  running = null;
  emit('updates-done', { rebootRequired: batch.rebootRequired, elevationRefused: batch.elevationRefused, results: batch.results });
}

function applySuccess(job) {
  const idx = dataset.findIndex((d) => d.key === job.key);
  if (idx < 0) return;
  const item = dataset[idx];
  const v = job.toVersion || item.availableVersion || item.installedVersion;
  const latest = item.availableVersion || item.installedVersion;
  const isLatest = !latest || cmpVersion(v, latest) >= 0;
  dataset[idx] = {
    ...item,
    installedVersion: v,
    versionCertainty: 'exact',
    kind: isLatest ? 'up_to_date' : 'update',
    availableVersion: isLatest ? null : latest,
    source: job.source === 'winget' && item.source === 'registry' ? 'winget' : item.source,
    assessment: isLatest ? { priority: 'normal', preselect: false, reasons: [] } : { priority: 'normal', preselect: false, reasons: [] },
    running: [],
    alternatives: [{ type: 'install_version' }],
  };
}

// ------------------------------------------------------------------ commands

const handlers = {
  async app_info() {
    await sleep(60);
    return { appVersion: params.get('updated') ? '0.2.0' : '0.1.3', updatedFrom: params.get('updated') ? '0.1.3' : null, scheduleNotice: params.get('schedule') || null, uiLanguage: navigator.language || 'fr-FR', elevated: false, adminMember: true, dataDir: 'C:\\Users\\demo\\AppData\\Local\\MAJestic' };
  },
  async get_settings() {
    await sleep(60);
    return clone(settings);
  },
  async save_settings({ settings: next }) {
    await sleep(120);
    if (!next || typeof next !== 'object') throw 'invalid settings';
    // Like the backend: only set_schedule changes the schedule.
    settings = { ...DEFAULT_SETTINGS, ...clone(next), schedule: settings.schedule };
    try { localStorage.setItem('majestic.mock.settings', JSON.stringify(settings)); } catch { /* ignore */ }
    return clone(settings);
  },
  scan,
  async get_icon({ hint }) {
    await sleep(rand(40, 260));
    return mockIcon(hint);
  },
  async list_versions({ id, source }) {
    await sleep(700);
    return versionsFor(id, source);
  },
  async search_winget({ query }) {
    await sleep(900);
    const words = String(query || '').toLowerCase().split(/\s+/).filter((w) => w.length > 2 && !/^\d/.test(w));
    if (!words.length) return [];
    return WINGET_CATALOG.filter((c) => words.some((w) => c.name.toLowerCase().includes(w) || c.id.toLowerCase().includes(w)));
  },
  async running_apps({ keys }) {
    await sleep(300);
    return (keys || []).map((k) => dataset.find((d) => d.key === k)).filter((d) => d && d.running?.length)
      .map((d) => ({ key: d.key, name: d.name, processes: clone(d.running) }));
  },
  async estimate_jobs({ jobs }) {
    await sleep(150);
    return (jobs || []).map((j) => {
      const e = estimateOf(j);
      return { key: j.key, downloadSecs: e.download / SPEED, installSecs: e.install / SPEED, basis: e.basis };
    });
  },
  async start_updates({ jobs, closeApps, allowElevation }) {
    if (running) throw 'An update batch is already running.';
    if (!Array.isArray(jobs) || !jobs.length) throw 'No jobs.';
    setTimeout(() => runBatch(clone(jobs), closeApps || [], allowElevation !== false), 0);
    return null;
  },
  async cancel_updates() {
    if (running) running.cancel = true;
    return null;
  },
  async get_history() {
    await sleep(200);
    return clone(history);
  },
  async clear_history() {
    await sleep(150);
    history = [];
    return null;
  },
  async open_url({ url }) {
    console.info('[mock] open_url', url);
    return null;
  },
  async open_store_updates() {
    console.info('[mock] open_store_updates');
    return null;
  },
  async open_logs() {
    console.info('[mock] open_logs');
    return null;
  },
  async schedule_status() {
    return mockScheduleStatus();
  },
  async set_schedule({ schedule }) {
    await sleep(500 / SPEED);
    settings = { ...settings, schedule: schedule || null };
    try { localStorage.setItem('majestic.mock.settings', JSON.stringify(settings)); } catch { /* ignore */ }
    return mockScheduleStatus();
  },
  async run_uninstaller({ key }) {
    console.info('[mock] run_uninstaller', key);
    return null;
  },
  async open_installed_apps({ classic }) {
    console.info('[mock] open_installed_apps', classic);
    return null;
  },
  async get_windows_updates() {
    return lastWindowsUpdates;
  },
  async open_windows_update({ page }) {
    console.info('[mock] open_windows_update', page);
    return null;
  },
  async export_inventory({ text }) {
    await sleep(600 / SPEED);
    console.info('[mock] export_inventory', text?.fileName);
    const packages = dataset.filter((i) => ['winget', 'msstore', 'chocolatey', 'scoop'].includes(i.source)).length;
    return { path: `C:\\Users\\demo\\Documents\\${text?.fileName || 'Logiciels MAJestic.json'}`, packages, manual: 6 };
  },
  async reveal_export() {
    console.info('[mock] reveal_export');
    return null;
  },
  // ?import=invalid answers with a broken file.
  async import_inventory() {
    await sleep(600 / SPEED);
    if (params.get('import') === 'invalid') throw 'invalid_file';
    const pkg = (source, id, name, version, scope = null) => ({ source, id, name, version, scope });
    return {
      file: 'E:\\Sauvegarde\\Logiciels MAJestic 2026-10-12.json',
      plan: {
        createdAt: '2026-10-12T18:20:00+02:00',
        toInstall: [
          pkg('winget', 'Inkscape.Inkscape', 'Inkscape', '1.4.2', 'machine'),
          pkg('winget', 'GIMP.GIMP.3', 'GIMP 3', '3.0.6', 'machine'),
          pkg('msstore', '9NCBCSZSJRSB', 'Spotify', null),
          pkg('winget', 'KeePassXCTeam.KeePassXC', 'KeePassXC', '2.7.10', 'machine'),
          pkg('scoop', 'ffmpeg', 'ffmpeg', '8.0', 'user'),
        ],
        installed: dataset.filter((i) => i.source === 'winget').slice(0, 12).map((i) => pkg(i.source, i.id, i.name, i.installedVersion)),
        manual: [
          { name: 'Jasc Paint Shop Pro 9', version: '9.01', url: null },
          { name: 'Canon IJ Scan Utility', version: '2.4.0', url: 'https://www.canon.fr/support/' },
        ],
        manualInstalled: 4,
      },
    };
  },
  async leftover_downloads() {
    await sleep(400 / SPEED);
    return { bytes: mockLeftovers, entries: mockLeftovers ? 3 : 0 };
  },
  async clean_downloads() {
    await sleep(900 / SPEED);
    if (running) throw 'busy';
    const freed = mockLeftovers;
    mockLeftovers = 0;
    return { freed, removed: freed ? 3 : 0, failed: 0 };
  },
  async describe_item({ request }) {
    await sleep(500 / SPEED);
    const known = {
      'Mozilla Firefox': { summary: 'Mozilla Firefox est un logiciel libre et ouvert, réalisé par une communauté de milliers de contributeurs dans le monde.', locale: 'fr-FR', origin: 'winget' },
      '7-Zip': { summary: 'A file archiver with a high compression ratio.', locale: 'en-US', origin: 'winget' },
      'VLC media player': { summary: 'VLC is a free and open source cross-platform multimedia player.', description: 'It plays most multimedia files as well as DVDs, Audio CDs, VCDs, and various streaming protocols.', locale: 'en-US', origin: 'winget' },
      'Notepad++': { summary: 'Notepad++ is a free source code editor and Notepad replacement that supports several languages.', locale: 'en-US', origin: 'winget' },
      'HandBrake': { description: 'HandBrake is a tool for converting video from nearly any format to a selection of modern, widely supported codecs.', locale: 'en-US', origin: 'chocolatey', url: 'https://community.chocolatey.org/packages/handbrake' },
    };
    const found = known[request?.name];
    return found ? { summary: null, description: null, url: null, ...found } : null;
  },
  // ?appUpdate=1 offers a newer MAJestic; ?appUpdate=fail makes its install
  // fail; ?appUpdate=error makes the check itself fail.
  async check_app_update() {
    await sleep(800 / SPEED);
    if (!params.get('appUpdate')) return null;
    if (params.get('appUpdate') === 'error') throw 'GET https://api.github.com/repos/Belenos-Toutatis/MAJestic/releases/latest: 403 Forbidden';
    return {
      version: '0.2.0',
      pageUrl: 'https://github.com/Belenos-Toutatis/MAJestic/releases/tag/v0.2.0',
      size: 6.6 * MB,
      notes: [
        '## Nouveautés / What’s new',
        '',
        '- **Mise à jour en un clic** : MAJestic se met à jour lui-même,',
        '  empreinte SHA-256 vérifiée.',
        '- Corrections diverses.',
        '',
        '*MAJestic now updates itself in one click.*',
      ].join('\n'),
    };
  },
  async install_app_update() {
    const total = 6.6 * MB;
    for (let done = 0; done < total; done += total / 12) {
      emit('app-update-progress', { phase: 'download', downloaded: Math.round(done), total });
      await sleep(250 / SPEED);
    }
    emit('app-update-progress', { phase: 'download', downloaded: total, total });
    if (params.get('appUpdate') === 'fail') {
      throw { kind: 'hash_mismatch', message: 'expected 9b9e…66ed, got 0b77…44e0' };
    }
    emit('app-update-progress', { phase: 'install', downloaded: 0, total: null });
    await sleep(600 / SPEED);
    emit('app-update-progress', { phase: 'restart', downloaded: 0, total: null });
    await sleep(1200 / SPEED);
    location.search = '?updated=1';
    return null;
  },
};

export async function invoke(cmd, args = {}) {
  const fn = handlers[cmd];
  if (!fn) throw `unknown command: ${cmd}`;
  return fn(args || {});
}
