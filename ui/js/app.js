// MAJestic UI bootstrap: settings, language, theme, navigation rail and routing.

import { h, clear } from './dom.js';
import { icon } from './icons.js';
import { invoke, setWindowTheme, errorText } from './api.js';
import { t, setLanguage, resolveLanguage } from './i18n.js';
import { state, bus, DEFAULT_SETTINGS, settings, runActive } from './state.js';
import { attachBackendListeners, startScan, go } from './actions.js';
import { isSelectable, priorityOf } from './model.js';
import { initTooltips, hide as hideTooltip } from './components/tooltip.js';
import { toast } from './components/toast.js';
import { mountScan } from './views/scan.js';
import { mountResults } from './views/results.js';
import { mountProgress } from './views/progress.js';
import { mountSummary } from './views/summary.js';
import { mountHistory } from './views/history.js';
import { mountSettings } from './views/settings.js';
import { checkAppUpdate, announceUpdated, openAppUpdate } from './appupdate.js';

const root = document.getElementById('app');
const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

let mainEl = null;
let railEl = null;
let unmountView = null;
let mountedKey = null;

// ------------------------------------------------------------------ theme

function applyTheme() {
  const pref = ['system', 'light', 'dark'].includes(settings().theme) ? settings().theme : 'system';
  const effective = pref === 'system' ? (darkQuery.matches ? 'dark' : 'light') : pref;
  state.theme = effective;
  document.documentElement.dataset.theme = effective;
  setWindowTheme(pref === 'system' ? null : pref);
}

function applyLanguage() {
  const lang = resolveLanguage(settings().language, state.info?.uiLanguage);
  const changed = lang !== state.lang || document.documentElement.lang !== lang;
  state.lang = lang;
  setLanguage(lang, state.info?.uiLanguage);
  return changed;
}

// ------------------------------------------------------------------ shell

function navButton(route, ic, labelKey, tipKey) {
  const btn = h('button', {
    type: 'button',
    class: 'nav-btn',
    dataset: { route },
    'aria-current': state.route === route ? 'page' : undefined,
    tip: [t(labelKey), t(tipKey)],
    onClick: () => go(route),
  }, icon(ic), h('span', null, t(labelKey)));
  return btn;
}

function renderShell() {
  clear(root);
  root.classList.remove('booting');
  root.removeAttribute('aria-busy');
  railEl = h('nav', { class: 'rail', 'aria-label': t('nav.label') },
    h('div', { class: 'rail-logo', tip: [t('app.name'), t('app.tagline')] },
      h('img', { src: 'assets/logo.svg', alt: t('app.name'), width: 36, height: 36 })),
    navButton('updates', 'updates', 'nav.updates', 'nav.updatesTip'),
    navButton('history', 'history', 'nav.history', 'nav.historyTip'),
    h('div', { class: 'rail-spacer' }),
    navButton('settings', 'settings', 'nav.settings', 'nav.settingsTip'));
  updateAppUpdateButton();
  mainEl = h('main', { class: 'main', id: 'main' });
  root.append(h('div', { class: 'app' }, railEl, mainEl));
  updateNavBadge();
}

/** Gold "new version" button above Settings while a newer MAJestic exists. */
function updateAppUpdateButton() {
  railEl?.querySelector('.nav-update')?.remove();
  const release = state.appUpdate;
  if (!railEl || !release) return;
  const btn = h('button', {
    type: 'button',
    class: 'nav-btn nav-update',
    tip: [t('appUpdate.railTitle', { version: release.version }), t('appUpdate.railTip')],
    onClick: openAppUpdate,
  }, icon('arrow-up'), h('span', null, t('appUpdate.rail')));
  railEl.insertBefore(btn, railEl.querySelector('[data-route="settings"]'));
}

function updateNav() {
  railEl?.querySelectorAll('.nav-btn').forEach((b) => {
    if (b.dataset.route === state.route) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  });
}

