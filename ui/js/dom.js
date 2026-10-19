// Tiny DOM helpers (no framework).

const ATTR_KEYS = new Set(['role', 'tabindex', 'for', 'type', 'name', 'id', 'title', 'href', 'src', 'alt',
  'value', 'placeholder', 'min', 'max', 'step', 'autocomplete', 'spellcheck', 'lang', 'inert', 'width', 'height']);

/**
 * h('button', { class: 'btn', onClick: fn, tip: [title, text], 'aria-label': 'x' }, 'Label', iconEl)
 * - `class` string or array; `style` string or object; `dataset` object
 * - `on<Event>` functions become listeners
 * - `tip`: [title, text] for the tooltip component
 * - `html`: trusted markup (icons only)
 * - children: strings, numbers, nodes, arrays; null/false/undefined skipped
 */
export function h(tag, props, ...children) {
  const el = document.createElement(tag);
  if (props) applyProps(el, props);
  append(el, children);
  return el;
}

export function applyProps(el, props) {
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === 'class') {
      el.className = Array.isArray(value) ? value.filter(Boolean).join(' ') : value;
    } else if (key === 'style') {
      if (typeof value === 'string') el.style.cssText = value;
      else for (const [k, v] of Object.entries(value)) {
        if (v === undefined || v === null) continue;
        if (k.startsWith('--')) el.style.setProperty(k, v);
        else el.style[k] = v;
      }
    } else if (key === 'dataset') {
      for (const [k, v] of Object.entries(value)) if (v !== undefined && v !== null) el.dataset[k] = v;
    } else if (key === 'tip') {
      setTip(el, value[0], value[1]);
    } else if (key === 'html') {
      el.innerHTML = value;
    } else if (key.startsWith('on') && typeof value === 'function') {
      el.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (key.startsWith('aria-') || key.startsWith('data-') || ATTR_KEYS.has(key)) {
      if (key === 'value' && 'value' in el) el.value = value;
      else el.setAttribute(key, value === true ? '' : String(value));
    } else if (key in el) {
      el[key] = value;
    } else {
      el.setAttribute(key, value === true ? '' : String(value));
    }
  }
}

export function append(el, children) {
  for (const child of children) {
    if (child === null || child === undefined || child === false || child === true) continue;
    if (Array.isArray(child)) append(el, child);
    else if (child instanceof Node) el.appendChild(child);
    else el.appendChild(document.createTextNode(String(child)));
  }
  return el;
}

/** Sets the tooltip of an element (see components/tooltip.js). */
export function setTip(el, title, text) {
  if (title) el.dataset.tipTitle = title; else delete el.dataset.tipTitle;
  if (text) el.dataset.tipText = text; else delete el.dataset.tipText;
  if (!title && !text) el.removeAttribute('data-tip');
  else el.setAttribute('data-tip', '');
}

export function clear(el) {
  while (el.firstChild) el.removeChild(el.firstChild);
  return el;
}

export function frag(...children) {
  return append(document.createDocumentFragment(), children);
}

export function debounce(fn, ms) {
  let timer = null;
  const wrapped = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
  wrapped.cancel = () => clearTimeout(timer);
  return wrapped;
}

export function reducedMotion() {
  try { return window.matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; }
}

/** Plays the closing animation (class `closing`) then calls done. */
export function animateOut(el, done, fallbackMs = 260) {
  if (!el || reducedMotion()) { done?.(); return; }
  let finished = false;
  const finish = () => { if (finished) return; finished = true; done?.(); };
  el.classList.add('closing');
  el.addEventListener('animationend', finish, { once: true });
  setTimeout(finish, fallbackMs);
}

/** Simple event bus. */
export function createBus() {
  const map = new Map();
  return {
    on(name, fn) {
      if (!map.has(name)) map.set(name, new Set());
      map.get(name).add(fn);
      return () => map.get(name)?.delete(fn);
    },
    emit(name, data) {
      for (const fn of [...(map.get(name) || [])]) {
        try { fn(data); } catch (e) { console.error(`[bus] ${name}`, e); }
      }
    },
  };
}

/** Deterministic color for letter avatars (royal-friendly hues). */
const AVATAR_COLORS = ['#5B3FD6', '#7B3FD3', '#2C66BA', '#2F7D80', '#B5487A', '#B8661A', '#4A5BC4', '#1E7F6A', '#8A4FB8', '#3C6E9E'];
export function avatarColor(seed) {
  let hash = 0;
  for (const ch of String(seed || '?')) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
  return AVATAR_COLORS[hash % AVATAR_COLORS.length];
}

export function initials(name) {
  const s = String(name || '?').trim();
  const m = s.match(/[\p{L}\p{N}]/u);
  return (m ? m[0] : '?').toUpperCase();
}
