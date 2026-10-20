// Idle / scanning / scan-error screens.

import { h, clear } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import { state, bus, settings } from '../state.js';
import { startScan, go, openLogs } from '../actions.js';
import { button } from '../components/widgets.js';
import { welcomeCard } from './welcome.js';

export const SCAN_STEPS = ['environment', 'winget_upgrade', 'winget_list', 'registry', 'chocolatey', 'scoop',
  'security_catalog', 'release_notes', 'processes', 'lookup', 'windows_update'];
const BACKGROUND = ['lookup', 'windows_update'];

export function mountScan(container) {
  const view = h('div', { class: 'view scan-view' });
  container.append(view);
  if (state.screen === 'scanning') return mountScanning(view);
  if (state.screen === 'scan-error') return mountError(view);
  return mountIdle(view);
}

function emblem(spinning) {
  return h('div', { class: ['emblem', spinning ? 'is-scanning' : null], 'aria-hidden': 'true' },
    h('div', { class: 'emblem-orbit' }),
    h('div', { class: 'emblem-orbit two' }),
    h('img', { src: 'assets/logo.svg', alt: '', width: 88, height: 88 }));
}

// ------------------------------------------------------------------ idle

function mountIdle(view) {
  const scroll = h('div', { class: 'view-scroll' });
  scroll.append(h('div', { class: 'scan-center' },
    emblem(false),
    h('h1', null, t('idle.title')),
    h('p', { class: 'lead' }, t('idle.text')),
    button({
      label: t('idle.start'), icon: 'search', variant: 'royal', size: 'large',
      tip: [t('idle.start'), t('idle.startTip')], onClick: () => startScan(),
    }),
    !settings().scanOnStart
      ? h('p', { class: 'caption subtle idle-note' }, t('idle.autoOff'), ' ',
        h('button', { type: 'button', class: 'linklike', onClick: () => go('settings'), tip: [t('nav.settings'), t('idle.autoOffTip')] }, t('idle.openSettings')))
      : null,
    welcomeCard()));
  view.append(scroll);
  return () => {};
}

// ------------------------------------------------------------------ error

function mountError(view) {
  const scroll = h('div', { class: 'view-scroll' });
  scroll.append(h('div', { class: 'scan-center' },
    h('div', { class: 'empty-icon danger' }, icon('error')),
    h('h1', null, t('scan.errorTitle')),
    h('p', { class: 'lead' }, t('scan.errorText')),
    h('pre', { class: 'error-detail' }, state.scan?.error || ''),
    h('div', { class: 'row-flex' },
      button({ label: t('scan.retry'), icon: 'refresh', variant: 'primary', tip: [t('scan.retry'), t('scan.retryTip')], onClick: () => startScan() }),
      button({ label: t('common.openLogs'), icon: 'folder', tip: [t('common.openLogs'), t('common.openLogsTip')], onClick: openLogs }))));
  view.append(scroll);
  return () => {};
}

// ------------------------------------------------------------------ scanning

function stepStateOf(step) {
  if (step === 'lookup') {
    if (!settings().lookupUnmanaged) return 'skipped';
    return state.lookup?.state || 'later';
  }
  if (step === 'windows_update') {
    if (!settings().windowsUpdate) return 'skipped';
    return state.wu?.searching ? 'started' : 'later';
  }
  return state.scan?.steps?.[step]?.state || 'pending';
}

function stepIcon(st) {
  switch (st) {
    case 'started': return h('span', { class: 'step-spin' });
    case 'done': return h('span', { class: 'step-ic ok' }, icon('check'));
    case 'failed': return h('span', { class: 'step-ic fail' }, icon('close'));
    case 'skipped': return h('span', { class: 'step-ic skip' }, icon('minus'));
    case 'later': return h('span', { class: 'step-ic later' }, icon('clock'));
    default: return h('span', { class: 'step-ic pending' });
  }
}

function mountScanning(view) {
  const scroll = h('div', { class: 'view-scroll' });
  const bar = h('div', { class: 'scan-bar-fill' });
  const barWrap = h('div', {
    class: 'scan-bar', role: 'progressbar', 'aria-label': t('scan.progressLabel'),
    'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-valuenow': 0,
  }, bar);
  const elapsed = h('span', { class: 'num' }, fmt.duration(0));
  const stepsList = h('ol', { class: 'scan-steps' });
  const rows = new Map();

  for (const step of SCAN_STEPS) {
    const status = h('span', { class: 'step-status caption' });
    const detail = h('span', { class: 'step-detail caption subtle ellipsis' });
    const ic = h('span', { class: 'step-icon' });
    const mini = h('span', { class: 'step-mini' }, h('span'));
    const li = h('li', {
      class: 'scan-step',
      tabindex: 0,
      tip: [t(`scan.steps.${step}.label`), t(`scan.steps.${step}.tip`)],
    },
    ic,
    h('div', { class: 'grow' },
      h('div', { class: 'step-label' }, t(`scan.steps.${step}.label`)),
      h('div', { class: 'caption subtle step-desc' }, t(`scan.steps.${step}.desc`))),
    h('div', { class: 'step-side' }, status, detail, mini));
    rows.set(step, { li, ic, status, detail, mini });
    stepsList.append(li);
  }

  scroll.append(h('div', { class: 'scan-split' },
    h('div', { class: 'scan-intro' },
      emblem(true),
      h('h1', { 'aria-live': 'polite' }, t('scan.title')),
      h('p', { class: 'lead' }, t('scan.text')),
      h('div', { class: 'scan-meter' }, barWrap,
        h('div', { class: 'caption subtle row-flex scan-meter-text' },
          h('span', null, t('scan.elapsed')), elapsed))),
    stepsList));
  view.append(scroll);

  function update() {
    const steps = SCAN_STEPS.filter((s) => !BACKGROUND.includes(s));
    let done = 0;
    for (const step of SCAN_STEPS) {
      const row = rows.get(step);
      const st = stepStateOf(step);
      const p = step === 'lookup' ? state.lookup : step === 'windows_update' ? null : state.scan?.steps?.[step];
      if (row.state !== st) {
        row.state = st;
        row.li.dataset.state = st;
        clear(row.ic).append(stepIcon(st));
      }
      row.status.textContent = t(`scan.state.${st}`);
      row.detail.textContent = p?.detail || '';
      const frac = typeof p?.fraction === 'number' ? Math.max(0, Math.min(1, p.fraction)) : null;
      row.mini.classList.toggle('visible', st === 'started' && frac !== null);
      row.mini.firstChild.style.transform = `scaleX(${frac ?? 0})`;
      if (!BACKGROUND.includes(step)) {
        if (st === 'done' || st === 'failed' || st === 'skipped') done += 1;
        else if (st === 'started') done += frac !== null ? frac * 0.9 : 0.35;
      }
    }
    const fraction = done / steps.length;
    bar.style.transform = `scaleX(${Math.max(0.02, fraction)})`;
    barWrap.setAttribute('aria-valuenow', String(Math.round(fraction * 100)));
  }

  const timer = setInterval(() => {
    elapsed.textContent = fmt.duration((Date.now() - (state.scan?.startedAt || Date.now())) / 1000);
  }, 1000);
  update();
  const off = bus.on('scan-progress', update);
  return () => { clearInterval(timer); off(); };
}

