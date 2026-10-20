// Details drawer for one program: reasons in plain words, release notes,
// links, technical info and every alternative action.

import { h, clear, animateOut } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt, getLanguage } from '../i18n.js';
import { invoke } from '../api.js';
import { state, itemByKey, runActive } from '../state.js';
import {
  kindOf, priorityOf, describeReason, reasonsOf, alternativesOf, runningOf, installedVersionInfo,
  availableVersionOf, sourceInfo, buildJob, needsElevation, categoryInfo, normSource, isSelectable, originName, categoryOf,
} from '../model.js';
import {
  startJobs, ignoreItem, unignore, openUrl, openStoreUpdates, webSearchUrl, uninstallItem, runUninstaller, openInstalledApps,
} from '../actions.js';
import { avatar } from '../components/avatar.js';
import { button, iconButton, badge, infoDot, spinner } from '../components/widgets.js';
import { chooseVersion, searchWinget } from '../components/versions.js';
import { hide as hideTooltip } from '../components/tooltip.js';

let scrimEl = null;
let drawerEl = null;
let returnFocusFn = null;
let keyHandler = null;

export function openDetails(key, { returnFocus } = {}) {
  const item = itemByKey(key);
  if (!item) return;
  const host = document.getElementById('main');
  if (!host) return;
  state.ui.detailsKey = key;
  if (returnFocus) returnFocusFn = returnFocus;
  hideTooltip(true);

  if (!drawerEl) {
    scrimEl = h('div', { class: 'drawer-scrim', onClick: () => closeDetails() });
    drawerEl = h('aside', { class: 'drawer', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'details-title' });
    keyHandler = (e) => {
      if (e.key === 'Escape' && !document.querySelector('dialog[open]')) { e.preventDefault(); e.stopPropagation(); closeDetails(); }
      if (e.key === 'Tab') trapFocus(e);
    };
    drawerEl.addEventListener('keydown', keyHandler);
    host.append(scrimEl, drawerEl);
    // keep the page behind inert for assistive tech
    for (const el of host.querySelectorAll(':scope > .view')) el.setAttribute('inert', '');
    document.querySelector('.rail')?.setAttribute('inert', '');
  }
  render(item);
  drawerEl.querySelector('.drawer-close')?.focus();
}

export function refreshDetails() {
  if (!drawerEl || !state.ui.detailsKey) return;
  const item = itemByKey(state.ui.detailsKey);
  if (!item) { closeDetails(); return; }
  const scrollTop = drawerEl.querySelector('.drawer-body')?.scrollTop || 0;
  render(item);
  const body = drawerEl.querySelector('.drawer-body');
  if (body) body.scrollTop = scrollTop;
}

export function closeDetails(immediate = false) {
  state.ui.detailsKey = null;
  if (!drawerEl) return;
  const d = drawerEl; const s = scrimEl;
  drawerEl = null; scrimEl = null;
  const host = document.getElementById('main');
  host?.querySelectorAll(':scope > .view').forEach((el) => el.removeAttribute('inert'));
  document.querySelector('.rail')?.removeAttribute('inert');
  hideTooltip(true);
  const target = returnFocusFn?.();
  returnFocusFn = null;
  if (immediate) { d.remove(); s?.remove(); return; }
  animateOut(d, () => d.remove(), 260);
  animateOut(s, () => s?.remove(), 260);
  if (target) { target.tabIndex = 0; target.focus({ preventScroll: false }); }
}

function trapFocus(e) {
  const focusables = [...drawerEl.querySelectorAll('button:not([disabled]), a[href], input, select, [tabindex="0"]')]
    .filter((el) => el.offsetParent !== null);
  if (!focusables.length) return;
  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
}

// ------------------------------------------------------------------ content

function section(title, info, ...children) {
  return h('section', { class: 'd-section' },
    h('h3', { class: 'd-title' }, title, info ? infoDot(info[0], info[1]) : null),
    ...children);
}

