// "These programs are open" dialog shown before an update batch.

import { h } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import { openDialog } from './dialog.js';
import { avatar } from './avatar.js';
import { badge, infoDot } from './widgets.js';
import { itemByKey } from '../state.js';

/** Resolves { action: 'close' | 'skip', dontAsk } or null (cancelled). */
export async function closeAppsDialog(running) {
  let dontAsk = false;
  const withWindows = running.some((r) => (r.processes || []).some((p) => p.hasWindow));

  const list = h('ul', { class: 'close-list' }, running.map((r) => {
    const item = itemByKey(r.key) || { name: r.name, key: r.key };
    const procs = (r.processes || []).filter(Boolean);
    const names = [...new Set(procs.map((p) => p.name).filter(Boolean))];
    const hasWindow = procs.some((p) => p.hasWindow);
    return h('li', { class: 'close-item' },
      avatar(item),
      h('div', { class: 'grow' },
        h('div', { class: 'strong ellipsis' }, r.name || item.name || r.key),
        h('div', { class: 'caption subtle ellipsis', tip: [t('closeApps.processesTitle'), procs.map((p) => p.exe || p.name).join('\n')] },
          t('closeApps.processes', { count: procs.length, list: fmt.list(names) }))),
      hasWindow
        ? badge({ text: t('closeApps.openWindow'), tone: 'warning', icon: 'window', tip: [t('closeApps.openWindow'), t('closeApps.openWindowTip')] })
        : badge({ text: t('closeApps.background'), tone: 'uncertain', icon: 'activity', tip: [t('closeApps.background'), t('closeApps.backgroundTip')] }));
  }));

  const dontAskBox = h('input', { type: 'checkbox', class: 'check', id: 'close-dont-ask' });
  dontAskBox.addEventListener('change', () => { dontAsk = dontAskBox.checked; });
  const dontAskLine = h('label', {
    class: 'checkline close-dont-ask',
    for: 'close-dont-ask',
    tip: [t('closeApps.dontAsk'), t('closeApps.dontAskTip')],
  }, dontAskBox, t('closeApps.dontAsk'));

  const value = await openDialog({
    title: t('closeApps.title', { count: running.length }),
    icon: 'activity',
    width: 580,
    body: [
      h('p', null, t('closeApps.intro', { count: running.length }), infoDot(t('closeApps.howTitle'), t('closeApps.howTip'))),
      withWindows ? h('div', { class: 'close-warning' }, icon('warning'), h('span', null, t('closeApps.unsavedWarning'))) : null,
      list,
      dontAskLine,
    ],
    actions: [
      { label: t('common.cancel'), value: null, tip: [t('common.cancel'), t('closeApps.cancelTip')] },
      { label: t('closeApps.skip', { count: running.length }), value: 'skip', tip: [t('closeApps.skip', { count: running.length }), t('closeApps.skipTip')] },
      { label: t('closeApps.close'), value: 'close', variant: 'primary', icon: 'updates', autofocus: true, tip: [t('closeApps.close'), t('closeApps.closeTip')] },
    ],
  });
  if (!value) return null;
  return { action: value, dontAsk: value === 'close' && dontAsk };
}
