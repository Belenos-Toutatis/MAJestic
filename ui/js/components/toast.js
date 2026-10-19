// Short non-blocking notifications (polite live region).

import { h, animateOut } from '../dom.js';
import { icon } from '../icons.js';

let host = null;

function ensureHost() {
  if (!host) {
    host = h('div', { class: 'toasts', role: 'status', 'aria-live': 'polite' });
    document.body.appendChild(host);
  }
  return host;
}

/** toast('Saved', { tone: 'success'|'danger'|'info', duration }) */
export function toast(message, { tone = 'info', duration = 3200 } = {}) {
  const ic = tone === 'success' ? 'success' : tone === 'danger' ? 'error' : 'info';
  const el = h('div', { class: ['toast', `tone-${tone}`] }, icon(ic), h('span', null, message));
  const parent = ensureHost();
  parent.appendChild(el);
  while (parent.children.length > 3) parent.firstElementChild.remove();
  setTimeout(() => {
    el.classList.add('leaving');
    animateOut(el, () => el.remove(), 260);
  }, duration);
}
