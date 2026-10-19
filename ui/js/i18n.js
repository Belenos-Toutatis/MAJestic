// Translations and locale-aware formatting.
//
// String tables live in ui/i18n/<lang>.js as nested objects. A value is either
// a string with {placeholders}, or an object of plural forms
// ({ zero?, one, other, ... } chosen with Intl.PluralRules on params.count).

import fr from '../i18n/fr.js';
import en from '../i18n/en.js';

const TABLES = { fr, en };
export const LANGUAGES = ['fr', 'en'];

let lang = 'fr';
let locale = 'fr-FR';
let plural = new Intl.PluralRules('fr');

/** Resolves the effective language: setting (fr/en/null) then Windows UI language. */
export function resolveLanguage(setting, uiLanguage) {
  if (setting && TABLES[setting]) return setting;
  const sys = String(uiLanguage || navigator.language || 'en').toLowerCase();
  return sys.startsWith('fr') ? 'fr' : 'en';
}

export function setLanguage(code, uiLanguage) {
  lang = TABLES[code] ? code : 'en';
  const sys = String(uiLanguage || navigator.language || '');
  locale = sys.toLowerCase().startsWith(lang) ? sys : (lang === 'fr' ? 'fr-FR' : 'en-US');
  try { new Intl.NumberFormat(locale); } catch { locale = lang; }
  plural = new Intl.PluralRules(locale);
  cache.clear();
  document.documentElement.lang = lang;
}

export function getLanguage() { return lang; }
export function getLocale() { return locale; }

function lookup(table, key) {
  let node = table;
  for (const part of key.split('.')) {
    if (node == null || typeof node !== 'object') return undefined;
    node = node[part];
  }
  return node;
}

function interpolate(str, params) {
  if (!params) return str;
  return str.replace(/\{(\w+)\}/g, (m, name) => {
    const v = params[name];
    if (v === undefined || v === null) return '';
    return typeof v === 'number' ? fmt.number(v) : String(v);
  });
}

/** t('results.title'), t('results.count', { count: 3 }). Missing key → the key itself. */
export function t(key, params) {
  let value = lookup(TABLES[lang], key);
  if (value === undefined) value = lookup(TABLES.en, key);
  if (value === undefined) value = lookup(TABLES.fr, key);
  if (value === undefined) {
    console.warn('[i18n] missing', key);
    return key;
  }
  if (typeof value === 'object' && value !== null) {
    const count = Number(params?.count ?? 0);
    let form = value.other;
    // French plural rules put 0 in "one", whose strings often spell out "1":
    // zero uses its own form, else "other" ("0 installées").
    if (count === 0) form = value.zero ?? value.other;
    else {
      const cat = plural.select(count);
      form = value[cat] ?? value.other;
    }
    return interpolate(String(form ?? key), params);
  }
  return interpolate(String(value), params);
}

/** True when the key exists (lets callers fall back for unknown enum values). */
export function has(key) {
  return lookup(TABLES[lang], key) !== undefined || lookup(TABLES.en, key) !== undefined;
}

// --------------------------------------------------------------- formatting

const cache = new Map();
function nf(opts) {
  const k = 'n' + JSON.stringify(opts);
  if (!cache.has(k)) {
    try { cache.set(k, new Intl.NumberFormat(locale, opts)); }
    catch { cache.set(k, new Intl.NumberFormat(locale)); }
  }
  return cache.get(k);
}
function df(opts) {
  const k = 'd' + JSON.stringify(opts);
  if (!cache.has(k)) cache.set(k, new Intl.DateTimeFormat(locale, opts));
  return cache.get(k);
}

const BYTE_UNITS = [['gigabyte', 1024 ** 3], ['megabyte', 1024 ** 2], ['kilobyte', 1024], ['byte', 1]];

function parseDate(value) {
  if (!value) return null;
  const d = value instanceof Date ? value : new Date(value);
  return Number.isNaN(d.getTime()) ? null : d;
}