function render(item) {
  clear(drawerEl);
  const kind = kindOf(item);
  const body = h('div', { class: 'drawer-body' });

  // Header
  drawerEl.append(h('header', { class: 'drawer-head' },
    avatar(item, { large: true }),
    h('div', { class: 'grow' },
      h('h2', { id: 'details-title', class: 'ellipsis' }, item.name || item.key),
      h('p', { class: 'muted ellipsis' }, item.publisher || t('details.noPublisher'))),
    iconButton({ icon: 'close', label: t('common.close'), tip: [t('common.close'), t('details.closeTip')], cls: 'drawer-close', onClick: () => closeDetails() })));

  body.append(versionCard(item, kind));
  body.append(aboutSection(item));
  body.append(statusSection(item, kind));

  const reasons = reasonsOf(item);
  if (reasons.length) {
    body.append(section(t('details.why'), [t('details.why'), t('details.whyTip')],
      h('ul', { class: 'reason-list' }, reasons.map((r) => {
        const d = describeReason(r);
        return h('li', { class: ['reason', `tone-${d.tone}`] },
          h('span', { class: 'reason-ic' }, icon(d.icon)),
          h('div', null, h('div', { class: 'strong' }, d.title), h('p', { class: 'muted' }, d.text),
            d.ransomware ? h('p', { class: 'reason-alert' }, icon('lock'), t('reasons.known_exploited.ransomware')) : null));
      }))));
  }

  const running = runningOf(item);
  if (running.length) {
    body.append(section(t('details.running'), [t('details.running'), t('details.runningTip')],
      h('ul', { class: 'proc-list' }, running.map((p) => h('li', null,
        icon(p.hasWindow ? 'window' : 'activity'),
        h('span', { class: 'strong' }, p.name || '?'),
        h('span', { class: 'caption subtle ellipsis grow', tip: [t('details.processPath'), p.exe || '—'] }, p.exe || ''),
        p.hasWindow ? badge({ text: t('closeApps.openWindow'), tone: 'warning', tip: [t('closeApps.openWindow'), t('closeApps.openWindowTip')] }) : null)))));
  }

  if (item.releaseNotes) {
    body.append(section(t('details.notes'), [t('details.notes'), t('details.notesTip')],
      h('div', { class: 'notes', tabindex: 0, 'aria-label': t('details.notes') }, String(item.releaseNotes).slice(0, 4000)),
      item.links?.releaseNotes ? button({ label: t('links.releaseNotes'), icon: 'external', variant: 'subtle', size: 'small', tip: [t('links.releaseNotes'), item.links.releaseNotes], onClick: () => openUrl(item.links.releaseNotes) }) : null));
  }

  const actions = actionsFor(item, kind);
  if (actions.length) body.append(section(t('details.actions'), null, h('div', { class: 'action-list' }, actions)));

  const uninstall = uninstallSection(item, kind);
  if (uninstall) body.append(uninstall);

  const links = linksFor(item);
  if (links.length) body.append(section(t('details.links'), [t('details.links'), t('details.linksTip')], h('div', { class: 'link-list' }, links)));

  body.append(section(t('details.tech'), [t('details.tech'), t('details.techTip')], techInfo(item)));
  drawerEl.append(body);
}

// ------------------------------------------------------------------ what it does

const aboutCache = new Map(); // item key → { loading: true } | { about }

function aboutSection(item) {
  const entry = aboutCache.get(item.key);
  if (!entry) loadAbout(item);
  const about = entry?.about;
  let content;
  if (!entry || entry.loading) {
    content = h('p', { class: 'muted about-loading' }, spinner(true), t('details.about.loading'));
  } else if (!about) {
    content = h('p', { class: 'muted' }, t('details.about.none'));
  } else {
    const paragraphs = [about.summary, ...String(about.description || '').split(/\n{2,}/)].map((p) => (p || '').trim()).filter(Boolean);
    const foreign = !String(about.locale || '').toLowerCase().startsWith(getLanguage());
    content = h('div', { class: 'about-text' },
      paragraphs.map((p, i) => h('p', { class: i === 0 ? 'about-lead' : 'muted' }, p)),
      about.origin === 'majestic' ? null
        : h('p', { class: 'caption subtle' }, t(`details.about.from.${about.origin}`), foreign ? ` ${t('details.about.foreign')}` : ''));
  }
  return h('section', { class: 'd-section d-about', 'data-about': item.key },
    h('h3', { class: 'd-title' }, t('details.about.title'), infoDot(t('details.about.title'), t('details.about.tip'))),
    content);
}

function loadAbout(item) {
  aboutCache.set(item.key, { loading: true });
  const request = {
    source: item.source, id: item.id, name: item.name, publisher: item.publisher || null,
    versions: [item.availableVersion, item.installedVersion].filter(Boolean),
    lang: getLanguage(),
  };
  invoke('describe_item', { request })
    .then((about) => aboutCache.set(item.key, { about: about || null }))
    .catch((e) => { console.warn('[about]', e); aboutCache.set(item.key, { about: null }); })
    .finally(() => {
      // Replace only this section: re-rendering the drawer would lose the focus.
      const old = drawerEl?.querySelector(`[data-about="${CSS.escape(item.key)}"]`);
      const current = itemByKey(item.key);
      if (old && current) old.replaceWith(aboutSection(current));
    });
}