function updateNavBadge() {
  const btn = railEl?.querySelector('[data-route="updates"]');
  if (!btn) return;
  btn.querySelector('.nav-badge')?.remove();
  let badgeEl = null;
  if (runActive() || state.screen === 'scanning') {
    badgeEl = h('span', { class: 'nav-badge is-busy', 'aria-hidden': 'true' });
  } else if (state.result) {
    const updates = state.items.filter(isSelectable);
    if (updates.length) {
      const critical = updates.some((i) => priorityOf(i) === 'critical');
      badgeEl = h('span', { class: ['nav-badge', critical ? 'is-critical' : null], 'aria-hidden': 'true' },
        updates.length > 99 ? '99+' : String(updates.length));
    }
  }
  if (badgeEl) btn.appendChild(badgeEl);
}

// ------------------------------------------------------------------ routing

function viewKey() {
  if (state.route !== 'updates') return state.route;
  return state.screen;
}

function renderView(force = false) {
  const key = viewKey();
  if (!force && key === mountedKey && unmountView) return;
  hideTooltip(true);
  try { unmountView?.(); } catch (e) { console.error(e); }
  unmountView = null;
  clear(mainEl);
  mountedKey = key;
  const mounts = {
    idle: mountScan,
    scanning: mountScan,
    'scan-error': mountScan,
    results: mountResults,
    progress: mountProgress,
    summary: mountSummary,
    history: mountHistory,
    settings: mountSettings,
  };
  const mount = mounts[key] || mountScan;
  try {
    unmountView = mount(mainEl) || null;
  } catch (e) {
    console.error('[view]', key, e);
    mainEl.append(h('div', { class: 'empty' }, h('h2', null, t('errors.viewTitle')), h('p', null, errorText(e))));
  }
  updateNav();
  updateNavBadge();
  document.title = `${t('app.name')} — ${t(`nav.${state.route}`)}`;
}

function rerenderAll() {
  renderShell();
  renderView(true);
}

// ------------------------------------------------------------------ boot

async function boot() {
  initTooltips();
  applyTheme();
  darkQuery.addEventListener?.('change', applyTheme);

  const listenersReady = attachBackendListeners().catch((e) => console.error('[listen]', e));
  const [info, loaded] = await Promise.allSettled([invoke('app_info'), invoke('get_settings')]);
  state.info = info.status === 'fulfilled' && info.value ? info.value : { appVersion: '?', uiLanguage: navigator.language };
  state.settings = { ...DEFAULT_SETTINGS, ...(loaded.status === 'fulfilled' && loaded.value ? loaded.value : {}) };
  applyLanguage();
  applyTheme();
  renderShell();
  renderView(true);
  if (loaded.status === 'rejected') toast(t('settings.loadFailed', { error: errorText(loaded.reason) }), { tone: 'danger', duration: 6000 });

  bus.on('route', () => renderView());
  bus.on('screen', () => { renderView(); updateNavBadge(); });
  bus.on('item-updated', updateNavBadge);
  bus.on('updates-done', updateNavBadge);
  bus.on('app-update', updateAppUpdateButton);
  bus.on('settings', () => {
    applyTheme();
    if (applyLanguage()) rerenderAll();
  });

  document.addEventListener('keydown', (e) => {
    // F5 / Ctrl+R would reload the web view (and lose a running batch): rescan instead.
    if (e.key === 'F5' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r')) {
      e.preventDefault();
      if (!runActive() && state.route === 'updates' && state.screen !== 'scanning') startScan();
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      const search = document.querySelector('.results-view .search input');
      if (search) { e.preventDefault(); search.focus(); search.select(); }
    }
  });

  await listenersReady;
  announceUpdated();
  if (state.settings.scanOnStart) startScan();
  checkAppUpdate();
}

boot().catch((e) => {
  console.error('[boot]', e);
  root.textContent = String(e?.message || e);
});
