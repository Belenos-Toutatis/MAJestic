// "Choose another version" dialog (list_versions) and the winget search dialog (search_winget).

import { h, clear } from '../dom.js';
import { t } from '../i18n.js';
import { invoke, errorText } from '../api.js';
import { openDialog } from './dialog.js';
import { badge, spinner, infoDot, button } from './widgets.js';

/**
 * chooseVersion({ id, source, name, installed, latest, preselect, rollback })
 * Resolves with the chosen version string, or null.
 */
export function chooseVersion({ id, source, name, installed, latest, preselect, rollback }) {
  let chosen = null;
  let confirmBtn = null;
  const listHost = h('div', { class: 'version-list', role: 'radiogroup', 'aria-label': t('versions.listLabel') },
    h('div', { class: 'loading-line' }, spinner(true), t('versions.loading')));

  const updateConfirm = () => {
    if (!confirmBtn) return;
    confirmBtn.disabled = !chosen;
    const label = confirmBtn.querySelector('.btn-label');
    if (label) label.textContent = chosen ? t('versions.installThis', { version: chosen }) : t('versions.pick');
  };

  invoke('list_versions', { id, source }).then((versions) => {
    clear(listHost);
    const list = (Array.isArray(versions) ? versions : []).filter((v) => typeof v === 'string' && v);
    if (!list.length) {
      listHost.append(h('p', { class: 'muted' }, t('versions.none')));
      return;
    }
    chosen = list.includes(preselect) ? preselect : (list.find((v) => v !== installed) || list[0]);
    const radios = list.map((v, i) => {
      const input = h('input', { type: 'radio', name: 'version', value: v, class: 'radio', id: `ver-${i}` });
      input.checked = v === chosen;
      input.addEventListener('change', () => { chosen = v; updateConfirm(); });
      const tags = [];
      if (i === 0) tags.push(badge({ text: t('versions.latest'), tone: 'accent', tip: [t('versions.latest'), t('versions.latestTip')] }));
      if (v === installed) tags.push(badge({ text: t('versions.installed'), tone: 'success', tip: [t('versions.installed'), t('versions.installedTip')] }));
      if (rollback && v === preselect) tags.push(badge({ text: t('versions.previous'), tone: 'gold', tip: [t('versions.previous'), t('versions.previousTip')] }));
      return h('label', { class: 'version-option', for: `ver-${i}` }, input, h('span', { class: 'ver strong' }, v), ...tags);
    });
    listHost.append(...radios);
    updateConfirm();
    const sel = listHost.querySelector('input:checked');
    sel?.scrollIntoView({ block: 'nearest' });
    sel?.focus();
  }).catch((err) => {
    clear(listHost);
    listHost.append(h('p', { class: 'muted' }, t('versions.error', { error: errorText(err) })));
  });

  return openDialog({
    title: t('versions.title', { name }),
    icon: 'tag',
    width: 520,
    body: [
      h('p', null, rollback ? t('versions.introRollback') : t('versions.intro'),
        infoDot(t('versions.howTitle'), t('versions.howTip'))),
      listHost,
    ],
    actions: [
      { label: t('common.cancel'), value: null },
      {
        label: t('versions.pick'), variant: 'primary', icon: 'download', disabled: true,
        ref: (b) => { confirmBtn = b; },
        onClick: (api) => { if (chosen) api.close(chosen); },
        tip: [t('versions.confirmTitle'), t('versions.confirmTip')],
      },
    ],
  });
}

/**
 * searchWinget(item) → resolves with { id, name, version } chosen, or null.
 */
export function searchWinget(item) {
  let query = item.name || '';
  const resultsHost = h('div', { class: 'winget-results' });
  const input = h('input', { class: 'input grow', value: query, 'aria-label': t('winget.queryLabel'), spellcheck: false });
  let closeApi = null;

  const run = async () => {
    query = input.value.trim();
    clear(resultsHost);
    if (!query) return;
    resultsHost.append(h('div', { class: 'loading-line' }, spinner(true), t('winget.searching')));
    try {
      const res = await invoke('search_winget', { query });
      clear(resultsHost);
      const list = (Array.isArray(res) ? res : []).filter((r) => r && r.id);
      if (!list.length) {
        resultsHost.append(h('p', { class: 'muted' }, t('winget.none')));
        return;
      }
      resultsHost.append(...list.map((r) => h('div', { class: 'winget-result' },
        h('div', { class: 'grow' },
          h('div', { class: 'strong ellipsis' }, r.name || r.id),
          h('div', { class: 'caption subtle ellipsis' }, r.id, r.version ? ` — ${r.version}` : '')),
        badge({ text: r.source || 'winget', outline: true, tip: [t('winget.sourceTitle'), t('winget.sourceTip')] }),
        button({
          label: t('winget.useThis'), size: 'small', icon: 'package',
          tip: [t('winget.useThis'), t('winget.useThisTip', { name: r.name || r.id })],
          onClick: () => closeApi?.close({ id: r.id, name: r.name || r.id, version: r.version || null }),
        }))));
    } catch (err) {
      clear(resultsHost);
      resultsHost.append(h('p', { class: 'muted' }, t('winget.error', { error: errorText(err) })));
    }
  };
  input.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); run(); } });

  const p = openDialog({
    title: t('winget.title', { name: item.name }),
    icon: 'package',
    width: 600,
    body: (api) => {
      closeApi = api;
      return [
        h('p', null, t('winget.intro'), infoDot(t('winget.howTitle'), t('winget.howTip'))),
        h('div', { class: 'row-flex winget-query' }, input,
          button({ label: t('winget.search'), icon: 'search', onClick: run, tip: [t('winget.search'), t('winget.searchTip')] })),
        resultsHost,
      ];
    },
    actions: [{ label: t('common.close'), value: null }],
  });
  setTimeout(run, 0);
  setTimeout(() => input.focus(), 30);
  return p;
}
