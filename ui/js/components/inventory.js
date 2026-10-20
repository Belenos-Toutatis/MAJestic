// The list of installed programs: export it to a file, import one on another
// PC and install what is missing (export_inventory / import_inventory).

import { h } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import { invoke, errorText } from '../api.js';
import { startJobs, openUrl, webSearchUrl } from '../actions.js';
import { sourceInfo } from '../model.js';
import { openDialog } from './dialog.js';
import { button } from './widgets.js';
import { toast } from './toast.js';

function dialogText(kind) {
  const date = new Date().toISOString().slice(0, 10);
  return {
    title: t(`inventory.${kind}Title`),
    filterName: t('inventory.filter'),
    fileName: kind === 'export' ? t('inventory.fileName', { date }) : '',
  };
}

function failed(e) {
  const text = errorText(e);
  if (text === 'no_scan') toast(t('inventory.noScan'), { tone: 'danger', duration: 6000 });
  else if (text === 'invalid_file') toast(t('inventory.invalid'), { tone: 'danger', duration: 7000 });
  else toast(t('inventory.failed', { error: text }), { tone: 'danger', duration: 7000 });
}

/** "Save as", then writes the list. Returns the export summary, or null. */
export async function exportInventory() {
  try {
    const done = await invoke('export_inventory', { text: dialogText('export') });
    if (done) {
      toast(t('inventory.exported', { count: done.packages, manual: done.manual }), { tone: 'success', duration: 6000 });
    }
    return done || null;
  } catch (e) {
    failed(e);
    return null;
  }
}

export async function revealExport() {
  try { await invoke('reveal_export'); } catch (e) { failed(e); }
}

/** "Open", then shows what the list would install on this PC. */
export async function importInventory() {
  let imported;
  try {
    imported = await invoke('import_inventory', { text: dialogText('import') });
  } catch (e) {
    failed(e);
    return;
  }
  if (imported) await importDialog(imported);
}

const packageKey = (p) => `${{ msstore: 'msstore', chocolatey: 'choco', scoop: 'scoop' }[p.source] || 'winget'}:${p.id}`;

function jobFor(p) {
  return {
    key: packageKey(p), source: p.source, id: p.id, name: p.name,
    action: { type: 'install' }, fromVersion: null, toVersion: null, scope: p.scope || null,
    includeUnknown: false, includePinned: false, downloadSize: null, installerType: null,
  };
}

async function importDialog({ file, plan }) {
  const toInstall = plan.toInstall || [];
  const chosen = new Set(toInstall.map(packageKey));
  let installBtn = null;
  const refresh = () => {
    if (!installBtn) return;
    installBtn.disabled = !chosen.size;
    const label = installBtn.querySelector('.btn-label');
    if (label) label.textContent = t('inventory.install', { count: chosen.size });
  };

  const rows = toInstall.map((p) => {
    const key = packageKey(p);
    const box = h('input', { type: 'checkbox', class: 'check', checked: true, 'aria-label': p.name });
    box.addEventListener('change', () => { if (box.checked) chosen.add(key); else chosen.delete(key); refresh(); });
    const src = sourceInfo(p.source);
    return h('label', { class: 'inv-row' },
      box,
      h('span', { class: 'grow ellipsis strong' }, p.name),
      p.version ? h('span', { class: 'caption subtle num' }, p.version) : null,
      h('span', { class: 'badge outline', tip: src.tip }, icon(src.icon), src.label));
  });

  const manual = (plan.manual || []).map((m) => h('li', { class: 'inv-manual' },
    h('span', { class: 'grow ellipsis' }, m.name, m.version ? h('span', { class: 'caption subtle' }, ` ${m.version}`) : null),
    button({
      label: m.url ? t('inventory.site') : t('inventory.search'), icon: m.url ? 'external' : 'search', size: 'small', variant: 'subtle',
      tip: [m.name, m.url || t('inventory.searchTip')],
      onClick: () => openUrl(m.url || webSearchUrl(m.name)),
    })));

  const created = plan.createdAt ? fmt.date(plan.createdAt) : '?';
  const body = h('div', { class: 'inv-dialog' },
    h('p', { class: 'muted' }, t('inventory.from', { date: created }), ' ', h('span', { class: 'caption subtle inv-file' }, file)),
    toInstall.length
      ? h('section', null,
        h('h3', { class: 'd-title' }, t('inventory.toInstall', { count: toInstall.length })),
        h('div', { class: 'inv-list' }, rows))
      : h('p', { class: 'strong' }, t('inventory.nothingToInstall')),
    h('p', { class: 'caption muted' }, t('inventory.already', { count: (plan.installed || []).length + (plan.manualInstalled || 0) })),
    manual.length
      ? h('section', null,
        h('h3', { class: 'd-title' }, t('inventory.manual', { count: manual.length })),
        h('p', { class: 'caption muted' }, t('inventory.manualText')),
        h('ul', { class: 'inv-manual-list' }, manual))
      : null);

  const choice = await openDialog({
    title: t('inventory.importTitle'),
    icon: 'download',
    width: 640,
    body,
    actions: [
      { label: t('common.close'), value: null },
      ...(toInstall.length ? [{
        label: t('inventory.install', { count: chosen.size }), icon: 'download', variant: 'primary', autofocus: true,
        tip: [t('inventory.installTitle'), t('inventory.installTip')],
        value: 'install', ref: (b) => { installBtn = b; },
      }] : []),
    ],
  });
  if (choice === 'install' && chosen.size) {
    await startJobs(toInstall.filter((p) => chosen.has(packageKey(p))).map(jobFor));
  }
}
