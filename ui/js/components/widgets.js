// Small UI building blocks. Every interactive element takes a tooltip.

import { h } from '../dom.js';
import { icon } from '../icons.js';
import { t } from '../i18n.js';

/** button({ label, icon, variant: 'primary'|'royal'|'subtle'|'danger', size, tip: [title, text], onClick }) */
export function button(opts) {
  const { label, icon: ic, variant, size, tip, onClick, disabled, ariaLabel, cls, type = 'button' } = opts;
  const el = h('button', {
    type,
    class: ['btn', variant, size, cls],
    tip,
    disabled: !!disabled,
    'aria-label': ariaLabel,
    onClick,
  }, ic ? icon(ic) : null, label ? h('span', { class: 'btn-label' }, label) : null);
  return el;
}

export function iconButton({ icon: ic, label, tip, onClick, cls, tabindex }) {
  return h('button', {
    type: 'button',
    class: ['icon-btn', cls],
    'aria-label': label,
    tip: tip || [label],
    tabindex,
    onClick,
  }, icon(ic));
}

/** "?" marker explaining a notion. Focusable so keyboard users get the tooltip too. */
export function infoDot(title, text, { tabindex = 0 } = {}) {
  return h('button', {
    type: 'button',
    class: 'info-dot',
    'aria-label': t('common.moreInfo', { topic: title }),
    tip: [title, text],
    tabindex,
    onClick: (e) => e.stopPropagation(),
  }, icon('help'));
}

/** badge({ text, tone, icon, tip, focusable }) */
export function badge({ text, tone, icon: ic, tip, outline, focusable, cls }) {
  return h('span', {
    class: ['badge', tone ? `tone-${tone}` : null, outline ? 'outline' : null, cls],
    tip,
    tabindex: focusable ? -1 : undefined,
    'data-rowfocus': focusable ? '' : undefined,
  }, ic ? icon(ic) : null, text);
}

export function toggle({ checked, label, tip, onChange, id }) {
  const el = h('button', {
    type: 'button',
    class: 'switch',
    role: 'switch',
    id,
    'aria-checked': String(!!checked),
    'aria-label': label,
    tip,
  });
  el.addEventListener('click', () => {
    const next = el.getAttribute('aria-checked') !== 'true';
    el.setAttribute('aria-checked', String(next));
    onChange?.(next, el);
  });
  return el;
}

/** segmented({ value, options: [{ value, label, icon, tip }], onChange, label }) — radio group. */
export function segmented({ value, options, onChange, label, idPrefix }) {
  const group = h('div', { class: 'segmented', role: 'radiogroup', 'aria-label': label });
  const buttons = options.map((opt) => h('button', {
    type: 'button',
    role: 'radio',
    id: idPrefix ? `${idPrefix}-${opt.value}` : undefined,
    'aria-checked': String(opt.value === value),
    tabindex: opt.value === value ? 0 : -1,
    tip: opt.tip,
    onClick: () => select(opt.value, true),
  }, opt.icon ? icon(opt.icon) : null, opt.label));
  function select(v, notify) {
    options.forEach((o, i) => {
      buttons[i].setAttribute('aria-checked', String(o.value === v));
      buttons[i].tabIndex = o.value === v ? 0 : -1;
    });
    if (notify) onChange?.(v);
  }
  group.addEventListener('keydown', (e) => {
    const idx = buttons.indexOf(document.activeElement);
    if (idx < 0) return;
    let next = null;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (idx + 1) % buttons.length;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (idx - 1 + buttons.length) % buttons.length;
    if (next === null) return;
    e.preventDefault();
    buttons[next].focus();
    select(options[next].value, true);
  });
  group.append(...buttons);
  return group;
}

/** Native <select> (accessible, keyboard friendly), styled. */
export function select({ value, options, onChange, label, tip, id }) {
  const el = h('select', { class: 'select', 'aria-label': label, tip, id },
    options.map((o) => h('option', { value: o.value }, o.label)));
  el.value = value;
  el.addEventListener('change', () => onChange?.(el.value));
  return el;
}

export function spinner(small) {
  return h('span', { class: ['spinner', small ? 'small' : null], role: 'presentation' });
}

export function emptyState({ icon: ic, title, text, action }) {
  return h('div', { class: 'empty' },
    h('div', { class: 'empty-icon' }, icon(ic || 'info')),
    h('h2', null, title),
    text ? h('p', null, text) : null,
    action || null);
}

export function banner({ tone = 'info', icon: ic, title, text, children, actions }) {
  return h('div', { class: ['banner', `tone-${tone}`], role: tone === 'danger' ? 'alert' : 'status' },
    icon(ic || (tone === 'warning' ? 'warning' : tone === 'danger' ? 'error' : 'info')),
    h('div', { class: 'banner-text' },
      title ? h('div', { class: 'banner-title' }, title) : null,
      text ? h('div', { class: 'banner-desc' }, text) : null,
      children || null),
    actions || null);
}
