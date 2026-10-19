// Custom tooltip: one shared element, shown ~400 ms after hover or keyboard focus
// on any element carrying data-tip (data-tip-title / data-tip-text, see dom.setTip).
// The target gets aria-describedby pointing to the tooltip as soon as it has focus,
// so screen readers announce the explanation even before it becomes visible.

const DELAY = 400;
const MARGIN = 8;

let tipEl, titleEl, textEl;
let current = null;
let timer = null;
let lastPointer = { x: 0, y: 0 };

export function initTooltips() {
  tipEl = document.createElement('div');
  tipEl.id = 'tooltip';
  tipEl.className = 'tooltip';
  tipEl.setAttribute('role', 'tooltip');
  titleEl = document.createElement('span');
  titleEl.className = 'tt-title';
  textEl = document.createElement('span');
  textEl.className = 'tt-text';
  tipEl.append(titleEl, textEl);
  document.body.appendChild(tipEl);

  document.addEventListener('pointerover', (e) => {
    if (e.pointerType === 'touch') return;
    const target = findTarget(e.target);
    if (target === current) return;
    if (target) schedule(target, 'pointer');
    else if (current && !current.contains(document.activeElement)) hide();
  });
  document.addEventListener('pointermove', (e) => { lastPointer = { x: e.clientX, y: e.clientY }; }, { passive: true });
  document.addEventListener('pointerout', (e) => {
    if (!current) return;
    const to = e.relatedTarget;
    if (to && current.contains(to)) return;
    if (findTarget(to) === current) return;
    if (current === document.activeElement && current.matches(':focus-visible')) return;
    hide();
  });
  document.addEventListener('focusin', (e) => {
    const target = findTarget(e.target);
    if (!target) { hide(); return; }
    // Only for keyboard focus: mouse clicks also focus buttons.
    let keyboard = true;
    try { keyboard = e.target.matches(':focus-visible'); } catch { /* old engines */ }
    if (keyboard) schedule(target, 'focus');
  });
  document.addEventListener('focusout', () => { if (current && !current.matches(':hover')) hide(); });
  document.addEventListener('pointerdown', () => hide(true), true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && tipEl.classList.contains('visible')) hide(); }, true);
  window.addEventListener('scroll', () => hide(true), true);
  window.addEventListener('blur', () => hide(true));
}

function findTarget(node) {
  if (!node || node.nodeType !== 1) node = node?.parentElement;
  const el = node?.closest?.('[data-tip]');
  return el && (el.dataset.tipTitle || el.dataset.tipText) ? el : null;
}

/** A modal <dialog> lives in the top layer: the tooltip must sit inside it to show above it. */
function attachHost() {
  const dialogs = [...document.querySelectorAll('dialog[open]')];
  const host = dialogs.length ? dialogs[dialogs.length - 1] : document.body;
  if (tipEl.parentNode !== host) host.appendChild(tipEl);
}

function schedule(target, mode) {
  clearTimeout(timer);
  detach();
  attachHost();
  current = target;
  fill(target);
  target.setAttribute('aria-describedby', 'tooltip');
  timer = setTimeout(() => show(target, mode), DELAY);
}

function fill(target) {
  const title = target.dataset.tipTitle || '';
  const text = target.dataset.tipText || '';
  titleEl.textContent = title;
  textEl.textContent = text;
  titleEl.hidden = !title;
  textEl.hidden = !text;
}

function show(target, mode) {
  if (!target.isConnected || current !== target) return;
  attachHost();
  fill(target);
  tipEl.classList.remove('visible', 'above');
  tipEl.style.left = '0px';
  tipEl.style.top = '0px';
  const r = target.getBoundingClientRect();
  const tw = tipEl.offsetWidth;
  const th = tipEl.offsetHeight;
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  // Anchor: element center (keyboard) or element center horizontally under the pointer row (mouse).
  let cx = r.left + r.width / 2;
  if (mode === 'pointer' && r.width > 240) cx = Math.min(Math.max(lastPointer.x, r.left), r.right);
  let left = cx - tw / 2;
  left = Math.max(MARGIN, Math.min(left, vw - tw - MARGIN));
  let top = r.bottom + 8;
  let above = false;
  if (top + th > vh - MARGIN) {
    top = r.top - th - 8;
    above = true;
  }
  if (top < MARGIN) top = Math.min(vh - th - MARGIN, Math.max(MARGIN, r.bottom + 8));
  tipEl.style.left = `${Math.round(left)}px`;
  tipEl.style.top = `${Math.round(top)}px`;
  tipEl.classList.toggle('above', above);
  // next frame so the transition runs
  requestAnimationFrame(() => tipEl.classList.add('visible'));
}

function detach() {
  if (current && current.getAttribute('aria-describedby') === 'tooltip') current.removeAttribute('aria-describedby');
}

export function hide(immediate = false) {
  clearTimeout(timer);
  if (!tipEl) return;
  tipEl.classList.remove('visible');
  if (immediate) tipEl.style.visibility = 'hidden', requestAnimationFrame(() => { tipEl.style.visibility = ''; });
  detach();
  current = null;
}
