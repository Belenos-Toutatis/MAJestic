// Installation progress: royal ring (purple → gold), calm ETA, per-program rows.

import { h, clear, reducedMotion } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt, roundEta, etaText } from '../i18n.js';
import { state, bus, itemByKey } from '../state.js';
import { cancelRun } from '../actions.js';
import { stateType, isFinal, failureInfo, needsElevation } from '../model.js';
import { avatar } from '../components/avatar.js';
import { button, emptyState, infoDot } from '../components/widgets.js';

/** Remaining-time display that never jumps around: smoothed, counted down
 *  locally between backend updates, rounded to calm steps, and slow to go up. */
class CalmEta {
  constructor() { this.est = null; this.at = 0; this.shown = null; this.upTicks = 0; }
  feed(secs) {
    if (secs === null || secs === undefined || !Number.isFinite(Number(secs))) return;
    const now = performance.now();
    secs = Math.max(0, Number(secs));
    if (this.est === null) this.est = secs;
    else {
      const projected = Math.max(0, this.est - (now - this.at) / 1000);
      this.est = projected + 0.3 * (secs - projected);
    }
    this.at = now;
  }
  current() {
    return this.est === null ? null : Math.max(0, this.est - (performance.now() - this.at) / 1000);
  }
  tick() {
    const c = this.current();
    if (c === null) return this.shown;
    const r = c < 60 ? 30 : roundEta(c);
    if (this.shown === null || r <= this.shown) { this.shown = r; this.upTicks = 0; }
    else if (r > this.shown * 1.15 || r - this.shown >= 90) {
      this.upTicks += 1;
      if (this.upTicks >= 3) { this.shown = r; this.upTicks = 0; }
    }
    return this.shown;
  }
}

const ACTIVE = ['closing_apps', 'starting', 'downloading', 'installing'];

