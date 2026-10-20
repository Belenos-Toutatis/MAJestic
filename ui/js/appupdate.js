// MAJestic updating itself: check at start then every 6 hours (or from
// Settings › About), rail button, update dialog
// (download with progress, SHA-256 check and restart are done by the backend).

import { h, clear } from './dom.js';
import { t, fmt, has } from './i18n.js';
import { invoke, listen, errorText } from './api.js';
import { state, bus, settings, runActive } from './state.js';
import { openUrl } from './actions.js';
import { openDialog } from './components/dialog.js';
import { banner } from './components/widgets.js';
import { toast } from './components/toast.js';

/** While the window stays open, checks again this often. */
const RECHECK_EVERY = 6 * 3600e3;
let checking = null;

export const appUpdateChecking = () => !!checking;

/**
 * Asks the backend for a newer release (does nothing when online sources are
 * off). Records the result in `state.appUpdateCheck` and emits
 * `app-update-check` when it starts and ends. `manual`: asked from Settings ›
 * About, which shows the result itself.
 */
export function checkAppUpdate({ manual = false } = {}) {
  if (!settings().onlineIntel) return Promise.resolve(null);
  if (checking) return checking;
  checking = (async () => {
    try {
      const release = await invoke('check_app_update');
      state.appUpdateCheck = { at: Date.now(), error: null };
      const known = state.appUpdate?.version;
      state.appUpdate = release?.version ? release : null;
      if ((state.appUpdate?.version || null) !== (known || null)) bus.emit('app-update', state.appUpdate);
      if (state.appUpdate && state.appUpdate.version !== known && !manual) {
        toast(t('appUpdate.toast', { version: state.appUpdate.version }), { tone: 'info', duration: 6000 });
      }
      return state.appUpdate;
    } catch (e) {
      console.warn('[app update]', e);
      state.appUpdateCheck = { at: Date.now(), error: errorText(e) };
      return null;
    } finally {
      checking = null;
      bus.emit('app-update-check');
    }
  })();
  bus.emit('app-update-check');
  return checking;
}

/** Checks again every 6 hours while the window stays open (timestamps, so a PC put to sleep catches up on waking). */
export function startAppUpdateTimer() {
  setInterval(() => {
    if (Date.now() - (state.appUpdateCheck?.at || 0) >= RECHECK_EVERY) checkAppUpdate();
  }, 10 * 60e3);
}

/** Right after an update: tells the user it worked. */
export function announceUpdated() {
  if (state.info?.updatedFrom) {
    toast(t('appUpdate.done', { from: state.info.updatedFrom, version: state.info.appVersion }), { tone: 'success', duration: 7000 });
  }
}

