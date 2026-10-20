// Application state (single source of truth) and the event bus views subscribe to.

import { createBus } from './dom.js';

export const bus = createBus();

export const DEFAULT_SETTINGS = {
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
  ignored: [],
  schedule: null,
};

export const state = {
  info: null,            // app_info
  settings: { ...DEFAULT_SETTINGS },
  lang: 'fr',
  theme: 'light',        // effective theme
  route: 'updates',      // updates | history | settings
  screen: 'idle',        // idle | scanning | scan-error | results | progress | summary
  scan: null,            // { steps: { [step]: ScanProgress }, startedAt, error }
  result: null,          // ScanResult without items (warnings, stats, environment, scannedAt)
  lookup: null,          // last ScanProgress of step "lookup"
  items: [],             // UpdateItem[]
  selection: new Set(),  // keys
  ui: {
    tab: 'updates',
    search: '',
    sources: new Set(),  // empty = all
    detailsKey: null,
  },
  run: null,             // see actions.newRun
  appUpdate: null,       // newer MAJestic release (check_app_update)
  appUpdateCheck: null,  // last check: { at, error }
  wu: null,              // { searching, data: WindowsUpdates } (Windows Update phase)
};

export function itemByKey(key) {
  return state.items.find((i) => i.key === key) || null;
}

export function setItems(items) {
  state.items = Array.isArray(items) ? items.filter((i) => i && typeof i === 'object' && i.key) : [];
}

/** Replaces (or adds) the item with the same key. Returns the previous item. */
export function replaceItem(item) {
  if (!item || !item.key) return null;
  const idx = state.items.findIndex((i) => i.key === item.key);
  if (idx < 0) { state.items.push(item); return null; }
  const prev = state.items[idx];
  state.items[idx] = item;
  return prev;
}

export function runActive() {
  return !!(state.run && !state.run.done);
}

export function settings() {
  return state.settings || DEFAULT_SETTINGS;
}

// Per-viewer conveniences (never required): first-run card dismissal.
export const local = {
  get(key, fallback = null) {
    try { const v = localStorage.getItem(`majestic.${key}`); return v === null ? fallback : JSON.parse(v); }
    catch { return fallback; }
  },
  set(key, value) {
    try { localStorage.setItem(`majestic.${key}`, JSON.stringify(value)); } catch { /* storage unavailable */ }
  },
};
