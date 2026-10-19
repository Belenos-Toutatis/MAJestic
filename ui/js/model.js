// Helpers that interpret backend data (model.rs / settings.rs) defensively:
// every optional field may be null, every enum may hold an unknown value.

import { t, has, fmt } from './i18n.js';

export const PRIORITIES = ['critical', 'major', 'recommended', 'normal', 'uncertain'];
export const CATEGORY_ORDER = ['game', 'web_app', 'auto_updating', 'driver', 'system_component', 'store_app', 'application'];
export const SOURCES = ['winget', 'ms_store', 'chocolatey', 'scoop', 'registry', 'msix'];

const SOURCE_ICONS = {
  winget: 'package', ms_store: 'store', chocolatey: 'choco', scoop: 'scoop', registry: 'registry', msix: 'store',
};

export function normSource(source) {
  const s = String(source || '').toLowerCase();
  if (s === 'msstore' || s === 'ms_store' || s === 'store') return 'ms_store';
  if (s === 'choco') return 'chocolatey';
  return SOURCES.includes(s) ? s : 'unknown';
}

export function sourceInfo(source) {
  const key = normSource(source);
  return {
    key,
    icon: SOURCE_ICONS[key] || 'app',
    label: t(`sources.${key}.label`),
    tip: [t(`sources.${key}.label`), t(`sources.${key}.tip`)],
  };
}

export function priorityOf(item) {
  const p = item?.assessment?.priority;
  return PRIORITIES.includes(p) ? p : 'normal';
}

export function kindOf(item) {
  const k = item?.kind;
  return ['update', 'manual', 'up_to_date', 'unmanaged'].includes(k) ? k : 'unmanaged';
}

export function categoryOf(item) {
  const type = item?.category?.type;
  return CATEGORY_ORDER.includes(type) ? type : 'application';
}

export function reasonsOf(item) {
  return Array.isArray(item?.assessment?.reasons) ? item.assessment.reasons.filter((r) => r && typeof r === 'object') : [];
}

export function alternativesOf(item) {
  return Array.isArray(item?.alternatives) ? item.alternatives.filter((a) => a && typeof a === 'object') : [];
}

export function runningOf(item) {
  return Array.isArray(item?.running) ? item.running.filter(Boolean) : [];
}

export function isSelectable(item) {
  return kindOf(item) === 'update' && !item.ignored;
}

/** Mirror of JobRequest::needs_elevation (model.rs). */
export function needsElevation(job) {
  if (job?.action?.type === 'store_update_scan') return true;
  const src = normSource(job?.source);
  if (src === 'scoop' || src === 'ms_store' || src === 'msix') return false;
  if (src === 'chocolatey') return true;
  return job?.scope !== 'user';
}

/** JobRequest from an UpdateItem (docs/ARCHITECTURE.md). */
export function buildJob(item, action = { type: 'upgrade' }, overrides = {}) {
  return {
    key: item.key,
    source: item.source,
    id: item.id,
    name: item.name,
    action,
    fromVersion: item.installedVersion ?? null,
    toVersion: action.type === 'install_version' ? action.version : (item.availableVersion ?? null),
    scope: item.scope ?? null,
    includeUnknown: item.versionCertainty !== 'exact',
    includePinned: !!item.pinned,
    downloadSize: item.downloadSize ?? null,
    installerType: item.installerType ?? null,
    ...overrides,
  };
}

// ------------------------------------------------------------------ versions

/** Display of the installed version with its certainty. */
export function installedVersionInfo(item) {
  const certainty = ['exact', 'approximate', 'from_file', 'unknown'].includes(item?.versionCertainty)
    ? item.versionCertainty : 'exact';
  const raw = item?.installedVersion;
  if (!raw || certainty === 'unknown') {
    return { text: t('version.unknown'), certainty: 'unknown', tip: [t('version.unknownTitle'), t('version.unknownTip')] };
  }
  if (certainty === 'approximate') {
    const clean = String(raw).replace(/^[<>]\s*/, '');
    return { text: `≈ ${clean}`, certainty, tip: [t('version.approxTitle'), t('version.approxTip', { raw })] };
  }
  if (certainty === 'from_file') {
    return { text: raw, certainty, note: t('version.fromFile'), tip: [t('version.fromFileTitle'), t('version.fromFileTip')] };
  }
  return { text: raw, certainty, tip: [t('version.installedTitle'), t('version.installedTip')] };
}

export function availableVersionOf(item) {
  if (item?.availableVersion) return item.availableVersion;
  const elsewhere = alternativesOf(item).find((a) => a.type === 'latest_elsewhere');
  return elsewhere?.version || null;
}

// ------------------------------------------------------------------ reasons

