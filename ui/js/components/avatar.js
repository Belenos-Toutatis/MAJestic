// Program icons: letter avatar immediately, real icon (get_icon) loaded lazily
// when the avatar scrolls into view. Results are cached per hint.

import { h, avatarColor, initials } from '../dom.js';
import { invoke } from '../api.js';

const cache = new Map(); // hint -> Promise<string|null>
let observer = null;

function load(hint) {
  if (!cache.has(hint)) {
    cache.set(hint, invoke('get_icon', { hint })
      .then((url) => (typeof url === 'string' && url.startsWith('data:') ? url : null))
      .catch(() => null));
  }
  return cache.get(hint);
}

function apply(el) {
  const hint = el.dataset.iconHint;
  if (!hint) return;
  load(hint).then((url) => {
    if (!url || !el.isConnected || el.querySelector('img')) return;
    const img = new Image();
    img.alt = '';
    img.decoding = 'async';
    img.onload = () => { el.classList.add('has-icon'); el.appendChild(img); };
    img.src = url;
  });
}

function getObserver() {
  if (!observer && 'IntersectionObserver' in window) {
    observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          observer.unobserve(entry.target);
          apply(entry.target);
        }
      }
    }, { rootMargin: '200px' });
  }
  return observer;
}

/** avatar(item, { large }) */
export function avatar(item, { large = false } = {}) {
  const name = item?.name || '?';
  const el = h('span', {
    class: ['avatar', large ? 'large' : null],
    style: { background: avatarColor(name) },
    'aria-hidden': 'true',
  }, initials(name));
  const hint = item?.iconHint;
  if (hint) {
    el.dataset.iconHint = hint;
    const obs = getObserver();
    if (obs) obs.observe(el); else apply(el);
  }
  return el;
}
