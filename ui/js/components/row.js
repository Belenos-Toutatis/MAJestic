// One program row of the results list.

import { h } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import {
  sourceInfo, describeReason, reasonsOf, runningOf, installedVersionInfo, availableVersionOf,
  needsElevation, buildJob, categoryInfo, alternativesOf, priorityOf, originName,
} from '../model.js';
import { avatar } from './avatar.js';
import { badge, button } from './widgets.js';
import { openSelfUpdate } from '../actions.js';

export function sourceBadge(item) {
  const s = sourceInfo(item.source);
  return h('span', { class: 'source-badge', tip: s.tip, tabindex: -1, 'data-rowfocus': '' }, icon(s.icon), s.label);
}

/** "129.0 → 130.0" with certainty markers. */
export function versionBlock(item, mode) {
  const inst = installedVersionInfo(item);
  const installed = h('span', {
    class: ['ver', 'ver-installed', `cert-${inst.certainty}`],
    tip: inst.tip,
    tabindex: -1,
    'data-rowfocus': '',
  }, inst.text);
  const note = inst.note ? h('span', { class: 'ver-note caption' }, icon('file'), inst.note) : null;

  if (mode === 'uptodate') {
    return h('div', { class: 'item-version' },
      h('div', { class: 'ver-line' }, installed,
        h('span', { class: 'ver-ok', tip: [t('version.upToDateTitle'), t('version.upToDateTip')] }, icon('check'))), note);
  }
  if (mode === 'other') {
    const latest = availableVersionOf(item);
    return h('div', { class: 'item-version' },
      h('div', { class: 'ver-line' }, installed,
        latest ? [h('span', { class: 'ver-arrow' }, icon('arrow-right')),
          h('span', { class: 'ver ver-available manual', tip: [t('version.elsewhereTitle'), t('version.elsewhereTip')] }, latest)] : null), note);
  }
  const available = availableVersionOf(item);
  return h('div', { class: 'item-version' },
    h('div', { class: 'ver-line' },
      installed,
      h('span', { class: 'ver-arrow', 'aria-label': t('version.to') }, icon('arrow-right')),
      h('span', {
        class: 'ver ver-available',
        tip: [t('version.availableTitle'), t('version.availableTip')],
        tabindex: -1, 'data-rowfocus': '',
      }, available || t('version.unknownShort'))),
    note);
}

function reasonBadges(item, mode) {
  const out = [];
  for (const reason of reasonsOf(item)) {
    if (reason.type === 'app_running') continue; // dedicated badge
    if (mode !== 'update' && mode !== 'ignored' && ['unknown_version', 'approximate_version', 'version_from_file'].includes(reason.type)) continue;
    const d = describeReason(reason);
    out.push(badge({ text: d.label, tone: d.tone, icon: d.icon, tip: [d.title, d.text], focusable: true }));
    if (d.ransomware) {
      out.push(badge({ text: t('reasons.known_exploited.ransomwareBadge'), tone: 'critical', icon: 'lock',
        tip: [t('reasons.known_exploited.ransomwareBadge'), t('reasons.known_exploited.ransomware')], focusable: true }));
    }
  }
  return out;
}

function runningBadge(item) {
  const procs = runningOf(item);
  if (!procs.length) return null;
  const names = [...new Set(procs.map((p) => p.name).filter(Boolean))];
  return badge({
    text: t('row.running'), tone: 'warning', icon: 'activity', focusable: true,
    tip: [t('row.runningTitle'), t('row.runningTip', { list: fmt.list(names) })],
  });
}

function elevationMark(item, mode) {
  if (mode !== 'update' || item.kind !== 'update') return null;
  if (!needsElevation(buildJob(item))) return null;
  return h('span', {
    class: 'flag-ic shield', tabindex: -1, 'data-rowfocus': '',
    tip: [t('row.adminTitle'), t('row.adminTip')],
  }, icon('shield'));
}

/**
 * itemRow(item, { mode, selected, onToggle, onOpen })
 * mode: update | manual | ignored | other | uptodate
 */