function versionCard(item, kind) {
  const inst = installedVersionInfo(item);
  const available = availableVersionOf(item);
  const showAvailable = kind !== 'up_to_date' && available;
  return h('div', { class: ['version-card', `kind-${kind}`] },
    h('div', { class: 'vc-col', tip: inst.tip, tabindex: 0 },
      h('span', { class: 'caption subtle' }, t('details.installed')),
      h('span', { class: ['vc-ver', 'ver', `cert-${inst.certainty}`] }, inst.text),
      inst.note ? h('span', { class: 'caption ver-note' }, icon('file'), inst.note) : null),
    showAvailable ? h('span', { class: 'vc-arrow' }, icon('arrow-right')) : null,
    showAvailable ? h('div', { class: 'vc-col', tabindex: 0, tip: [t('version.availableTitle'), kind === 'manual' || kind === 'unmanaged' ? t('version.elsewhereTip') : t('version.availableTip')] },
      h('span', { class: 'caption subtle' }, t('details.available')),
      h('span', { class: 'vc-ver ver strong' }, available)) : null,
    kind === 'up_to_date' ? h('div', { class: 'vc-col' }, badge({ text: t('details.upToDate'), tone: 'success', icon: 'check', tip: [t('version.upToDateTitle'), t('version.upToDateTip')] })) : null,
    inst.certainty !== 'exact' ? h('p', { class: 'vc-explain caption' }, inst.tip[1]) : null);
}

function statusSection(item, kind) {
  if (kind === 'update') {
    const p = priorityOf(item);
    return h('div', { class: ['status-line', `tone-${p}`] },
      badge({ text: t(`groups.${p}.title`), tone: p, tip: [t(`groups.${p}.title`), t(`groups.${p}.info`)] }),
      h('p', null, t(`groups.${p}.text`)),
      item.ignored ? badge({ text: t('groups.ignored.title'), tone: 'uncertain', icon: 'eye-off', tip: [t('groups.ignored.title'), t('groups.ignored.text')] }) : null);
  }
  if (kind === 'manual') {
    return h('div', { class: 'status-line tone-manual' },
      badge({ text: t('groups.manual.title'), tone: 'manual', icon: 'wrench', tip: [t('groups.manual.title'), t('groups.manual.info')] }),
      h('p', null, t('groups.manual.text')));
  }
  if (kind === 'up_to_date') {
    return h('div', { class: 'status-line tone-success' }, h('p', null, t('groups.uptodate.text')));
  }
  const cat = categoryInfo(item);
  return h('div', { class: 'status-line tone-neutral' },
    badge({ text: cat.short, icon: cat.icon, outline: true, tip: [cat.label, cat.explain] }),
    h('p', null, cat.by ? t(`categories.${cat.type}.explainBy`, { by: cat.by }) : cat.explain));
}

function actionCard({ ic, title, text, control, tone, stacked }) {
  return h('div', { class: ['action-card', tone ? `tone-${tone}` : null, stacked ? 'stacked' : null] },
    h('span', { class: 'action-ic' }, icon(ic)),
    h('div', { class: 'grow' },
      h('div', { class: 'strong' }, title),
      text ? h('p', { class: 'caption muted' }, text) : null,
      stacked && control ? h('div', { class: 'action-stack' }, control) : null),
    stacked ? null : control || null);
}

// ------------------------------------------------------------------ uninstall

const MANAGER_NAMES = { winget: 'winget', chocolatey: 'Chocolatey', scoop: 'Scoop' };

/** How this program is best removed: its package manager, its own
 * uninstaller (registry), or Windows Settings (Store apps). */
function uninstallMethod(item) {
  const src = normSource(item.source);
  if (MANAGER_NAMES[src]) return src;
  if (src === 'ms_store' || src === 'msix' || categoryOf(item) === 'store_app') return 'settings';
  return item.uninstallKey ? 'uninstaller' : 'settings';
}

