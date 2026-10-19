// User-level operations shared by the views: scanning, saving settings,
// starting/cancelling updates, ignoring packages, opening links.

import { invoke, listen, errorText } from './api.js';
import { state, bus, setItems, replaceItem, itemByKey, runActive, settings } from './state.js';
import { isSelectable, buildJob, runningOf, isFinal } from './model.js';
import { t } from './i18n.js';
import { toast } from './components/toast.js';
import { closeAppsDialog } from './components/close-apps.js';

let pendingItemUpdates = [];

export function go(route) {
  if (!['updates', 'history', 'settings'].includes(route)) route = 'updates';
  state.route = route;
  bus.emit('route', route);
}

function setScreen(screen) {
  state.screen = screen;
  bus.emit('screen', screen);
}

// ------------------------------------------------------------------ backend events

export async function attachBackendListeners() {
  await Promise.all([
    listen('scan-progress', onScanProgress),
    listen('item-updated', onItemUpdated),
    listen('job-progress', onJobProgress),
    listen('queue-progress', onQueueProgress),
    listen('updates-done', onUpdatesDone),
  ]);
}

function onScanProgress(p) {
  if (!p || typeof p !== 'object') return;
  if (p.step === 'lookup') {
    state.lookup = p;
    bus.emit('lookup', p);
    return;
  }
  if (!state.scan) state.scan = { steps: {}, startedAt: Date.now(), error: null };
  state.scan.steps[p.step] = p;
  bus.emit('scan-progress', p);
}

function onItemUpdated(item) {
  if (!item || !item.key) return;
  if (!state.result) { pendingItemUpdates.push(item); return; }
  const prev = replaceItem(item);
  if (!isSelectable(item)) state.selection.delete(item.key);
  else if (!prev && item.assessment?.preselect) state.selection.add(item.key);
  bus.emit('item-updated', { item, prev });
}

function onJobProgress(ev) {
  const run = state.run;
  if (!run || !ev || !ev.key) return;
  let entry = run.entries.get(ev.key);
  if (!entry) {
    entry = { job: { key: ev.key, name: itemByKey(ev.key)?.name || ev.key }, state: { type: 'queued' }, ev: null, startedAt: null, finishedAt: null };
    run.entries.set(ev.key, entry);
    run.order.push(ev.key);
  }
  const type = ev.state?.type;
  if (type && type !== 'queued' && !entry.startedAt) entry.startedAt = Date.now();
  if (isFinal(ev.state) && !entry.finishedAt) entry.finishedAt = Date.now();
  entry.state = ev.state || entry.state;
  entry.ev = ev;
  bus.emit('job-progress', { key: ev.key, entry });
}

function onQueueProgress(q) {
  if (!state.run || !q) return;
  state.run.queue = q;
  if (q.current) state.run.current = q.current;
  bus.emit('queue-progress', q);
}

function onUpdatesDone(payload) {
  const run = state.run;
  if (!run) return;
  const results = Array.isArray(payload?.results) ? payload.results : [];
  for (const r of results) {
    if (!r?.key) continue;
    const entry = run.entries.get(r.key);
    if (entry && r.state) { entry.state = r.state; entry.finishedAt = entry.finishedAt || Date.now(); }
  }
  run.done = {
    rebootRequired: !!payload?.rebootRequired,
    elevationRefused: !!payload?.elevationRefused,
    results,
  };
  run.finishedAt = Date.now();
  // Successful updates leave the selection; the list reflects them until the next scan.
  for (const r of results) {
    if (r?.state?.type !== 'succeeded') continue;
    state.selection.delete(r.key);
    const item = itemByKey(r.key);
    const job = run.entries.get(r.key)?.job;
    if (item && job && job.action?.type !== 'install_version') {
      replaceItem({
        ...item, kind: 'up_to_date', installedVersion: job.toVersion ?? item.availableVersion ?? item.installedVersion,
        availableVersion: null, versionCertainty: 'exact', running: [],
      });
    }
  }
  bus.emit('updates-done', run.done);
  setScreen('summary');
}

// ------------------------------------------------------------------ scan

export async function startScan() {
  if (runActive() || state.screen === 'scanning') return;
  state.scan = { steps: {}, startedAt: Date.now(), error: null };
  state.lookup = null;
  state.result = null;
  state.ui.detailsKey = null;
  pendingItemUpdates = [];
  if (state.run?.done) state.run = null;
  setScreen('scanning');
  try {
    const result = await invoke('scan');
    const { items, ...rest } = result || {};
    setItems(items);
    state.result = { warnings: [], stats: {}, environment: {}, scannedAt: new Date().toISOString(), ...rest };
    state.selection = new Set(state.items.filter((i) => isSelectable(i) && i.assessment?.preselect).map((i) => i.key));
    for (const item of pendingItemUpdates) replaceItem(item);
    pendingItemUpdates = [];
    state.scan.finishedAt = Date.now();
    setScreen('results');
  } catch (err) {
    state.scan.error = errorText(err) || t('scan.errorUnknown');
    setScreen('scan-error');
  }
}

// ------------------------------------------------------------------ settings

export async function saveSettings(patch, { quiet = false } = {}) {
  const next = { ...settings(), ...patch };
  try {
    const saved = await invoke('save_settings', { settings: next });
    state.settings = saved && typeof saved === 'object' ? { ...next, ...saved } : next;
    bus.emit('settings', state.settings);
    if (!quiet) toast(t('settings.saved'), { tone: 'success', duration: 1800 });
    return true;
  } catch (err) {
    toast(t('settings.saveFailed', { error: errorText(err) }), { tone: 'danger', duration: 5000 });
    bus.emit('settings', state.settings);
    return false;
  }
}