export function itemRow(item, { mode, selected = false, onToggle, onOpen }) {
  const name = item.name || item.id || item.key;
  const row = h('li', {
    class: ['item-row', `mode-${mode}`, mode === 'update' ? `prio-${priorityOf(item)}` : null, selected ? 'is-selected' : null],
    tabindex: -1,
    dataset: { key: item.key },
  });

  let lead;
  if (mode === 'update') {
    lead = h('input', {
      type: 'checkbox', class: 'check', tabindex: -1,
      'aria-label': t('row.select', { name }),
      tip: [t('row.selectTitle'), t('row.selectTip')],
    });
    lead.checked = selected;
    lead.addEventListener('click', (e) => { e.stopPropagation(); onToggle?.(item, lead.checked); });
  } else if (mode === 'manual') {
    lead = h('span', { class: 'lead-ic manual', tip: [t('groups.manual.title'), t('row.manualTip')] }, icon('wrench'));
  } else if (mode === 'ignored') {
    lead = h('span', { class: 'lead-ic', tip: [t('groups.ignored.title'), t('row.ignoredTip')] }, icon('eye-off'));
  } else {
    lead = h('span', { class: 'lead-ic placeholder' });
  }

  const meta = [];
  if (item.publisher) meta.push(h('span', { class: 'publisher ellipsis' }, item.publisher));
  if (mode === 'other') {
    const cat = categoryInfo(item);
    meta.push(badge({ text: cat.short, icon: cat.icon, outline: true, tip: [cat.label, cat.explain], focusable: true }));
    const cand = alternativesOf(item).find((a) => a.type === 'winget_candidate');
    if (cand) meta.push(badge({ text: t('row.wingetCandidate'), tone: 'accent', icon: 'package', focusable: true, tip: [t('row.wingetCandidate'), t('row.wingetCandidateTip', { id: cand.id || '?' })] }));
    const elsewhere = alternativesOf(item).find((a) => a.type === 'latest_elsewhere');
    if (elsewhere) meta.push(badge({ text: t('row.foundOn', { origin: originName(elsewhere.origin) }), tone: 'manual', icon: 'globe', focusable: true, tip: [t('row.foundOnTitle'), t('row.foundOnTip', { origin: originName(elsewhere.origin), version: elsewhere.version || '?' })] }));
  } else if (mode === 'manual') {
    const elsewhere = alternativesOf(item).find((a) => a.type === 'latest_elsewhere');
    const self = alternativesOf(item).find((a) => a.type === 'self_update');
    if (self) {
      // winget cannot update it (installed differently): its own updater can.
      const app = self.app || name;
      meta.push(badge({ text: t('row.selfUpdate'), tone: 'manual', icon: 'autoupdate', focusable: true, tip: [t('reasons.technology_mismatch.title'), t('row.selfUpdateTip', { app })] }));
      meta.push(button({ label: t('alternatives.selfUpdate', { app }), icon: 'external', size: 'small', cls: 'row-action', tip: [t('alternatives.selfUpdate', { app }), t('alternatives.selfUpdateTip', { app })], onClick: (e) => { e?.stopPropagation?.(); openSelfUpdate(item); } }));
    } else {
      meta.push(badge({ text: elsewhere ? t('row.foundOn', { origin: originName(elsewhere.origin) }) : t('row.manualBadge'), tone: 'manual', icon: 'globe', focusable: true, tip: [t('row.foundOnTitle'), t('row.manualTip')] }));
    }
  } else {
    meta.push(sourceBadge(item));
  }
  if (mode === 'update' || mode === 'ignored') meta.push(...reasonBadges(item, mode));
  if (mode === 'uptodate' && item.scope === 'user') meta.push(badge({ text: t('row.userScope'), icon: 'user', outline: true, focusable: true, tip: [t('details.scope.user'), t('details.scope.userTip')] }));

  const size = (mode === 'update' || mode === 'ignored')
    ? h('div', { class: 'item-size num', tip: item.downloadSize ? [t('row.sizeTitle'), t('row.sizeTip')] : [t('row.sizeUnknownTitle'), t('row.sizeUnknownTip')] },
      item.downloadSize ? fmt.bytes(item.downloadSize) : '—')
    : h('div', { class: 'item-size' });

  const chevron = h('button', {
    type: 'button', class: 'icon-btn row-open', tabindex: -1,
    'aria-label': t('row.openDetails', { name }),
    tip: [t('row.detailsTitle'), t('row.detailsTip')],
    onClick: (e) => { e.stopPropagation(); onOpen?.(item); },
  }, icon('chevron-right'));

  row.append(
    h('div', { class: 'item-lead' }, lead),
    avatar(item),
    h('div', { class: 'item-main' },
      h('div', { class: 'item-title' },
        h('span', { class: 'item-name ellipsis' }, name),
        runningBadge(item),
        elevationMark(item, mode)),
      h('div', { class: 'item-meta' }, meta)),
    versionBlock(item, mode),
    size,
    chevron,
  );
  updateRowLabel(row, item, mode, selected);

  row.addEventListener('click', (e) => {
    if (e.target.closest('a, button, input')) return;
    if (mode === 'update' && !e.target.closest('.item-main .badge, .source-badge')) {
      // a click on the row toggles selection; details open with the chevron / Enter / double-click
      lead.checked = !lead.checked;
      onToggle?.(item, lead.checked);
    } else if (mode !== 'update') {
      onOpen?.(item);
    }
  });
  row.addEventListener('dblclick', (e) => { if (!e.target.closest('input, button')) onOpen?.(item); });
  return row;
}

export function updateRowLabel(row, item, mode, selected) {
  const inst = installedVersionInfo(item);
  const avail = availableVersionOf(item);
  const parts = [item.name || item.key];
  if (mode === 'update') parts.push(selected ? t('row.ariaSelected') : t('row.ariaNotSelected'));
  parts.push(avail && mode !== 'uptodate' ? t('row.ariaVersions', { from: inst.text, to: avail }) : inst.text);
  row.setAttribute('aria-label', parts.join(', '));
}
