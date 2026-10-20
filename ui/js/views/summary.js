// Summary after `updates-done`: celebration when everything succeeded, reboot
// banner, failures explained with their best next action.

import { h, reducedMotion } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import { state, itemByKey } from '../state.js';
import { stateType, failureInfo, needsElevation } from '../model.js';
import { startJobs, backToResults, go, openUrl, openLogs, webSearchUrl, ignoreItem } from '../actions.js';
import { avatar } from '../components/avatar.js';
import { button, banner, emptyState, badge } from '../components/widgets.js';

function sparkles(host) {
  if (reducedMotion()) return;
  const colors = ['#F0C351', '#FFE09A', '#E9B442', '#A897FF', '#7B5CFF', '#FFFFFF'];
  const layer = h('div', { class: 'sparkles', 'aria-hidden': 'true' });
  const n = 34;
  for (let i = 0; i < n; i++) {
    const angle = (i / n) * Math.PI * 2 + Math.random() * 0.4;
    const dist = 90 + Math.random() * 130;
    const shape = i % 3 === 0 ? 'star' : i % 3 === 1 ? 'diamond' : 'dot';
    layer.append(h('span', {
      class: ['spark', shape],
      style: {
        '--x': `${Math.cos(angle) * dist}px`,
        '--y': `${Math.sin(angle) * dist * 0.55}px`,
        '--r': `${Math.round(Math.random() * 360)}deg`,
        '--d': `${Math.round(Math.random() * 220)}ms`,
        '--s': (0.6 + Math.random() * 0.9).toFixed(2),
        '--c': colors[i % colors.length],
      },
    }));
  }
  host.append(layer);
  setTimeout(() => layer.remove(), 2600);
}