// Release notes are Markdown: keep headings, bullets and paragraphs as text.
function notesNodes(markdown) {
  const plain = (s) => s
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/\*\*|__|`/g, '')
    .replace(/(^|\s)[*_]([^*_]+)[*_](?=\s|$|[.,;:!?])/g, '$1$2')
    .trim();
  const blocks = [];
  let current = null;
  for (const raw of String(markdown || '').split(/\r?\n/)) {
    const line = raw.trimEnd();
    if (!line.trim()) { current = null; continue; }
    const heading = line.match(/^#{1,6}\s+(.*)$/);
    const bullet = line.match(/^\s*[-*]\s+(.*)$/);
    if (heading) {
      blocks.push({ type: 'h', text: heading[1] });
      current = null;
    } else if (bullet) {
      current = { type: 'li', text: bullet[1] };
      blocks.push(current);
    } else if (current) {
      current.text += ` ${line.trim()}`;
    } else {
      current = { type: 'p', text: line.trim(), muted: /^\*[^*]/.test(line.trim()) };
      blocks.push(current);
    }
  }
  const nodes = [];
  let list = null;
  for (const b of blocks) {
    if (b.type === 'li') {
      if (!list) { list = h('ul'); nodes.push(list); }
      list.append(h('li', null, plain(b.text)));
      continue;
    }
    list = null;
    nodes.push(b.type === 'h' ? h('h3', null, plain(b.text)) : h('p', { class: b.muted ? 'muted' : null }, plain(b.text)));
  }
  return nodes;
}

const PHASES = ['download', 'install', 'restart'];

/** The update dialog. */
export function openAppUpdate() {
  const release = state.appUpdate;
  if (!release) return;
  let installBtn = null;
  let pageBtn = null;
  let laterBtn = null;
  let unlisten = null;

  const fill = h('div', { class: 'update-fill' });
  const status = h('div', { class: 'update-status' });
  const progress = h('div', { class: 'update-progress', hidden: true },
    h('div', { class: 'update-track', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': 100 }, fill),
    status);
  const errorBox = h('div', { class: 'update-error' });

  const setProgress = (p) => {
    progress.hidden = false;
    const phase = PHASES.includes(p?.phase) ? p.phase : 'download';
    const pct = p?.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null;
    const width = phase === 'download' ? (pct ?? 5) : 100;
    fill.style.width = `${width}%`;
    fill.parentElement.setAttribute('aria-valuenow', String(width));
    fill.classList.toggle('done', phase !== 'download');
    status.textContent = phase === 'download'
      ? t('appUpdate.phase.download', { percent: pct === null ? '…' : fmt.percent(pct / 100), size: p?.total ? fmt.bytes(p.total) : '' })
      : t(`appUpdate.phase.${phase}`);
  };

  const showError = (err) => {
    const kind = err && typeof err === 'object' && err.kind ? err.kind : 'network';
    const key = `appUpdate.errors.${kind}`;
    const text = has(key) ? t(key) : t('appUpdate.errors.network');
    clear(errorBox).append(banner({ tone: 'danger', title: t('appUpdate.failed'), text: `${text} ${t('appUpdate.failedManual')}` }));
    if (err?.message) console.warn('[app update]', errorText(err));
    progress.hidden = true;
    installBtn.disabled = false;
    laterBtn.disabled = false;
    const label = installBtn.querySelector('.btn-label');
    if (label) label.textContent = t('appUpdate.retry');
  };

  const install = async () => {
    if (runActive()) return;
    installBtn.disabled = true;
    laterBtn.disabled = true;
    clear(errorBox);
    setProgress({ phase: 'download', downloaded: 0, total: release.size });
    try {
      unlisten = unlisten || await listen('app-update-progress', setProgress);
      await invoke('install_app_update');
      // The backend starts the new version and closes this window.
      setProgress({ phase: 'restart' });
    } catch (err) {
      showError(err);
    }
  };

  const busy = runActive();
  const body = h('div', { class: 'update-dialog' },
    h('div', { class: 'update-versions' },
      h('div', null, h('span', { class: 'caption' }, t('appUpdate.current')), h('strong', null, state.info?.appVersion || '?')),
      h('span', { class: 'arrow-inline', 'aria-hidden': 'true' }, '→'),
      h('div', null, h('span', { class: 'caption' }, t('appUpdate.new')), h('strong', null, release.version)),
      release.size ? h('span', { class: 'caption subtle grow-right' }, fmt.bytes(release.size)) : null),
    release.notes ? h('div', { class: 'update-notes', tabindex: 0, tip: [t('appUpdate.notes'), t('appUpdate.notesTip')] }, notesNodes(release.notes)) : null,
    h('p', { class: 'muted' }, t('appUpdate.how')),
    busy ? banner({ tone: 'warning', text: t('appUpdate.errors.busy') }) : null,
    progress,
    errorBox);

  openDialog({
    title: t('appUpdate.title', { version: release.version }),
    icon: 'sparkle',
    width: 600,
    body,
    actions: [
      { label: t('appUpdate.later'), value: null, tip: [t('appUpdate.later'), t('appUpdate.laterTip')], ref: (b) => { laterBtn = b; } },
      { label: t('appUpdate.page'), icon: 'external', tip: [t('appUpdate.page'), release.pageUrl], onClick: () => openUrl(release.pageUrl), ref: (b) => { pageBtn = b; } },
      {
        label: t('appUpdate.install'), icon: 'download', variant: 'royal', autofocus: !busy, disabled: busy,
        tip: [t('appUpdate.install'), busy ? t('appUpdate.errors.busy') : t('appUpdate.installTip')],
        onClick: install, ref: (b) => { installBtn = b; },
      },
    ],
  }).then(() => { try { unlisten?.(); } catch { /* ignore */ } });
  return pageBtn;
}
