// History of installations (get_history), grouped by day, with rollback.

import { h, clear } from '../dom.js';
import { t, fmt } from '../i18n.js';
import { invoke, errorText } from '../api.js';
import { itemByKey, runActive } from '../state.js';
import { startJobs } from '../actions.js';
import { sourceInfo, failureInfo, normSource } from '../model.js';
import { button, badge, emptyState, spinner, infoDot } from '../components/widgets.js';
import { confirmDialog } from '../components/dialog.js';
import { chooseVersion } from '../components/versions.js';
import { toast } from '../components/toast.js';
import { icon } from '../icons.js';

const RESULT_TONES = { succeeded: 'success', failed: 'danger', cancelled: 'uncertain', skipped: 'uncertain' };
const ACTIONS = ['install_version', 'force_reinstall', 'interactive', 'store_update_scan', 'uninstall'];
const actionKey = (a) => (ACTIONS.includes(a) ? a : 'install_version');
const focusable = (el) => { el.tabIndex = 0; return el; };

export function mountHistory(container) {
  const view = h('div', { class: 'view history-view' });
  const body = h('div', { class: 'page-body' }, h('div', { class: 'loading-line' }, spinner(true), t('history.loading')));
  const clearBtn = button({
    label: t('history.clear'), icon: 'trash', variant: 'subtle',
    tip: [t('history.clear'), t('history.clearTip')], disabled: true,
    onClick: onClear,
  });
  view.append(h('div', { class: 'view-scroll' },
    h('div', { class: 'page-head' },
      h('div', { class: 'grow' },
        h('h1', null, t('history.title')),
        h('p', { class: 'muted' }, t('history.subtitle'), infoDot(t('history.title'), t('history.info')))),
      clearBtn),
    body));
  container.append(view);
  let alive = true;

  async function load() {
    try {
      const entries = await invoke('get_history');
      if (!alive) return;
      render(Array.isArray(entries) ? entries.filter((e) => e && typeof e === 'object') : []);
    } catch (err) {
      if (!alive) return;
      clear(body).append(emptyState({ icon: 'error', title: t('history.errorTitle'), text: t('history.errorText', { error: errorText(err) }),
        action: button({ label: t('scan.retry'), icon: 'refresh', tip: [t('scan.retry'), t('history.retryTip')], onClick: load }) }));
    }
  }

  function render(entries) {
    clear(body);
    clearBtn.disabled = !entries.length;
    if (!entries.length) {
      body.append(emptyState({ icon: 'history', title: t('history.emptyTitle'), text: t('history.emptyText') }));
      return;
    }
    const days = new Map();
    for (const e of entries) {
      const d = new Date(e.date);
      const key = Number.isNaN(d.getTime()) ? 'unknown' : `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
      if (!days.has(key)) days.set(key, { date: e.date, list: [] });
      days.get(key).list.push(e);
    }
    const table = h('table', { class: 'history-table' },
      h('thead', null, h('tr', null,
        th(t('history.col.time'), t('history.col.timeTip')),
        th(t('history.col.program'), t('history.col.programTip')),
        th(t('history.col.versions'), t('history.col.versionsTip')),
        th(t('history.col.result'), t('history.col.resultTip')),
        th(t('history.col.duration'), t('history.col.durationTip'), 'num-col'),
        th(t('history.col.actions'), t('history.col.actionsTip'), 'act-col'))));
    for (const { date, list } of days.values()) {
      const tbody = h('tbody', null,
        h('tr', { class: 'day-row' }, h('th', { colspan: 6, scope: 'rowgroup' }, fmt.dayLabel(date), h('span', { class: 'caption subtle' }, ` — ${t('history.dayCount', { count: list.length })}`))));
      for (const e of list) tbody.append(rowFor(e));
      table.append(tbody);
    }
    body.append(h('div', { class: 'table-wrap' }, table));
  }

  function th(label, tip, cls) {
    return h('th', { scope: 'col', class: cls }, h('span', { tip: [label, tip], tabindex: 0 }, label));
  }

  function rowFor(e) {
    const result = ['succeeded', 'failed', 'cancelled', 'skipped'].includes(e.result) ? e.result : 'unknown';
    const src = sourceInfo(e.source);
    const action = e.action?.type || 'upgrade';
    let resultTip = t(`history.resultTip.${result}`);
    if (result === 'failed' && e.failure) resultTip = `${failureInfo(e.failure).title}. ${failureInfo(e.failure).text}`;
    if (e.code !== null && e.code !== undefined && result === 'failed') resultTip += ` ${t('summary.code', { code: e.code })}`;
    const dl = Number(e.downloadSecs) || 0;
    const inst = Number(e.installSecs) || 0;
    const total = Math.max(Number(e.durationSecs) || 0, dl + inst);
    const canRollback = result === 'succeeded' && e.fromVersion && ['upgrade', 'install_version', 'force_reinstall'].includes(action)
      && ['winget', 'ms_store', 'chocolatey'].includes(normSource(e.source)) && e.fromVersion !== e.toVersion;
    return h('tr', null,
      h('td', { class: 'num subtle' }, fmt.time(e.date)),
      h('td', null,
        h('div', { class: 'h-prog' },
          h('span', { class: 'strong ellipsis' }, e.name || e.id || e.key),
          h('span', { class: 'source-badge', tip: src.tip, tabindex: 0 }, icon(src.icon), src.label),
          action !== 'upgrade' ? focusable(badge({ text: t(`progress.action.${actionKey(action)}`), outline: true, tip: [t('history.actionTitle'), t(`progress.action.${actionKey(action)}Tip`)] })) : null)),
      h('td', { class: 'ver h-ver' }, action === 'uninstall'
        ? [h('span', { class: 'nowrap' }, e.fromVersion || t('version.unknown')), ' ', h('span', { class: 'nowrap' }, h('span', { class: 'arrow-inline' }, '→ '), h('strong', null, t('uninstall.done')))]
        : e.fromVersion || e.toVersion
        ? [h('span', { class: 'nowrap' }, e.fromVersion || t('version.unknown')), ' ', h('span', { class: 'nowrap' }, h('span', { class: 'arrow-inline' }, '→ '), h('strong', null, e.toVersion || '?'))]
        : '—'),
      h('td', null,
        h('div', { class: 'row-flex wrap' },
          focusable(badge({ text: t(`history.result.${result}`), tone: RESULT_TONES[result] || 'uncertain', icon: result === 'succeeded' ? 'success' : result === 'failed' ? 'error' : 'minus', tip: [t(`history.result.${result}`), resultTip] })),
          e.rebootRequired ? focusable(badge({ text: t('summary.rebootBadge'), tone: 'gold', icon: 'restart', tip: [t('summary.rebootTitle'), t('summary.rebootBadgeTip')] })) : null)),
      h('td', { class: 'num num-col', tip: total ? [t('history.durationTitle'), t('history.durationDetail', { download: fmt.duration(dl), install: fmt.duration(inst) })] : undefined, tabindex: total ? 0 : undefined },
        total ? fmt.duration(total) : '—'),
      h('td', { class: 'act-col' },
        canRollback ? button({
          label: t('history.rollback'), icon: 'undo', size: 'small', variant: 'subtle', ariaLabel: t('history.rollbackAria', { name: e.name || e.id, version: e.fromVersion }),
          tip: [t('history.rollback'), runActive() ? t('run.alreadyRunning') : t('history.rollbackTip', { version: e.fromVersion })],
          disabled: runActive(),
          onClick: () => rollback(e),
        }) : null));
  }

  async function rollback(e) {
    const item = itemByKey(e.key);
    const version = await chooseVersion({
      id: e.id, source: e.source, name: e.name,
      installed: item?.installedVersion ?? e.toVersion, latest: null,
      preselect: e.fromVersion, rollback: true,
    });
    if (!version) return;
    startJobs([{
      key: e.key,
      source: e.source,
      id: e.id,
      name: e.name,
      action: { type: 'install_version', version },
      fromVersion: item?.installedVersion ?? e.toVersion ?? null,
      toVersion: version,
      scope: item?.scope ?? null,
      includeUnknown: false,
      includePinned: !!item?.pinned,
      downloadSize: null,
      installerType: e.installerType ?? item?.installerType ?? null,
    }]);
  }

  async function onClear() {
    const ok = await confirmDialog({
      title: t('history.clearConfirmTitle'), text: t('history.clearConfirmText'),
      confirmLabel: t('history.clearConfirm'), cancelLabel: t('common.cancel'), danger: true, icon: 'trash',
    });
    if (!ok) return;
    try {
      await invoke('clear_history');
      toast(t('history.cleared'), { tone: 'success' });
      load();
    } catch (err) {
      toast(t('history.clearFailed', { error: errorText(err) }), { tone: 'danger' });
    }
  }

  load();
  return () => { alive = false; };
}