export const fmt = {
  number(n, opts) {
    if (n === null || n === undefined || Number.isNaN(Number(n))) return '—';
    return nf(opts || {}).format(n);
  },
  percent(f) {
    return nf({ style: 'percent', maximumFractionDigits: 0 }).format(Math.max(0, Math.min(1, f || 0)));
  },
  bytes(n) {
    if (n === null || n === undefined || !Number.isFinite(Number(n))) return '—';
    n = Number(n);
    const [unit, size] = BYTE_UNITS.find(([, s]) => n >= s) || BYTE_UNITS[3];
    const v = n / size;
    return nf({ style: 'unit', unit, unitDisplay: 'short', maximumFractionDigits: v < 10 && unit !== 'byte' ? 1 : 0 }).format(v);
  },
  speed(bps) {
    if (!bps || !Number.isFinite(bps)) return '—';
    const [unit, size] = BYTE_UNITS.find(([, s]) => bps >= s) || BYTE_UNITS[3];
    const v = bps / size;
    return nf({ style: 'unit', unit: `${unit}-per-second`, unitDisplay: 'short', maximumFractionDigits: v < 10 ? 1 : 0 }).format(v);
  },
  /** 3 min 20 s / 1 h 05 min / 45 s */
  duration(secs) {
    if (secs === null || secs === undefined || !Number.isFinite(Number(secs))) return '—';
    let s = Math.max(0, Math.round(Number(secs)));
    const h = Math.floor(s / 3600); s -= h * 3600;
    const m = Math.floor(s / 60); s -= m * 60;
    const u = (unit, v) => nf({ style: 'unit', unit, unitDisplay: 'short' }).format(v);
    if (h) return m ? `${u('hour', h)} ${u('minute', m)}` : u('hour', h);
    if (m) return s ? `${u('minute', m)} ${u('second', s)}` : u('minute', m);
    return u('second', s);
  },
  /** Rounded to calm steps, for estimates. */
  roughDuration(secs) {
    if (!Number.isFinite(Number(secs))) return '—';
    return fmt.duration(roundEta(Number(secs)));
  },
  date(value, opts = { dateStyle: 'medium' }) {
    const d = parseDate(value);
    return d ? df(opts).format(d) : '—';
  },
  time(value) {
    const d = parseDate(value);
    return d ? df({ hour: '2-digit', minute: '2-digit' }).format(d) : '—';
  },
  dateTime(value) {
    const d = parseDate(value);
    return d ? df({ dateStyle: 'medium', timeStyle: 'short' }).format(d) : '—';
  },
  /** "il y a 5 minutes" */
  relative(value) {
    const d = parseDate(value);
    if (!d) return '—';
    const diff = (d.getTime() - Date.now()) / 1000;
    const abs = Math.abs(diff);
    const rtf = cache.get('rtf') || (cache.set('rtf', new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })), cache.get('rtf'));
    if (abs < 45) return rtf.format(0, 'second');
    if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute');
    if (abs < 86400) return rtf.format(Math.round(diff / 3600), 'hour');
    return rtf.format(Math.round(diff / 86400), 'day');
  },
  /** "Aujourd'hui", "Hier", or the full date. */
  dayLabel(value) {
    const d = parseDate(value);
    if (!d) return '—';
    const today = new Date(); today.setHours(0, 0, 0, 0);
    const day = new Date(d); day.setHours(0, 0, 0, 0);
    const delta = Math.round((today - day) / 86400000);
    if (delta === 0 || delta === 1) {
      const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
      const s = rtf.format(-delta, 'day');
      return s.charAt(0).toUpperCase() + s.slice(1);
    }
    const s = df({ weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }).format(d);
    return s.charAt(0).toUpperCase() + s.slice(1);
  },
  list(items, type = 'conjunction') {
    const arr = items.filter(Boolean).map(String);
    try { return new Intl.ListFormat(locale, { style: 'long', type }).format(arr); }
    catch { return arr.join(', '); }
  },
};

/** Calm rounding for remaining-time estimates (seconds in, seconds out). */
export function roundEta(secs) {
  if (secs < 60) return Math.max(5, Math.ceil(secs / 5) * 5);
  if (secs < 600) return Math.round(secs / 10) * 10;
  if (secs < 3600) return Math.round(secs / 60) * 60;
  return Math.round(secs / 300) * 300;
}

/** "≈ 3 min 20 s restantes" / "moins d'une minute". */
export function etaText(secs) {
  if (secs === null || secs === undefined || !Number.isFinite(secs)) return t('eta.computing');
  if (secs < 60) return t('eta.lessThanMinute');
  const rounded = roundEta(secs);
  // grammatical number follows the leading unit ("1 min 30 s restante" / "3 min restantes")
  const single = rounded === 60 || rounded === 3600;
  return t('eta.remaining', { duration: fmt.duration(rounded), count: single ? 1 : 2 });
}