export function mountSummary(container) {
  const run = state.run;
  const view = h('div', { class: 'view summary-view' });
  container.append(view);
  if (!run || !run.done) {
    view.append(emptyState({ icon: 'updates', title: t('progress.noneTitle'), text: t('progress.noneText') }));
    return () => {};
  }

  const entries = run.order.map((k) => run.entries.get(k)).filter(Boolean);
  const by = (type) => entries.filter((e) => stateType(e.state) === type);
  const ok = by('succeeded');
  const failed = by('failed');
  const skipped = [...by('skipped'), ...by('cancelled')];
  const rebootNames = ok.filter((e) => e.state.rebootRequired).map((e) => e.job.name);
  const allGood = ok.length > 0 && !failed.length && !skipped.length;
  const duration = ((run.finishedAt || Date.now()) - run.startedAt) / 1000;

  const tone = allGood ? 'gold' : failed.length && !ok.length ? 'danger' : failed.length ? 'warning' : 'neutral';
  const emblem = h('div', { class: ['sum-emblem', `tone-${tone}`] },
    h('span', { class: 'sum-emblem-ic' }, icon(allGood ? 'crown' : failed.length ? 'warning' : 'success')));

  const onlyUpgrades = entries.every((e) => (e.job.action?.type || 'upgrade') === 'upgrade');
  const onlyUninstalls = entries.every((e) => e.job.action?.type === 'uninstall');
  const okLabel = (count) => t(onlyUninstalls ? 'summary.uninstalledCount' : 'summary.okCount', { count });
  let title;
  if (allGood && onlyUninstalls) title = t('summary.allGoodUninstall', { count: ok.length });
  else if (allGood) title = onlyUpgrades ? t('summary.allGood', { count: ok.length }) : t('summary.allGoodOther', { count: ok.length });
  else if (!ok.length && failed.length && !skipped.length) title = t('summary.allFailed', { count: failed.length });
  else if (!ok.length && !failed.length) title = t('summary.nothingDone');
  else {
    title = [
      ok.length ? okLabel(ok.length) : null,
      failed.length ? t('summary.failCount', { count: failed.length }) : null,
      skipped.length ? t('summary.skippedCount', { count: skipped.length }) : null,
    ].filter(Boolean).join(', ');
    title = title.charAt(0).toUpperCase() + title.slice(1);
  }

  const stats = h('div', { class: 'sum-stats' },
    h('span', { class: 'pg-chip ok', tabindex: 0, tip: [t('progress.okTitle'), t('progress.okTip')] }, icon('success'), okLabel(ok.length)),
    failed.length ? h('span', { class: 'pg-chip fail has', tabindex: 0, tip: [t('progress.failTitle'), t('summary.failTip')] }, icon('error'), t('summary.failCount', { count: failed.length })) : null,
    skipped.length ? h('span', { class: 'pg-chip', tabindex: 0, tip: [t('summary.skippedTitle'), t('summary.skippedTip')] }, icon('minus'), t('summary.skippedCount', { count: skipped.length })) : null,
    h('span', { class: 'pg-chip', tabindex: 0, tip: [t('summary.durationTitle'), t('summary.durationTip')] }, icon('clock'), fmt.duration(duration)));

  const lead = allGood && onlyUninstalls ? t('summary.allGoodUninstallText', { count: ok.length })
    : allGood ? (onlyUpgrades ? t('summary.allGoodText') : t('summary.allGoodOtherText'))
      : failed.length ? t('summary.failText') : t('summary.neutralText');
  const hero = h('section', { class: ['sum-hero', allGood && !onlyUninstalls ? 'is-celebrating' : null] },
    emblem,
    h('div', { class: 'sum-head' },
      h('h1', null, title),
      h('p', { class: 'lead' }, lead),
      stats));

  const body = h('div', { class: 'sum-body' });

  if (rebootNames.length) {
    body.append(banner({
      tone: 'gold', icon: 'restart',
      title: t('summary.rebootTitle'),
      text: t('summary.rebootText', { list: fmt.list(rebootNames), count: rebootNames.length }),
    }));
  }

  if (run.done.elevationRefused) {
    const adminJobs = entries.filter((e) => stateType(e.state) === 'failed' && ['elevation_refused', 'elevation_required'].includes(e.state.kind)).map((e) => e.job);
    const retryJobs = adminJobs.length ? adminJobs : entries.filter((e) => stateType(e.state) !== 'succeeded' && needsElevation(e.job)).map((e) => e.job);
    body.append(banner({
      tone: 'warning', icon: 'shield',
      title: t('summary.uacTitle'),
      text: t('summary.uacText'),
      children: h('div', { class: 'row-flex wrap banner-actions' },
        button({ label: t('summary.retryAdmin'), icon: 'shield', size: 'small', tip: [t('summary.retryAdmin'), t('summary.retryAdminTip')], onClick: () => startJobs(retryJobs) }),
        button({ label: t('summary.retryNoAdmin'), icon: 'user', size: 'small', variant: 'primary', tip: [t('summary.retryNoAdmin'), t('summary.retryNoAdminTip')], onClick: () => startJobs(retryJobs, { allowElevation: false }) })),
    }));
  }

  if (failed.length) {
    const retryAll = failed.length > 1
      ? button({ label: t('summary.retryAll', { count: failed.length }), icon: 'refresh', size: 'small', tip: [t('summary.retryAll', { count: failed.length }), t('summary.retryAllTip')], onClick: () => startJobs(failed.map((e) => e.job)) })
      : null;
    body.append(h('section', { class: 'sum-section' },
      h('div', { class: 'sum-section-head' }, h('h2', null, t('summary.failuresTitle', { count: failed.length })), h('span', { class: 'grow' }), retryAll),
      h('ul', { class: 'fail-list' }, failed.map((e) => failureCard(e, run)))));
  }

  if (ok.length) {
    body.append(h('details', { class: 'sum-section sum-fold', open: !failed.length && ok.length <= 6 },
      h('summary', { tip: [t('summary.okListTitle'), t('summary.okListTip')] }, icon('chevron-right'), h('h2', null, t(onlyUninstalls ? 'summary.uninstalledList' : 'summary.okList', { count: ok.length }))),
      h('ul', { class: 'ok-list' }, ok.map((e) => {
        const item = itemByKey(e.job.key) || { name: e.job.name, key: e.job.key };
        const secs = e.startedAt && e.finishedAt ? (e.finishedAt - e.startedAt) / 1000 : null;
        return h('li', null,
          h('span', { class: 'pg-ic ok static', html: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle class="ic-circle" cx="12" cy="12" r="10"/><path class="ic-check" d="M7.4 12.4l3.1 3.1 6.1-6.3"/></svg>' }),
          avatar(item),
          h('span', { class: 'strong ellipsis grow' }, e.job.name),
          h('span', { class: 'caption subtle ver' }, e.job.action?.type === 'uninstall'
            ? t('uninstall.done')
            : e.job.fromVersion || e.job.toVersion ? `${e.job.fromVersion || t('version.unknown')} → ${e.job.toVersion || '?'}` : ''),
          e.state.rebootRequired ? badge({ text: t('summary.rebootBadge'), tone: 'gold', icon: 'restart', tip: [t('summary.rebootTitle'), t('summary.rebootBadgeTip')] }) : null,
          secs ? h('span', { class: 'caption subtle num', tip: [t('summary.itemDurationTitle'), t('summary.itemDurationTip')] }, fmt.duration(secs)) : null);
      }))));
  }

  if (skipped.length) {
    body.append(h('section', { class: 'sum-section' },
      h('div', { class: 'sum-section-head' },
        h('h2', null, t('summary.skippedList', { count: skipped.length })),
        h('span', { class: 'grow' }),
        skipped.length > 1 ? button({ label: t('summary.retrySkipped', { count: skipped.length }), icon: 'play', size: 'small', tip: [t('summary.retrySkipped', { count: skipped.length }), t('summary.retrySkippedTip')], onClick: () => startJobs(skipped.map((e) => e.job)) }) : null),
      h('ul', { class: 'ok-list' }, skipped.map((e) => {
        const item = itemByKey(e.job.key) || { name: e.job.name, key: e.job.key };
        const type = stateType(e.state);
        return h('li', null,
          h('span', { class: 'pg-ic muted-ic' }, icon('minus')),
          avatar(item),
          h('span', { class: 'strong ellipsis grow' }, e.job.name),
          h('span', { class: 'caption subtle ellipsis' }, type === 'skipped' ? (e.state.reason || t('progress.skippedDetail')) : t('progress.cancelledDetail')),
          button({ label: t('summary.retry'), icon: 'refresh', size: 'small', variant: 'subtle', tip: [t('summary.retry'), t('summary.retryTip')], onClick: () => startJobs([e.job]) }));
      }))));
  }

  body.append(h('div', { class: 'sum-foot' },
    button({ label: t('summary.history'), icon: 'history', tip: [t('summary.history'), t('summary.historyTip')], onClick: () => go('history') }),
    button({ label: t('summary.back'), icon: 'refresh', variant: 'royal', size: 'large', tip: [t('summary.back'), t('summary.backTip')], onClick: backToResults })));

  const scroll = h('div', { class: 'view-scroll' }, hero, body);
  view.append(scroll);
  if (allGood && !onlyUninstalls) requestAnimationFrame(() => sparkles(hero));
  setTimeout(() => view.querySelector('.sum-foot .btn.royal')?.focus({ preventScroll: true }), 400);
  return () => {};
}

function failureCard(entry, run) {
  const job = entry.job;
  const item = itemByKey(job.key) || { name: job.name, key: job.key };
  const f = failureInfo(entry.state.kind);
  const code = entry.state.code;
  const excerpt = entry.state.logExcerpt;
  const actions = [];
  for (const a of f.actions) {
    switch (a) {
      case 'retry':
        actions.push(button({ label: t('summary.retry'), icon: 'refresh', size: 'small', tip: [t('summary.retry'), t('summary.retryTip')], onClick: () => startJobs([job]) }));
        break;
      case 'closeRetry':
        actions.push(button({ label: t('summary.closeRetry'), icon: 'activity', size: 'small', variant: 'primary', tip: [t('summary.closeRetry'), t('summary.closeRetryTip')], onClick: () => startJobs([job], { forceClose: true }) }));
        break;
      case 'interactive':
        actions.push(button({ label: t('summary.interactive'), icon: 'window', size: 'small', tip: [t('summary.interactive'), t('summary.interactiveTip')], onClick: () => startJobs([{ ...job, action: { type: 'interactive' } }]) }));
        break;
      case 'noAdmin':
        if (run.done.elevationRefused) actions.push(button({ label: t('summary.retryNoAdmin'), icon: 'user', size: 'small', tip: [t('summary.retryNoAdmin'), t('summary.retryNoAdminTip')], onClick: () => startJobs([job], { allowElevation: false }) }));
        break;
      case 'download': {
        const url = item.links?.download || item.links?.homepage || webSearchUrl(job.name);
        actions.push(button({ label: t('summary.download'), icon: 'external', size: 'small', tip: [t('summary.download'), url], onClick: () => openUrl(url) }));
        break;
      }
      case 'ignoreVersion':
        if (job.toVersion) {
          const label = t('summary.ignoreVersion', { version: job.toVersion });
          actions.push(button({ label, icon: 'eye-off', size: 'small', variant: 'primary', tip: [label, t('summary.ignoreVersionTip')], onClick: () => ignoreItem(item, job.toVersion) }));
        }
        break;
      case 'logs':
        actions.push(button({ label: t('common.openLogs'), icon: 'folder', size: 'small', variant: 'subtle', tip: [t('common.openLogs'), t('common.openLogsTip')], onClick: openLogs }));
        break;
      default:
    }
  }
  if (!f.actions.includes('logs')) {
    actions.push(button({ label: t('common.openLogs'), icon: 'folder', size: 'small', variant: 'subtle', tip: [t('common.openLogs'), t('common.openLogsTip')], onClick: openLogs }));
  }
  return h('li', { class: 'fail-card' },
    avatar(item),
    h('div', { class: 'grow' },
      h('div', { class: 'fail-title' },
        h('span', { class: 'strong' }, job.name),
        h('span', { class: 'badge tone-danger', tabindex: 0, 'data-tip': '', 'data-tip-title': f.title, 'data-tip-text': code !== null && code !== undefined ? t('summary.code', { code }) : f.text }, icon('error'), f.title)),
      h('p', { class: 'muted' }, f.text),
      excerpt ? h('details', { class: 'log-excerpt' },
        h('summary', { tip: [t('summary.logTitle'), t('summary.logTip')] }, t('summary.logTitle')),
        h('pre', null, String(excerpt).slice(0, 2000))) : null,
      h('div', { class: 'row-flex wrap fail-actions' }, actions)));
}