function uninstallSection(item, kind) {
  if (!['update', 'manual', 'unmanaged', 'up_to_date'].includes(kind)) return null;
  const busy = runActive();
  const busyTip = busy ? t('run.alreadyRunning') : null;
  const method = uninstallMethod(item);
  const name = item.name || item.id;
  const cards = [];

  if (MANAGER_NAMES[method]) {
    const manager = MANAGER_NAMES[method];
    cards.push(actionCard({
      ic: 'trash', title: t('uninstall.withManager', { manager }), text: t('uninstall.withManagerText', { manager }),
      control: button({
        label: t('uninstall.button'), icon: 'trash', size: 'small', disabled: busy,
        tip: [t('uninstall.withManager', { manager }), busyTip || t('uninstall.withManagerTip', { manager })],
        onClick: async () => { if (await uninstallItem(item, manager)) closeDetails(true); },
      }),
    }));
  } else if (method === 'uninstaller') {
    cards.push(actionCard({
      ic: 'trash', title: t('uninstall.ownUninstaller'), text: t('uninstall.ownUninstallerText'),
      control: button({ label: t('uninstall.buttonOwn'), icon: 'trash', size: 'small', tip: [t('uninstall.ownUninstaller'), t('uninstall.ownUninstallerTip')], onClick: () => runUninstaller(item) }),
    }));
  } else {
    cards.push(actionCard({
      ic: 'settings', title: t('uninstall.fromSettings'), text: t('uninstall.fromSettingsText', { name }),
      control: button({ label: t('uninstall.openSettings'), icon: 'external', size: 'small', tip: [t('uninstall.openSettings'), t('uninstall.openSettingsTip')], onClick: () => openInstalledApps(false) }),
    }));
  }

  const cat = categoryOf(item);
  const by = categoryInfo(item).by || '…';
  const notes = {
    system_component: ['warning', 'uninstall.cautionComponent'],
    driver: ['warning', 'uninstall.cautionDriver'],
    game: ['info', 'uninstall.noteGame'],
    web_app: ['info', 'uninstall.noteWebApp'],
  };
  if (notes[cat]) {
    const [ic, key] = notes[cat];
    cards.push(actionCard({ ic, tone: ic === 'warning' ? 'warning' : null, title: t(`${key}Title`, { by }), text: t(key, { by }) }));
  }

  cards.push(actionCard({
    ic: 'list', title: t('uninstall.otherWays'), text: t('uninstall.otherWaysText'), stacked: true,
    control: h('div', { class: 'row-flex wrap' },
      method !== 'uninstaller' && item.uninstallKey
        ? button({ label: t('uninstall.buttonOwn'), size: 'small', variant: 'subtle', tip: [t('uninstall.ownUninstaller'), t('uninstall.ownUninstallerTip')], onClick: () => runUninstaller(item) })
        : null,
      method !== 'settings'
        ? button({ label: t('uninstall.openSettings'), icon: 'external', size: 'small', variant: 'subtle', tip: [t('uninstall.openSettings'), t('uninstall.openSettingsTip')], onClick: () => openInstalledApps(false) })
        : null,
      button({ label: t('uninstall.openClassic'), icon: 'external', size: 'small', variant: 'subtle', tip: [t('uninstall.openClassic'), t('uninstall.openClassicTip')], onClick: () => openInstalledApps(true) })),
  }));

  return section(t('uninstall.title'), [t('uninstall.title'), t('uninstall.info')], h('div', { class: 'action-list' }, cards));
}