const REASON_META = {
  known_exploited: { tone: 'critical', icon: 'bug' },
  security_fix: { tone: 'critical', icon: 'shield' },
  exposed: { tone: 'recommended', icon: 'globe' },
  major_upgrade: { tone: 'major', icon: 'rocket' },
  unknown_version: { tone: 'uncertain', icon: 'help' },
  approximate_version: { tone: 'uncertain', icon: 'help' },
  version_from_file: { tone: 'uncertain', icon: 'file' },
  requires_explicit: { tone: 'normal', icon: 'target' },
  pinned: { tone: 'uncertain', icon: 'pin' },
  app_running: { tone: 'warning', icon: 'activity' },
  duplicate_manager: { tone: 'uncertain', icon: 'duplicate' },
};

function exposureLabel(category) {
  return has(`exposure.${category}`) ? t(`exposure.${category}`) : t('exposure.other');
}

/** { type, label, title, text, tone, icon } — translated, never throws. */
export function describeReason(reason) {
  const type = reason?.type;
  const meta = REASON_META[type] || { tone: 'uncertain', icon: 'info' };
  const base = `reasons.${type}`;
  if (!REASON_META[type]) {
    return { type: 'unknown', ...meta, label: t('reasons.unknown.label'), title: t('reasons.unknown.label'), text: t('reasons.unknown.text') };
  }
  const p = {};
  switch (type) {
    case 'known_exploited': {
      const cves = Array.isArray(reason.cves) ? reason.cves.filter(Boolean) : [];
      p.cves = cves.length ? fmt.list(cves.slice(0, 5)) : t('reasons.known_exploited.noCve');
      p.date = reason.mostRecent ? fmt.date(reason.mostRecent) : '—';
      // the ransomware note is shown separately (badge / highlighted line)
      const text = `${t(`${base}.text`, p)} ${reason.recent ? t(`${base}.recent`, p) : t(`${base}.old`, p)}`;
      return { type, ...meta, label: t(`${base}.label`), title: t(`${base}.title`), text, ransomware: !!reason.ransomware, recent: !!reason.recent, cves };
    }
    case 'security_fix': {
      const kw = Array.isArray(reason.keywords) ? reason.keywords.filter(Boolean) : [];
      p.keywords = kw.length ? fmt.list(kw.slice(0, 4).map((k) => t('quote', { text: k }))) : '—';
      break;
    }
    case 'exposed':
      p.category = exposureLabel(reason.category);
      break;
    case 'major_upgrade':
      p.from = reason.from || '?';
      p.to = reason.to || '?';
      break;
    case 'version_from_file':
      p.version = reason.version || '?';
      break;
    case 'app_running': {
      const procs = Array.isArray(reason.processes) ? reason.processes.filter(Boolean) : [];
      p.processes = procs.length ? fmt.list(procs) : '—';
      break;
    }
    case 'duplicate_manager':
      p.other = sourceInfo(reason.other).label;
      break;
    default:
      break;
  }
  return { type, ...meta, label: t(`${base}.label`, p), title: t(`${base}.title`, p), text: t(`${base}.text`, p) };
}

// ------------------------------------------------------------------ categories

const CATEGORY_ICONS = {
  game: 'gamepad', web_app: 'webapp', auto_updating: 'autoupdate', driver: 'chip',
  system_component: 'component', store_app: 'store', application: 'app',
};

export function categoryInfo(item) {
  const type = categoryOf(item);
  const cat = item?.category || {};
  const by = cat.launcher || cat.browser || cat.by || null;
  return {
    type,
    icon: CATEGORY_ICONS[type],
    label: t(`categories.${type}.label`),
    by,
    short: by ? t(`categories.${type}.by`, { by }) : t(`categories.${type}.label`),
    explain: t(`categories.${type}.explain`),
  };
}

// ------------------------------------------------------------------ jobs

export const FINAL_STATES = ['succeeded', 'failed', 'cancelled', 'skipped'];

export function stateType(state) {
  const type = state?.type;
  return ['queued', 'closing_apps', 'starting', 'downloading', 'installing', ...FINAL_STATES].includes(type) ? type : 'queued';
}

export function isFinal(state) {
  return FINAL_STATES.includes(state?.type);
}

const FAILURE_ACTIONS = {
  app_running: ['closeRetry', 'retry'],
  install_in_progress: ['retry'],
  reboot_required: ['retry'],
  no_network: ['retry'],
  disk_full: ['retry'],
  cancelled_by_user: ['retry', 'interactive'],
  blocked_by_policy: ['download'],
  hash_mismatch: ['download', 'retry'],
  no_applicable_installer: ['download'],
  upgrade_not_applicable: ['interactive', 'download'],
  downgrade_refused: ['download'],
  already_installed: [],
  missing_dependency: ['retry', 'download'],
  package_not_found: ['download'],
  elevation_required: ['retry'],
  elevation_refused: ['retry', 'noAdmin'],
  installer_failed: ['interactive', 'retry'],
  timeout: ['interactive', 'retry'],
  unknown: ['interactive', 'logs'],
};

export function failureInfo(kind) {
  const k = has(`failures.${kind}.title`) ? kind : 'unknown';
  return {
    kind: k,
    title: t(`failures.${k}.title`),
    text: t(`failures.${k}.text`),
    actions: FAILURE_ACTIONS[k] || ['retry'],
  };
}