function stateIcon(type) {
  if (type === 'succeeded') {
    return h('span', { class: 'pg-ic ok', html: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle class="ic-circle" cx="12" cy="12" r="10"/><path class="ic-check" d="M7.4 12.4l3.1 3.1 6.1-6.3"/></svg>' });
  }
  if (type === 'failed') {
    return h('span', { class: 'pg-ic fail', html: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle class="ic-circle" cx="12" cy="12" r="10"/><path class="ic-x" d="M8.8 8.8l6.4 6.4M15.2 8.8l-6.4 6.4"/></svg>' });
  }
  if (type === 'skipped' || type === 'cancelled') return h('span', { class: 'pg-ic muted-ic' }, icon('minus'));
  if (ACTIVE.includes(type)) return h('span', { class: ['pg-ic', 'busy', type === 'closing_apps' ? 'amber' : null] });
  return h('span', { class: 'pg-ic wait' });
}

export function mountProgress(container) {
  const run = state.run;
  const view = h('div', { class: 'view progress-view' });
  container.append(view);
  if (!run) {
    view.append(emptyState({ icon: 'updates', title: t('progress.noneTitle'), text: t('progress.noneText') }));
    return () => {};
  }

  const reduce = reducedMotion();
  const total = run.order.length;
  const eta = new CalmEta();
  const rowEtas = new Map(); // key -> CalmEta

  // ---------------------------------------------------------------- hero
  const pctEl = h('span', { class: 'ring-pct num' }, fmt.percent(0));
  const countEl = h('span', { class: 'ring-count' }, t('progress.count', { done: 0, total }));
  const ring = h('div', {
    class: 'ring', role: 'progressbar', 'aria-label': t('progress.ringLabel'),
    'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-valuenow': 0,
    tip: [t('progress.ringTitle'), t('progress.ringTip')], tabindex: 0,
  },
  h('div', { class: 'ring-glow' }),
  h('div', { class: 'ring-track' }),
  h('div', { class: 'ring-fill' }),
  h('div', { class: 'ring-sheen' }),
  h('div', { class: 'ring-cap' }),
  h('div', { class: 'ring-head' }),
  h('div', { class: 'ring-center' },
    h('span', { class: 'ring-crown' }, icon('crown')),
    pctEl, countEl));

  const only = (type) => !!run?.jobs?.length && run.jobs.every((j) => j.action?.type === type);
  const titleEl = h('h1', null, t(only('install') ? 'progress.titleInstall' : only('uninstall') ? 'progress.titleUninstall' : 'progress.title'));
  const currentEl = h('p', { class: 'pg-current', 'aria-live': 'polite' }, t('progress.preparing'));
  const etaEl = h('div', { class: 'pg-eta', tip: [t('progress.etaTitle'), t('progress.etaTip')], tabindex: 0 }, t('eta.computing'));
  const elapsedEl = h('span', { class: 'num' }, fmt.duration(0));
  const okEl = h('span', { class: 'num' }, t('progress.okCount', { count: 0 }));
  const failEl = h('span', { class: 'num' }, t('progress.failCount', { count: 0 }));
  const cancelBtn = button({
    label: t('progress.cancel'), icon: 'stop',
    tip: [t('progress.cancel'), t('progress.cancelTip')],
    onClick: () => cancelRun(),
  });
  const adminNote = run.jobs.some(needsElevation) && run.allowElevation !== false
    ? h('span', { class: 'pg-chip admin', tabindex: 0, tip: [t('progress.adminTitle'), t('progress.adminTip')] }, icon('shield'), t('progress.admin'))
    : null;

  const hero = h('section', { class: 'pg-hero' },
    h('div', { class: 'ring-wrap' }, ring),
    h('div', { class: 'pg-info' },
      titleEl,
      currentEl,
      etaEl,
      h('div', { class: 'pg-chips' },
        h('span', { class: 'pg-chip', tabindex: 0, tip: [t('progress.elapsedTitle'), t('progress.elapsedTip')] }, icon('clock'), t('progress.elapsed'), ' ', elapsedEl),
        h('span', { class: 'pg-chip ok', tabindex: 0, tip: [t('progress.okTitle'), t('progress.okTip')] }, icon('success'), okEl),
        h('span', { class: 'pg-chip fail', tabindex: 0, tip: [t('progress.failTitle'), t('progress.failTip')] }, icon('error'), failEl),
        adminNote),
      h('div', { class: 'pg-actions' }, cancelBtn)));

  // ---------------------------------------------------------------- rows
  const list = h('ul', { class: 'pg-list', 'aria-label': t('progress.listLabel') });
  const rows = new Map();
  for (const key of run.order) {
    const entry = run.entries.get(key);
    const job = entry.job;
    const item = itemByKey(key) || { key, name: job.name, iconHint: null };
    const ic = h('span', { class: 'pg-state' }, stateIcon('queued'));
    const phase = h('span', { class: 'pg-phase', tabindex: -1 });
    const detail = h('span', { class: 'pg-detail num ellipsis' });
    const rowEta = h('span', { class: 'pg-row-eta num' });
    const fill = h('div', { class: 'pg-bar-fill' });
    const bar = h('div', { class: 'pg-bar', role: 'progressbar', 'aria-label': t('progress.itemBar', { name: job.name || key }), 'aria-valuemin': 0, 'aria-valuemax': 100 },
      fill, h('div', { class: 'pg-bar-flow' }));
    const versions = job.action?.type === 'install_version'
      ? t('progress.toVersion', { version: job.action.version || '?' })
      : (job.fromVersion || job.toVersion) ? `${job.fromVersion || t('version.unknown')} → ${job.toVersion || '?'}` : '';
    const li = h('li', { class: 'pg-row state-queued', dataset: { key } },
      ic,
      avatar(item),
      h('div', { class: 'pg-main' },
        h('div', { class: 'pg-name-line' },
          h('span', { class: 'pg-name ellipsis' }, job.name || item.name || key),
          versions ? h('span', { class: 'pg-vers caption subtle ver ellipsis' }, versions) : null,
          job.action?.type && job.action.type !== 'upgrade'
            ? h('span', { class: 'pg-action caption', tip: [t(`progress.action.${job.action.type}`), t(`progress.action.${job.action.type}Tip`)] }, t(`progress.action.${job.action.type}`))
            : null),
        bar,
        h('div', { class: 'pg-sub caption' }, phase, detail, h('span', { class: 'grow' }), rowEta)));
    rows.set(key, { li, ic, phase, detail, rowEta, fill, bar, type: 'queued' });
    list.append(li);
  }

  const listWrap = h('div', { class: 'pg-list-wrap' },
    h('div', { class: 'pg-list-head' },
      h('h2', null, t('progress.listTitle')),
      infoDot(t('progress.listTitle'), t('progress.listTip'))),
    list);
  view.append(hero, listWrap);

  // ---------------------------------------------------------------- updates

  function updateRow(key) {
    const row = rows.get(key);
    const entry = run.entries.get(key);
    if (!row || !entry) return;
    const st = entry.state || { type: 'queued' };
    const type = stateType(st);
    const ev = entry.ev || {};
    if (type !== row.type) {
      const prev = row.type;
      row.li.className = `pg-row state-${type}`;
      clear(row.ic).append(stateIcon(type));
      if (prev !== 'queued' || type !== 'queued') row.li.classList.add('just-changed');
      setTimeout(() => row.li.classList.remove('just-changed'), 700);
      row.type = type;
    }
    row.li.classList.toggle('is-current', run.current === key && !isFinal(st));
    row.phase.textContent = t(`progress.phase.${type}`);
    row.phase.dataset.tipTitle = t(`progress.phase.${type}`);
    row.phase.dataset.tipText = t(`progress.phaseTip.${type}`);
    row.phase.setAttribute('data-tip', '');

    let frac = null;
    let indeterminate = false;
    let detail = '';
    switch (type) {
      case 'downloading': {
        const total = Number(ev.totalBytes) || 0;
        const got = Number(ev.downloaded) || 0;
        if (total > 0) frac = Math.min(1, got / total);
        else if (typeof ev.progress === 'number') frac = ev.progress;
        else indeterminate = true;
        detail = total > 0
          ? t('progress.downloadDetail', { done: fmt.bytes(got), total: fmt.bytes(total), speed: ev.speed ? fmt.speed(ev.speed) : '…' })
          : got ? fmt.bytes(got) : '';
        break;
      }
      case 'installing':
        if (typeof ev.progress === 'number') { frac = Math.min(1, Math.max(0, ev.progress)); row.li.classList.add('flowing'); }
        else indeterminate = true;
        detail = ev.message || t('progress.installDetail');
        break;
      case 'closing_apps':
        indeterminate = true;
        detail = ev.message || t('progress.closingDetail');
        break;
      case 'starting':
        indeterminate = true;
        detail = ev.message || t('progress.startingDetail');
        break;
      case 'succeeded':
        frac = 1;
        detail = st.rebootRequired ? t('progress.rebootDetail') : (entry.job.toVersion ? t('progress.successDetail', { version: entry.job.toVersion }) : t('progress.successShort'));
        break;
      case 'failed': {
        const f = failureInfo(st.kind);
        detail = f.title;
        row.phase.dataset.tipText = f.text;
        break;
      }
      case 'skipped':
        detail = st.reason || t('progress.skippedDetail');
        break;
      case 'cancelled':
        detail = t('progress.cancelledDetail');
        break;
      default:
        detail = '';
    }
    row.li.classList.toggle('indeterminate', indeterminate);
    if (frac !== null) {
      row.fill.style.transform = `scaleX(${frac})`;
      row.bar.setAttribute('aria-valuenow', String(Math.round(frac * 100)));
    } else if (type === 'queued') {
      row.fill.style.transform = 'scaleX(0)';
      row.bar.removeAttribute('aria-valuenow');
    } else {
      row.bar.removeAttribute('aria-valuenow');
    }
    row.detail.textContent = detail;
    if (ACTIVE.includes(type)) {
      if (!rowEtas.has(key)) rowEtas.set(key, new CalmEta());
      rowEtas.get(key).feed(ev.etaSecs);
    } else {
      rowEtas.delete(key);
      row.rowEta.textContent = '';
    }
  }

  let lastCurrent = null;
  let userScrolledAt = 0;
  list.addEventListener('wheel', () => { userScrolledAt = Date.now(); }, { passive: true });
  listWrap.addEventListener('wheel', () => { userScrolledAt = Date.now(); }, { passive: true });

  function updateCurrent() {
    const key = run.current || run.order.find((k) => ACTIVE.includes(run.entries.get(k)?.state?.type));
    for (const [k, r] of rows) r.li.classList.toggle('is-current', k === key && !isFinal(run.entries.get(k)?.state));
    const entry = key ? run.entries.get(key) : null;
    if (run.cancelRequested) {
      titleEl.textContent = t('progress.stopping');
    }
    if (entry && !isFinal(entry.state)) {
      const type = stateType(entry.state);
      currentEl.textContent = t(`progress.current.${type}`, { name: entry.job.name || key });
    } else if (doneCount() >= total) {
      currentEl.textContent = t('progress.finishing');
    }
    if (key && key !== lastCurrent) {
      lastCurrent = key;
      if (Date.now() - userScrolledAt > 4000) {
        rows.get(key)?.li.scrollIntoView({ block: 'nearest', behavior: reduce ? 'auto' : 'smooth' });
      }
    }
  }

  function doneCount() {
    let n = 0;
    for (const e of run.entries.values()) if (isFinal(e.state)) n += 1;
    return n;
  }

  function counts() {
    let ok = 0; let fail = 0;
    for (const e of run.entries.values()) {
      if (e.state?.type === 'succeeded') ok += 1;
      if (e.state?.type === 'failed') fail += 1;
    }
    const q = run.queue;
    okEl.textContent = t('progress.okCount', { count: Math.max(ok, q?.succeeded || 0) });
    failEl.textContent = t('progress.failCount', { count: Math.max(fail, q?.failed || 0) });
    failEl.parentElement.classList.toggle('has', Math.max(fail, q?.failed || 0) > 0);
    const done = Math.max(doneCount(), q?.done || 0);
    countEl.textContent = t('progress.count', { done: Math.min(done, total), total: q?.total || total });
  }

  function syncCancel() {
    if (run.cancelRequested) {
      cancelBtn.disabled = true;
      const entry = run.current ? run.entries.get(run.current) : null;
      cancelBtn.querySelector('.btn-label').textContent = entry ? t('progress.cancelling', { name: entry.job.name }) : t('progress.cancellingShort');
      titleEl.textContent = t('progress.stopping');
    }
  }

  // ---------------------------------------------------------------- ring tween

  let shown = 0;
  let target = 0;
  let lastTs = 0;
  let raf = 0;
  let lastPct = -1;

  function computeTarget() {
    const q = run.queue;
    let f = typeof q?.fraction === 'number' ? q.fraction : null;
    if (f === null) {
      // fallback: count-based with the current job's own progress
      let sum = 0;
      for (const e of run.entries.values()) {
        if (isFinal(e.state)) sum += 1;
        else if (typeof e.ev?.progress === 'number') sum += e.ev.progress;
      }
      f = sum / Math.max(1, total);
    }
    target = Math.max(target, Math.min(1, Math.max(0, f)));
  }

  function paint(p) {
    ring.style.setProperty('--p', p.toFixed(4));
    const pct = Math.floor(p * 100);
    if (pct !== lastPct) {
      lastPct = pct;
      pctEl.textContent = fmt.percent(pct / 100);
      ring.setAttribute('aria-valuenow', String(pct));
    }
  }

  function frame(ts) {
    const dt = lastTs ? Math.min(100, ts - lastTs) : 16;
    lastTs = ts;
    if (reduce) shown = target;
    else shown += (target - shown) * (1 - Math.exp(-dt / 450));
    if (Math.abs(target - shown) < 0.0005) shown = target;
    paint(shown);
    raf = requestAnimationFrame(frame);
  }

  // ---------------------------------------------------------------- per-second tick (calm texts)

  function tick() {
    const q = run.queue;
    const elapsed = typeof q?.elapsedSecs === 'number' ? q.elapsedSecs + (Date.now() - (run.queueAt || Date.now())) / 1000 : (Date.now() - run.startedAt) / 1000;
    elapsedEl.textContent = fmt.duration(elapsed);
    const shownEta = eta.tick();
    if (doneCount() >= total) etaEl.textContent = t('progress.almostDone');
    else etaEl.textContent = shownEta === null ? t('eta.computing') : etaText(shownEta);
    for (const [key, calm] of rowEtas) {
      const r = rows.get(key);
      const v = calm.tick();
      if (r) r.rowEta.textContent = v === null ? '' : (v < 60 ? t('eta.lessThanMinuteShort') : t('eta.approx', { duration: fmt.duration(roundEta(v)) }));
    }
  }

  // ---------------------------------------------------------------- wiring

  for (const key of run.order) updateRow(key);
  computeTarget();
  shown = target;
  paint(shown);
  counts();
  updateCurrent();
  syncCancel();
  if (run.queue) eta.feed(run.queue.etaSecs);
  tick();
  raf = requestAnimationFrame(frame);
  const timer = setInterval(tick, 1000);

  const offs = [
    bus.on('job-progress', ({ key }) => {
      if (!rows.has(key)) return;
      updateRow(key);
      computeTarget();
      counts();
      updateCurrent();
    }),
    bus.on('queue-progress', (q) => {
      run.queueAt = Date.now();
      eta.feed(q.etaSecs);
      computeTarget();
      counts();
      updateCurrent();
      syncCancel();
    }),
    bus.on('cancel-requested', syncCancel),
  ];

  return () => {
    cancelAnimationFrame(raf);
    clearInterval(timer);
    offs.forEach((off) => off());
  };
}