export async function ignoreItem(item, version) {
  const rules = (settings().ignored || []).filter((r) => !(r.key === item.key && (r.version ?? null) === (version ?? null)));
  rules.push({ key: item.key, name: item.name || item.key, version: version ?? null });
  const ok = await saveSettings({ ignored: rules }, { quiet: true });
  if (ok) {
    const updated = { ...item, ignored: true };
    replaceItem(updated);
    state.selection.delete(item.key);
    bus.emit('item-updated', { item: updated, prev: item });
    toast(version ? t('ignore.versionDone', { name: item.name, version }) : t('ignore.itemDone', { name: item.name }), { tone: 'success' });
  }
}

export async function unignore(key) {
  const rules = (settings().ignored || []).filter((r) => r.key !== key);
  const ok = await saveSettings({ ignored: rules }, { quiet: true });
  if (ok) {
    const item = itemByKey(key);
    if (item) {
      const updated = { ...item, ignored: false };
      replaceItem(updated);
      bus.emit('item-updated', { item: updated, prev: item });
    }
    toast(t('ignore.undone'), { tone: 'success' });
  }
}

// ------------------------------------------------------------------ links

export async function openUrl(url) {
  if (!url) return;
  try {
    await invoke('open_url', { url });
    toast(t('links.opened'), { duration: 2200 });
  } catch (err) {
    toast(t('links.openFailed', { error: errorText(err) }), { tone: 'danger' });
  }
}

export async function openStoreUpdates() {
  try { await invoke('open_store_updates'); toast(t('links.storeOpened'), { duration: 2400 }); }
  catch (err) { toast(t('links.openFailed', { error: errorText(err) }), { tone: 'danger' }); }
}

export async function openLogs() {
  try { await invoke('open_logs'); }
  catch (err) { toast(t('links.openFailed', { error: errorText(err) }), { tone: 'danger' }); }
}

export function webSearchUrl(name) {
  return `https://www.bing.com/search?q=${encodeURIComponent(`${name} download`)}`;
}

// ------------------------------------------------------------------ updates

export function selectedJobs() {
  return state.items.filter((i) => state.selection.has(i.key) && isSelectable(i)).map((i) => buildJob(i));
}

/**
 * Starts a batch through the normal flow: fresh running-apps check, the
 * close-apps dialog when needed (settings.closeApps), then start_updates.
 * opts.forceClose: close the listed programs without asking (retry after app_running).
 */
export async function startJobs(jobs, opts = {}) {
  if (runActive()) { toast(t('run.alreadyRunning')); return false; }
  jobs = (jobs || []).filter((j) => j && j.key);
  if (!jobs.length) return false;
  const allowElevation = opts.allowElevation !== false;
  let closeApps = [];
  const policy = ['ask', 'always', 'never'].includes(settings().closeApps) ? settings().closeApps : 'ask';

  // Fresh check of running programs.
  let running = [];
  try {
    const res = await invoke('running_apps', { keys: jobs.map((j) => j.key) });
    running = (Array.isArray(res) ? res : []).filter((r) => r && r.key && Array.isArray(r.processes) && r.processes.length);
  } catch {
    running = jobs.map((j) => itemByKey(j.key)).filter((i) => i && runningOf(i).length)
      .map((i) => ({ key: i.key, name: i.name, processes: runningOf(i) }));
  }
  running = running.filter((r) => jobs.some((j) => j.key === r.key));

  if (running.length) {
    const keys = running.map((r) => r.key);
    if (opts.forceClose || policy === 'always') {
      closeApps = keys;
    } else if (policy === 'never') {
      jobs = jobs.filter((j) => !keys.includes(j.key));
      toast(t('closeApps.skippedNever', { count: keys.length }), { duration: 4500 });
    } else {
      const choice = await closeAppsDialog(running);
      if (!choice) return false;
      if (choice.action === 'close') {
        closeApps = keys;
        if (choice.dontAsk) await saveSettings({ closeApps: 'always' }, { quiet: true });
      } else {
        jobs = jobs.filter((j) => !keys.includes(j.key));
      }
    }
  }
  if (!jobs.length) { toast(t('run.nothingLeft')); return false; }

  state.run = newRun(jobs, { closeApps, allowElevation });
  state.ui.detailsKey = null;
  go('updates');
  setScreen('progress');
  try {
    await invoke('start_updates', { jobs, closeApps, allowElevation });
    return true;
  } catch (err) {
    state.run = null;
    toast(t('run.startFailed', { error: errorText(err) }), { tone: 'danger', duration: 6000 });
    setScreen(state.result ? 'results' : 'idle');
    return false;
  }
}

function newRun(jobs, { closeApps, allowElevation }) {
  const entries = new Map();
  for (const job of jobs) {
    entries.set(job.key, { job, state: { type: 'queued' }, ev: null, startedAt: null, finishedAt: null });
  }
  return {
    jobs,
    entries,
    order: jobs.map((j) => j.key),
    queue: null,
    current: null,
    closeApps,
    allowElevation,
    startedAt: Date.now(),
    cancelRequested: false,
    done: null,
  };
}

export async function cancelRun() {
  if (!runActive()) return;
  state.run.cancelRequested = true;
  bus.emit('cancel-requested');
  try { await invoke('cancel_updates'); }
  catch (err) { toast(t('run.cancelFailed', { error: errorText(err) }), { tone: 'danger' }); }
}

/** Leaves the summary: back to the results through a fresh scan. */
export function backToResults() {
  state.run = null;
  startScan();
}