function actionsFor(item, kind) {
  const out = [];
  const busy = runActive();
  const busyTip = busy ? t('run.alreadyRunning') : null;
  const alts = alternativesOf(item);
  const seenUrls = new Set();

  if (kind === 'update' && isSelectable(item)) {
    out.push(actionCard({
      ic: 'download', tone: 'accent', title: t('details.updateNow'), text: t('details.updateNowText', { version: availableVersionOf(item) || '?' }),
      control: button({ label: t('details.updateNowBtn'), variant: 'primary', size: 'small', disabled: busy,
        tip: [t('details.updateNow'), busyTip || t('details.updateNowTip')],
        onClick: () => { closeDetails(true); startJobs([buildJob(item)]); } }),
    }));
  }

  for (const alt of alts) {
    switch (alt.type) {
      case 'force_reinstall':
        out.push(actionCard({
          ic: 'refresh', title: t('alternatives.forceReinstall'), text: t('alternatives.forceReinstallText'),
          control: button({ label: t('alternatives.forceReinstallBtn'), size: 'small', disabled: busy,
            tip: [t('alternatives.forceReinstall'), busyTip || t('alternatives.forceReinstallTip')],
            onClick: () => { closeDetails(true); startJobs([buildJob(item, { type: 'force_reinstall' }, { includeUnknown: true })]); } }),
        }));
        break;
      case 'install_version':
        out.push(actionCard({
          ic: 'tag', title: t('alternatives.installVersion'), text: t('alternatives.installVersionText'),
          control: button({ label: t('alternatives.installVersionBtn'), size: 'small', disabled: busy,
            tip: [t('alternatives.installVersion'), busyTip || t('alternatives.installVersionTip')],
            onClick: () => pickVersion(item) }),
        }));
        break;
      case 'winget_candidate':
        out.push(actionCard({
          ic: 'package', tone: 'accent', title: t('alternatives.wingetCandidate', { name: alt.name || alt.id }),
          text: t('alternatives.wingetCandidateText', { id: alt.id || '?', version: alt.version || t('version.unknownShort') }),
          control: button({ label: t('alternatives.wingetCandidateBtn'), size: 'small', disabled: busy,
            tip: [t('alternatives.wingetCandidate', { name: alt.name || alt.id }), busyTip || t('alternatives.wingetCandidateTip')],
            onClick: () => installFromWinget(item, { id: alt.id, name: alt.name || alt.id, version: alt.version || null }) }),
        }));
        break;
      case 'latest_elsewhere':
        if (alt.url) seenUrls.add(alt.url);
        out.push(actionCard({
          ic: 'globe', tone: 'manual', title: t('alternatives.latestElsewhere', { version: alt.version || '?', origin: originName(alt.origin) }),
          text: t('alternatives.latestElsewhereText'),
          control: alt.url ? button({ label: t('alternatives.open'), icon: 'external', size: 'small', tip: [t('alternatives.open'), alt.url], onClick: () => openUrl(alt.url) }) : null,
        }));
        break;
      case 'managed_elsewhere':
        out.push(actionCard({ ic: 'autoupdate', title: t('alternatives.managedElsewhere', { by: alt.by || '?' }), text: t('alternatives.managedElsewhereText', { by: alt.by || '?' }) }));
        break;
      case 'store_update':
        out.push(actionCard({
          ic: 'store', title: t('alternatives.storeUpdate'), text: t('alternatives.storeUpdateText'),
          control: button({ label: t('alternatives.open'), icon: 'external', size: 'small', tip: [t('alternatives.storeUpdate'), t('alternatives.storeUpdateTip')], onClick: openStoreUpdates }),
        }));
        break;
      case 'open_url':
        break; // shown with the links
      default:
        break;
    }
  }

  // Unmanaged programs: winget search + web search
  if (kind === 'unmanaged' || kind === 'manual') {
    {
      out.push(actionCard({
        ic: 'search', title: t('alternatives.searchWinget'), text: t('alternatives.searchWingetText'),
        control: button({ label: t('alternatives.searchWingetBtn'), size: 'small', disabled: busy,
          tip: [t('alternatives.searchWinget'), busyTip || t('alternatives.searchWingetTip')],
          onClick: async () => {
            const chosen = await searchWinget(item);
            if (chosen) installFromWinget(item, chosen);
          } }),
      }));
    }
    const download = item.links?.download || alts.find((a) => a.type === 'open_url' && a.purpose === 'download')?.url;
    const search = alts.find((a) => a.type === 'open_url' && a.purpose === 'search')?.url || webSearchUrl(item.name || item.id);
    out.push(actionCard({
      ic: 'globe', title: t('alternatives.findOnline'), text: t('alternatives.findOnlineText'), stacked: true,
      control: h('div', { class: 'row-flex wrap' },
        download && !seenUrls.has(download) ? button({ label: t('links.download'), icon: 'external', size: 'small', tip: [t('links.download'), download], onClick: () => openUrl(download) }) : null,
        button({ label: t('alternatives.webSearch'), icon: 'search', size: 'small', tip: [t('alternatives.webSearch'), t('alternatives.webSearchTip')], onClick: () => openUrl(search) })),
    }));
  }

  // Ignore / un-ignore
  if (kind === 'update' || kind === 'manual') {
    if (item.ignored) {
      out.push(actionCard({
        ic: 'eye', title: t('ignore.undoTitle'), text: t('ignore.undoText'),
        control: button({ label: t('ignore.undo'), size: 'small', tip: [t('ignore.undo'), t('ignore.undoTip')], onClick: () => unignore(item.key) }),
      }));
    } else {
      const v = availableVersionOf(item);
      out.push(actionCard({
        ic: 'eye-off', title: t('ignore.title'), text: t('ignore.text'), stacked: true,
        control: h('div', { class: 'row-flex wrap' },
          v ? button({ label: t('ignore.version', { version: v }), size: 'small', variant: 'subtle', tip: [t('ignore.version', { version: v }), t('ignore.versionTip')], onClick: () => ignoreItem(item, v) }) : null,
          button({ label: t('ignore.item'), size: 'small', variant: 'subtle', tip: [t('ignore.item'), t('ignore.itemTip')], onClick: () => ignoreItem(item, null) })),
      }));
    }
  }
  return out;
}

