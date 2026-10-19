// Results: summary strip, tabs (updates / other software / up to date), grouped
// list with selection, details drawer and the sticky action bar.

import { h, clear, debounce } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import { invoke } from '../api.js';
import { state, bus, runActive } from '../state.js';
import {
  PRIORITIES, CATEGORY_ORDER, kindOf, priorityOf, categoryOf, isSelectable, buildJob,
  normSource, sourceInfo, needsElevation, categoryInfo,
} from '../model.js';
import { startScan, startJobs, selectedJobs, openStoreUpdates } from '../actions.js';
import { button, iconButton, infoDot, banner, emptyState, spinner } from '../components/widgets.js';
import { itemRow, updateRowLabel } from '../components/row.js';
import { openDetails, refreshDetails, closeDetails } from './details.js';
import { welcomeCard } from './welcome.js';

const WARNING_CODES = ['winget_missing', 'winget_failed', 'choco_failed', 'scoop_failed', 'offline', 'kev_failed', 'registry_failed'];

export function mountResults(container) {
  const view = h('div', { class: 'view results-view' });
  const scroll = h('div', { class: 'view-scroll results-scroll' });
  const headHost = h('div', { class: 'results-head' });
  const toolbarHost = h('div', { class: 'results-toolbar' });
  const listHost = h('div', { class: 'results-list' });
  const barHost = h('div', { class: 'actionbar' });
  scroll.append(headHost, toolbarHost, listHost);
  view.append(scroll, barHost);
  container.append(view);

  const offs = [];
  let estimateToken = 0;
  let estimate = { state: 'idle', secs: null, basis: null };
  let showIgnored = false;

  // ---------------------------------------------------------------- data

  const items = () => state.items;
  const updates = () => items().filter((i) => kindOf(i) === 'update' && !i.ignored);
  const manual = () => items().filter((i) => kindOf(i) === 'manual' && !i.ignored);
  const ignored = () => items().filter((i) => i.ignored && ['update', 'manual'].includes(kindOf(i)));
  const others = () => items().filter((i) => kindOf(i) === 'unmanaged');
  const uptodate = () => items().filter((i) => kindOf(i) === 'up_to_date');

  function tabItems(tab) {
    if (tab === 'others') return others();
    if (tab === 'uptodate') return uptodate();
    return [...updates(), ...manual(), ...ignored()];
  }

  function matches(item) {
    const q = state.ui.search.trim().toLowerCase();
    if (q) {
      const hay = `${item.name || ''} ${item.publisher || ''} ${item.id || ''}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    if (state.ui.sources.size) {
      const src = item.kind === 'unmanaged' || item.kind === 'manual' ? 'unmanaged' : normSource(item.source);
      if (!state.ui.sources.has(src)) return false;
    }
    return true;
  }

  // ---------------------------------------------------------------- header

  function renderHead() {
    clear(headHost);
    const result = state.result || {};
    const scannedAt = result.scannedAt;
    const lookup = state.lookup;
    const lookupActive = lookup && lookup.state === 'started';

    headHost.append(h('div', { class: 'page-head' },
      h('div', { class: 'grow' },
        h('h1', null, t('results.title')),
        h('p', { class: 'muted row-flex wrap' },
          h('span', { tip: [t('results.scannedTitle'), scannedAt ? fmt.dateTime(scannedAt) : '—'], tabindex: 0 },
            t('results.scanned', { when: fmt.relative(scannedAt) })),
          lookupActive
            ? h('span', { class: 'lookup-pill', tip: [t('results.lookupTitle'), t('results.lookupTip')], tabindex: 0 },
              spinner(true),
              t('results.lookup', { percent: typeof lookup.fraction === 'number' ? fmt.percent(lookup.fraction) : '…' }))
            : null)),
      button({
        label: t('results.rescan'), icon: 'refresh',
        tip: [t('results.rescan'), t('results.rescanTip')],
        disabled: runActive(),
        onClick: () => startScan(),
      })));

    const warnings = Array.isArray(result.warnings) ? result.warnings.filter(Boolean) : [];
    if (warnings.length && !state.ui.warningsDismissed) {
      const list = h('ul', null, warnings.map((w) => {
        const code = WARNING_CODES.includes(w.code) ? w.code : 'unknown';
        return h('li', null, t(`warnings.${code}`, { code: w.code || '?' }),
          w.detail ? h('span', { class: 'caption subtle warning-detail', tip: [t('warnings.detailTitle'), w.detail], tabindex: 0 }, ` (${t('warnings.detail')})`) : null);
      }));
      headHost.append(h('div', { class: 'page-body tight' }, banner({
        tone: 'warning',
        title: t('warnings.title', { count: warnings.length }),
        children: list,
        actions: iconButton({ icon: 'close', label: t('warnings.dismiss'), tip: [t('warnings.dismiss'), t('warnings.dismissTip')], onClick: () => { state.ui.warningsDismissed = true; renderHead(); } }),
      })));
    }

    const welcome = welcomeCard();
    if (welcome) headHost.append(h('div', { class: 'page-body tight' }, welcome));
    headHost.append(h('div', { class: 'page-body tight' }, summaryStrip()));
  }

  function summaryStrip() {
    const ups = updates();
    const count = (p) => ups.filter((i) => priorityOf(i) === p).length;
    const totalSize = ups.reduce((s, i) => s + (Number(i.downloadSize) || 0), 0);
    const unknownSizes = ups.filter((i) => !i.downloadSize).length;
    const seg = (key, value, tone, group, ic) => {
      const el = h(group ? 'button' : 'div', {
        type: group ? 'button' : undefined,
        class: ['strip-seg', `tone-${tone}`, value === 0 && group ? 'is-zero' : null],
        tip: [t(`summary.${key}.title`), t(`summary.${key}.tip`)],
        tabindex: group ? undefined : 0,
        onClick: group ? () => jumpToGroup(group) : undefined,
      },
      h('span', { class: 'strip-ic' }, icon(ic)),
      h('span', { class: 'strip-text' },
        h('span', { class: 'strip-value num' }, typeof value === 'number' ? fmt.number(value) : value),
        h('span', { class: 'strip-label' }, t(`summary.${key}.label`, { count: typeof value === 'number' ? value : 2 }))));
      return el;
    };
    return h('div', { class: 'strip', role: 'group', 'aria-label': t('summary.label') },
      seg('updates', ups.length, 'accent', 'all', 'updates'),
      seg('critical', count('critical'), 'critical', 'critical', 'shield'),
      seg('major', count('major'), 'major', 'major', 'rocket'),
      seg('uncertain', count('uncertain'), 'uncertain', 'uncertain', 'help'),
      seg('size', totalSize ? fmt.bytes(totalSize) : '—', 'gold', null, 'download'),
      unknownSizes && ups.length ? h('span', { class: 'sr-only' }, t('summary.size.unknown', { count: unknownSizes })) : null);
  }

  function jumpToGroup(group) {
    if (state.ui.tab !== 'updates') { state.ui.tab = 'updates'; renderToolbar(); renderList(); }
    const target = group === 'all' ? listHost : listHost.querySelector(`[data-group="${group}"]`);
    if (!target) return;
    const top = target.offsetTop - toolbarHost.offsetHeight - 8;
    scroll.scrollTo({ top: Math.max(0, top), behavior: 'smooth' });
    const firstRow = target.querySelector('.item-row');
    if (firstRow) setTimeout(() => focusRow(firstRow), 250);
  }

  // ---------------------------------------------------------------- toolbar

  function renderToolbar() {
    clear(toolbarHost);
    const tabs = [
      ['updates', updates().length + manual().length],
      ['others', others().length],
      ['uptodate', uptodate().length],
    ];
    const tablist = h('div', { class: 'tabs', role: 'tablist', 'aria-label': t('results.tabsLabel') },
      tabs.map(([key, count]) => h('button', {
        type: 'button', role: 'tab', class: 'tab', id: `tab-${key}`,
        'aria-selected': String(state.ui.tab === key),
        'aria-controls': 'results-panel',
        tabindex: state.ui.tab === key ? 0 : -1,
        tip: [t(`tabs.${key}.label`), t(`tabs.${key}.tip`)],
        onClick: () => switchTab(key),
      }, t(`tabs.${key}.label`), h('span', { class: 'count' }, fmt.number(count)))));
    tablist.addEventListener('keydown', (e) => {
      const keys = tabs.map(([k]) => k);
      const idx = keys.indexOf(state.ui.tab);
      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        e.preventDefault();
        const next = keys[(idx + (e.key === 'ArrowRight' ? 1 : keys.length - 1)) % keys.length];
        switchTab(next);
        toolbarHost.querySelector(`#tab-${next}`)?.focus();
      }
    });

    const input = h('input', {
      class: 'input', type: 'search', placeholder: t('results.searchPlaceholder'),
      'aria-label': t('results.searchLabel'), value: state.ui.search, spellcheck: false,
      tip: [t('results.searchLabel'), t('results.searchTip')],
    });
    input.addEventListener('input', debounce(() => { state.ui.search = input.value; renderList(); }, 150));
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && input.value) { e.stopPropagation(); input.value = ''; state.ui.search = ''; renderList(); }
      if (e.key === 'ArrowDown') { const r = listHost.querySelector('.item-row'); if (r) { e.preventDefault(); focusRow(r); } }
    });
    const search = h('div', { class: 'search' }, icon('search'), input);

    // Source filters: only sources present in the current tab.
    const present = new Set(tabItems(state.ui.tab).map((i) => (i.kind === 'unmanaged' || i.kind === 'manual' ? 'unmanaged' : normSource(i.source))));
    const filterKeys = ['winget', 'ms_store', 'chocolatey', 'scoop', 'unmanaged'].filter((k) => present.has(k));
    const chips = filterKeys.length > 1 ? h('div', { class: 'chips', role: 'group', 'aria-label': t('results.filtersLabel') },
      h('span', { class: 'chips-label caption subtle' }, t('results.filters'), infoDot(t('sources.title'), t('sources.tip'))),
      filterKeys.map((k) => {
        const info = k === 'unmanaged' ? { icon: 'registry', label: t('sources.unmanaged.label'), tip: [t('sources.unmanaged.label'), t('sources.unmanaged.tip')] } : sourceInfo(k);
        return h('button', {
          type: 'button', class: 'chip', 'aria-pressed': String(state.ui.sources.has(k)),
          tip: [t('results.filterBy', { source: info.label }), info.tip[1]],
          onClick: () => {
            if (state.ui.sources.has(k)) state.ui.sources.delete(k); else state.ui.sources.add(k);
            renderToolbar(); renderList();
          },
        }, icon(info.icon), info.label);
      })) : null;

    const row1 = h('div', { class: 'toolbar-row' }, tablist, h('div', { class: 'grow' }), search);
    toolbarHost.append(row1);

    const row2 = h('div', { class: 'toolbar-row second' });
    if (state.ui.tab === 'updates') {
      const selectable = updates();
      row2.append(h('div', { class: 'selection-tools', role: 'group', 'aria-label': t('selection.label') },
        button({ label: t('selection.recommended'), icon: 'sparkle', variant: 'subtle', size: 'small',
          tip: [t('selection.recommended'), t('selection.recommendedTip')], disabled: !selectable.length,
          onClick: () => setSelection(selectable.filter((i) => i.assessment?.preselect)) }),
        button({ label: t('selection.all'), icon: 'select-all', variant: 'subtle', size: 'small',
          tip: [t('selection.all'), t('selection.allTip')], disabled: !selectable.length,
          onClick: () => setSelection(selectable) }),
        button({ label: t('selection.none'), icon: 'select-none', variant: 'subtle', size: 'small',
          tip: [t('selection.none'), t('selection.noneTip')], disabled: !state.selection.size,
          onClick: () => setSelection([]) })));
    } else {
      row2.append(h('p', { class: 'caption subtle tab-hint' }, t(`tabs.${state.ui.tab}.hint`)));
    }
    row2.append(h('div', { class: 'grow' }));
    if (chips) row2.append(chips);
    toolbarHost.append(row2);
  }

  function switchTab(key) {
    if (state.ui.tab === key) return;
    state.ui.tab = key;
    state.ui.sources.clear();
    renderToolbar();
    renderList();
    scroll.scrollTo({ top: Math.min(scroll.scrollTop, headHost.offsetHeight) });
  }

  function setSelection(list) {
    state.selection = new Set(list.filter(isSelectable).map((i) => i.key));
    syncSelectionUI();
  }

  // ---------------------------------------------------------------- list

  function group({ id, tone, title, text, count, info, rows, selectAll, extra, collapsed, onToggleCollapse }) {
    let headCheck = null;
    if (selectAll) {
      headCheck = h('input', { type: 'checkbox', class: 'check', 'aria-label': t('groups.selectGroup', { group: title }), tip: [t('groups.selectGroupTitle'), t('groups.selectGroupTip', { group: title })] });
      headCheck.addEventListener('change', () => {
        for (const it of selectAll) {
          if (headCheck.checked) state.selection.add(it.key); else state.selection.delete(it.key);
        }
        syncSelectionUI();
      });
    }
    const section = h('section', { class: ['group', `tone-${tone}`, collapsed ? 'is-collapsed' : null], dataset: { group: id }, 'aria-label': title });
    const head = h('header', { class: 'group-head' },
      headCheck ? h('div', { class: 'group-check' }, headCheck) : h('div', { class: 'group-check' }),
      h('div', { class: 'grow' },
        h('h2', { class: 'group-title' },
          h('span', { class: 'group-dot' }),
          h('span', { tip: [title, text], tabindex: -1 }, title),
          h('span', { class: 'group-count num' }, fmt.number(count)),
          info ? infoDot(info[0], info[1]) : null),
        h('p', { class: 'group-text' }, text)),
      extra || null,
      onToggleCollapse ? button({
        label: collapsed ? t('groups.show') : t('groups.hide'), variant: 'subtle', size: 'small',
        icon: collapsed ? 'chevron-down' : 'minus',
        tip: [collapsed ? t('groups.show') : t('groups.hide'), t('groups.collapseTip')],
        onClick: onToggleCollapse,
      }) : null);
    section.append(head);
    if (!collapsed) section.append(h('ul', { class: 'rows', role: 'list' }, rows));
    if (headCheck) updateGroupCheck(headCheck, selectAll);
    section._check = headCheck;
    section._items = selectAll;
    return section;
  }

  function updateGroupCheck(check, list) {
    const n = list.filter((i) => state.selection.has(i.key)).length;
    check.checked = n > 0 && n === list.length;
    check.indeterminate = n > 0 && n < list.length;
  }

  function rowFor(item, mode) {
    return itemRow(item, {
      mode,
      selected: state.selection.has(item.key),
      onToggle: (it, checked) => {
        if (checked) state.selection.add(it.key); else state.selection.delete(it.key);
        syncSelectionUI();
      },
      onOpen: (it) => openDetails(it.key, { returnFocus: () => listHost.querySelector(`.item-row[data-key="${cssEscape(it.key)}"]`) }),
    });
  }

  function renderList() {
    const focusKey = document.activeElement?.closest?.('.item-row')?.dataset.key;
    const scrollTop = scroll.scrollTop;
    clear(listHost);
    const tab = state.ui.tab;
    const panel = h('div', { id: 'results-panel', role: 'tabpanel', 'aria-labelledby': `tab-${tab}` });
    listHost.append(panel);

    if (tab === 'updates') renderUpdatesTab(panel);
    else if (tab === 'others') renderOthersTab(panel);
    else renderUptodateTab(panel);

    // roving tabindex: first row (or the previously focused one) is the tab stop
    const rows = [...listHost.querySelectorAll('.item-row')];
    const target = rows.find((r) => r.dataset.key === focusKey) || rows[0];
    if (target) target.tabIndex = 0;
    if (focusKey && target && target.dataset.key === focusKey) target.focus({ preventScroll: true });
    scroll.scrollTop = scrollTop;
    renderBar();
  }

  function renderUpdatesTab(panel) {
    const ups = updates().filter(matches);
    const man = manual().filter(matches);
    const ign = ignored().filter(matches);
    if (!updates().length && !manual().length) {
      panel.append(emptyState({
        icon: 'crown', title: t('empty.updatesTitle'), text: t('empty.updatesText'),
        action: button({ label: t('results.rescan'), icon: 'refresh', tip: [t('results.rescan'), t('results.rescanTip')], onClick: () => startScan() }),
      }));
    } else if (!ups.length && !man.length && !ign.length) {
      panel.append(noMatch());
      return;
    } else if (!updates().length) {
      panel.append(h('div', { class: 'inline-ok' }, banner({ tone: 'info', icon: 'success', title: t('empty.updatesTitle'), text: t('empty.onlyManualText') })));
    }
    for (const p of PRIORITIES) {
      const list = ups.filter((i) => priorityOf(i) === p);
      if (!list.length) continue;
      panel.append(group({
        id: p, tone: p, title: t(`groups.${p}.title`), text: t(`groups.${p}.text`), count: list.length,
        info: [t(`groups.${p}.title`), t(`groups.${p}.info`)],
        rows: list.map((i) => rowFor(i, 'update')),
        selectAll: list,
      }));
    }
    if (man.length) {
      panel.append(group({
        id: 'manual', tone: 'manual', title: t('groups.manual.title'), text: t('groups.manual.text'), count: man.length,
        info: [t('groups.manual.title'), t('groups.manual.info')],
        rows: man.map((i) => rowFor(i, 'manual')),
      }));
    }
    if (ign.length) {
      panel.append(group({
        id: 'ignored', tone: 'uncertain', title: t('groups.ignored.title'), text: t('groups.ignored.text'), count: ign.length,
        rows: ign.map((i) => rowFor(i, 'ignored')),
        collapsed: !showIgnored,
        onToggleCollapse: () => { showIgnored = !showIgnored; renderList(); },
      }));
    }
  }

  function renderOthersTab(panel) {
    const all = others();
    const list = all.filter(matches);
    if (!all.length) {
      panel.append(emptyState({ icon: 'app', title: t('empty.othersTitle'), text: t('empty.othersText') }));
      return;
    }
    if (!list.length) { panel.append(noMatch()); return; }
    for (const cat of CATEGORY_ORDER) {
      const inCat = list.filter((i) => categoryOf(i) === cat);
      if (!inCat.length) continue;
      const bys = [...new Set(inCat.map((i) => categoryInfo(i).by).filter(Boolean))];
      const extra = cat === 'store_app'
        ? button({ label: t('alternatives.storeUpdate'), icon: 'store', size: 'small', tip: [t('alternatives.storeUpdate'), t('alternatives.storeUpdateTip')], onClick: openStoreUpdates })
        : null;
      panel.append(group({
        id: `cat-${cat}`, tone: 'neutral',
        title: t(`categories.${cat}.group`),
        text: bys.length ? t(`categories.${cat}.explainBy`, { by: fmt.list(bys.slice(0, 3)) }) : t(`categories.${cat}.explain`),
        count: inCat.length,
        info: [t(`categories.${cat}.group`), t(`categories.${cat}.info`)],
        rows: inCat.map((i) => rowFor(i, 'other')),
        extra,
      }));
    }
  }

  function renderUptodateTab(panel) {
    const all = uptodate();
    const list = all.filter(matches);
    if (!all.length) {
      panel.append(emptyState({ icon: 'check', title: t('empty.uptodateTitle'), text: t('empty.uptodateText') }));
      return;
    }
    if (!list.length) { panel.append(noMatch()); return; }
    panel.append(group({
      id: 'uptodate', tone: 'success', title: t('groups.uptodate.title'), text: t('groups.uptodate.text'), count: list.length,
      info: [t('groups.uptodate.title'), t('groups.uptodate.info')],
      rows: list.map((i) => rowFor(i, 'uptodate')),
    }));
  }

  function noMatch() {
    return emptyState({
      icon: 'search', title: t('empty.noMatchTitle'), text: t('empty.noMatchText'),
      action: button({
        label: t('empty.clearFilters'), icon: 'close', tip: [t('empty.clearFilters'), t('empty.clearFiltersTip')],
        onClick: () => { state.ui.search = ''; state.ui.sources.clear(); renderToolbar(); renderList(); },
      }),
    });
  }

  // ---------------------------------------------------------------- selection sync (no re-render)

  function syncSelectionUI() {
    for (const row of listHost.querySelectorAll('.item-row.mode-update')) {
      const key = row.dataset.key;
      const sel = state.selection.has(key);
      row.classList.toggle('is-selected', sel);
      const cb = row.querySelector('.item-lead .check');
      if (cb) cb.checked = sel;
      const item = state.items.find((i) => i.key === key);
      if (item) updateRowLabel(row, item, 'update', sel);
    }
    for (const section of listHost.querySelectorAll('.group')) {
      if (section._check && section._items) updateGroupCheck(section._check, section._items);
    }
    const noneBtn = toolbarHost.querySelector('.selection-tools .btn:last-child');
    if (noneBtn) noneBtn.disabled = !state.selection.size;
    renderBar();
  }

  // ---------------------------------------------------------------- action bar

  const requestEstimate = debounce(async () => {
    const jobs = selectedJobs();
    const token = ++estimateToken;
    if (!jobs.length) { estimate = { state: 'idle', secs: null }; renderBar(); return; }
    try {
      const res = await invoke('estimate_jobs', { jobs });
      if (token !== estimateToken) return;
      const list = Array.isArray(res) ? res : [];
      const secs = list.reduce((s, e) => s + (Number(e?.downloadSecs) || 0) + (Number(e?.installSecs) || 0), 0);
      const fromHistory = list.filter((e) => e?.basis === 'history').length;
      estimate = { state: 'ready', secs, fromHistory, count: list.length };
    } catch {
      if (token !== estimateToken) return;
      estimate = { state: 'error', secs: null };
    }
    renderBar();
  }, 350);

  let lastSelectionSig = '';
  function renderBar() {
    const sel = updates().filter((i) => state.selection.has(i.key));
    const sig = sel.map((i) => i.key).sort().join('|');
    if (sig !== lastSelectionSig) {
      lastSelectionSig = sig;
      estimate = { state: sel.length ? 'loading' : 'idle', secs: null };
      requestEstimate();
    }
    clear(barHost);
    const size = sel.reduce((s, i) => s + (Number(i.downloadSize) || 0), 0);
    const admin = sel.some((i) => needsElevation(buildJob(i)));
    const all = updates();
    const busy = runActive();

    let durationText;
    if (estimate.state === 'loading') durationText = h('span', { class: 'row-flex' }, spinner(true), t('bar.estimating'));
    else if (estimate.state === 'ready') durationText = t('bar.duration', { duration: fmt.roughDuration(estimate.secs) });
    else if (estimate.state === 'error') durationText = t('bar.durationUnknown');
    else durationText = '—';

    barHost.append(
      h('div', { class: 'bar-stats' },
        h('div', { class: 'bar-stat', tabindex: 0, tip: [t('bar.selectedTitle'), t('bar.selectedTip')] },
          h('span', { class: 'bar-value num' }, fmt.number(sel.length)),
          h('span', { class: 'bar-label' }, t('bar.selected', { count: sel.length, total: all.length }))),
        h('div', { class: 'bar-stat', tabindex: 0, tip: [t('bar.sizeTitle'), t('bar.sizeTip')] },
          icon('download'), h('span', { class: 'num' }, size ? fmt.bytes(size) : '—')),
        h('div', { class: 'bar-stat', tabindex: 0, tip: [t('bar.durationTitle'), estimate.state === 'ready' && estimate.fromHistory ? t('bar.durationTipHistory', { count: estimate.fromHistory }) : t('bar.durationTip')] },
          icon('hourglass'), h('span', { class: 'num' }, durationText)),
        admin ? h('div', { class: 'bar-stat admin', tabindex: 0, tip: [t('bar.adminTitle'), t('bar.adminTip')] },
          icon('shield'), h('span', null, t('bar.admin'))) : null),
      h('div', { class: 'grow' }),
      button({
        label: t('bar.updateAll', { count: all.length }), icon: 'sparkle',
        tip: [t('bar.updateAllTitle'), busy ? t('run.alreadyRunning') : t('bar.updateAllTip', { count: all.length })],
        disabled: !all.length || busy,
        onClick: () => { setSelection(all); startJobs(selectedJobs()); },
      }),
      button({
        label: t('bar.update', { count: sel.length }), icon: 'download', variant: 'royal', size: 'large',
        tip: [t('bar.updateTitle'), busy ? t('run.alreadyRunning') : sel.length ? t('bar.updateTip', { count: sel.length }) : t('bar.updateNone')],
        disabled: !sel.length || busy,
        onClick: () => startJobs(selectedJobs()),
      }));
  }

  // ---------------------------------------------------------------- keyboard on rows

  function focusRow(row) {
    for (const r of listHost.querySelectorAll('.item-row[tabindex="0"]')) r.tabIndex = -1;
    row.tabIndex = 0;
    row.focus();
    row.scrollIntoView({ block: 'nearest' });
  }

  listHost.addEventListener('keydown', (e) => {
    const row = e.target.closest('.item-row');
    if (!row) return;
    const rows = [...listHost.querySelectorAll('.item-row')];
    const idx = rows.indexOf(row);
    const inner = e.target !== row;
    const innerFocusables = [...row.querySelectorAll('[data-rowfocus]')];
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault(); if (rows[idx + 1]) focusRow(rows[idx + 1]); break;
      case 'ArrowUp':
        e.preventDefault(); if (rows[idx - 1]) focusRow(rows[idx - 1]); else toolbarHost.querySelector('.search input')?.focus(); break;
      case 'Home':
        if (!inner) { e.preventDefault(); focusRow(rows[0]); } break;
      case 'End':
        if (!inner) { e.preventDefault(); focusRow(rows[rows.length - 1]); } break;
      case 'ArrowRight': {
        e.preventDefault();
        const i = inner ? innerFocusables.indexOf(e.target) : -1;
        innerFocusables[i + 1]?.focus();
        break;
      }
      case 'ArrowLeft': {
        e.preventDefault();
        const i = innerFocusables.indexOf(e.target);
        if (i > 0) innerFocusables[i - 1].focus(); else row.focus();
        break;
      }
      case 'Escape':
        if (inner) { e.preventDefault(); row.focus(); }
        break;
      case ' ': {
        if (inner && e.target.matches('button, input')) return;
        e.preventDefault();
        const cb = row.querySelector('.item-lead .check');
        if (cb) { cb.checked = !cb.checked; const it = state.items.find((i) => i.key === row.dataset.key); if (it) { if (cb.checked) state.selection.add(it.key); else state.selection.delete(it.key); syncSelectionUI(); } }
        break;
      }
      case 'Enter':
        if (inner && e.target.matches('button, input')) return;
        e.preventDefault();
        openDetails(row.dataset.key, { returnFocus: () => listHost.querySelector(`.item-row[data-key="${cssEscape(row.dataset.key)}"]`) });
        break;
      default:
    }
  });
  listHost.addEventListener('focusin', (e) => {
    const row = e.target.closest('.item-row');
    if (row && e.target === row) {
      for (const r of listHost.querySelectorAll('.item-row[tabindex="0"]')) if (r !== row) r.tabIndex = -1;
      row.tabIndex = 0;
    }
  });

  // ---------------------------------------------------------------- live updates

  const rerenderSoon = debounce(() => {
    const top = scroll.scrollTop;
    renderHead(); renderToolbar(); renderList();
    scroll.scrollTop = top;
  }, 120);
  offs.push(bus.on('item-updated', ({ item, prev }) => {
    if (state.ui.detailsKey === item.key) refreshDetails();
    const old = listHost.querySelector(`.item-row[data-key="${cssEscape(item.key)}"]`);
    const sameGroup = prev && old && kindOf(prev) === kindOf(item) && priorityOf(prev) === priorityOf(item)
      && !!prev.ignored === !!item.ignored && categoryOf(prev) === categoryOf(item);
    if (sameGroup) {
      const mode = [...old.classList].find((c) => c.startsWith('mode-'))?.slice(5) || 'update';
      const hadFocus = old.contains(document.activeElement);
      const fresh = rowFor(item, mode);
      fresh.tabIndex = old.tabIndex;
      old.replaceWith(fresh);
      if (hadFocus) fresh.focus({ preventScroll: true });
      renderBar();
      return;
    }
    rerenderSoon();
  }));
  offs.push(bus.on('lookup', debounce(() => { const top = scroll.scrollTop; renderHead(); scroll.scrollTop = top; }, 200)));
  offs.push(bus.on('settings', () => { /* language handled by app */ }));

  renderHead();
  renderToolbar();
  renderList();
  if (state.ui.detailsKey) openDetails(state.ui.detailsKey);

  return () => {
    offs.forEach((off) => off());
    requestEstimate.cancel();
    rerenderSoon.cancel();
    closeDetails(true);
  };
}

function cssEscape(s) {
  return window.CSS?.escape ? CSS.escape(s) : String(s).replace(/["\\]/g, '\\$&');
}