async function pickVersion(item) {
  const version = await chooseVersion({
    id: item.id, source: item.source, name: item.name,
    installed: item.installedVersion, latest: item.availableVersion,
  });
  if (!version) return;
  closeDetails(true);
  startJobs([buildJob(item, { type: 'install_version', version })]);
}

function installFromWinget(item, cand) {
  closeDetails(true);
  startJobs([{
    key: item.key,
    source: 'winget',
    id: cand.id,
    name: cand.name || item.name,
    action: { type: 'force_reinstall' },
    fromVersion: item.installedVersion ?? null,
    toVersion: cand.version ?? null,
    scope: item.scope ?? null,
    includeUnknown: true,
    includePinned: false,
    downloadSize: null,
    installerType: null,
  }]);
}

function linksFor(item) {
  const urls = new Map();
  const add = (purpose, url) => { if (url && !urls.has(url)) urls.set(url, purpose); };
  const L = item.links || {};
  add('homepage', L.homepage);
  add('releaseNotes', L.releaseNotes);
  add('download', L.download);
  add('support', L.support);
  for (const a of alternativesOf(item)) {
    if (a.type === 'open_url' && a.url) {
      const map = { homepage: 'homepage', download: 'download', release_notes: 'releaseNotes', support: 'support', search: 'search' };
      add(map[a.purpose] || 'homepage', a.url);
    }
  }
  const icons = { homepage: 'home', releaseNotes: 'notes', download: 'download', support: 'support', search: 'search' };
  return [...urls.entries()].map(([url, purpose]) => button({
    label: t(`links.${purpose}`), icon: icons[purpose] || 'link', size: 'small',
    tip: [t(`links.${purpose}`), url], onClick: () => openUrl(url),
  }));
}

function techInfo(item) {
  const src = sourceInfo(item.source);
  const rows = [
    [t('details.source'), h('span', { class: 'row-flex' }, icon(src.icon), src.label), src.tip],
    [t('details.id'), h('code', null, item.id || '—'), [t('details.id'), t('details.idTip')]],
  ];
  if (item.scope) {
    rows.push([t('details.scopeLabel'), t(`details.scope.${item.scope === 'user' ? 'user' : 'machine'}`),
      [t(`details.scope.${item.scope === 'user' ? 'user' : 'machine'}`), t(`details.scope.${item.scope === 'user' ? 'userTip' : 'machineTip'}`)]]);
  }
  if (['update', 'up_to_date'].includes(kindOf(item))) {
    const admin = needsElevation(buildJob(item));
    rows.push([t('details.admin'), admin ? t('common.yes') : t('common.no'), [t('row.adminTitle'), admin ? t('row.adminTip') : t('details.noAdminTip')]]);
  }
  if (item.installerType) rows.push([t('details.installer'), item.installerType, [t('details.installer'), t('details.installerTip')]]);
  if (item.downloadSize) rows.push([t('details.size'), fmt.bytes(item.downloadSize), [t('row.sizeTitle'), t('row.sizeTip')]]);
  if (item.installLocation) rows.push([t('details.location'), h('code', { class: 'ellipsis' }, item.installLocation), [t('details.location'), item.installLocation]]);
  if (item.explicitOnly) rows.push([t('reasons.requires_explicit.label'), t('common.yes'), [t('reasons.requires_explicit.title'), t('reasons.requires_explicit.text')]]);
  if (item.pinned) rows.push([t('reasons.pinned.label'), t('common.yes'), [t('reasons.pinned.title'), t('reasons.pinned.text')]]);
  if (normSource(item.source) === 'unknown') rows.push([t('details.source'), String(item.source), null]);
  return h('dl', { class: 'tech' }, rows.map(([k, v, tip]) => [
    h('dt', { tip, tabindex: tip ? 0 : undefined }, k),
    h('dd', null, v),
  ]));
}
