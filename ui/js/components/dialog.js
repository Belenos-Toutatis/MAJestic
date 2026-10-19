// Modal dialogs built on <dialog>: focus is trapped by the browser, Esc closes.

import { h, animateOut } from '../dom.js';
import { icon } from '../icons.js';
import { button } from './widgets.js';
import { hide as hideTooltip } from './tooltip.js';
import { t } from '../i18n.js';

/**
 * openDialog({ title, icon, body: Node|Node[], footStart: Node, width,
 *              actions: [{ label, value, variant, icon, tip, autofocus, disabled }] })
 * Resolves with the chosen action's value, or null when dismissed (Esc / close).
 * `api.close(value)` and `api.dialog` are passed to `build` when body is a function.
 */
export function openDialog(opts) {
  return new Promise((resolve) => {
    hideTooltip(true);
    const previousFocus = document.activeElement;
    let settled = false;
    const titleId = `dlg-title-${Math.random().toString(36).slice(2, 8)}`;

    const dlg = h('dialog', { class: 'dialog', 'aria-labelledby': titleId });
    if (opts.width) dlg.style.width = `min(${opts.width}px, calc(100vw - 48px))`;

    const close = (value) => {
      if (settled) return;
      settled = true;
      hideTooltip(true);
      animateOut(dlg, () => {
        try { dlg.close(); } catch { /* already closed */ }
        dlg.remove();
        if (previousFocus && previousFocus.isConnected) previousFocus.focus?.();
        resolve(value ?? null);
      }, 200);
    };

    const api = { close, dialog: dlg };
    const body = typeof opts.body === 'function' ? opts.body(api) : opts.body;
    const actionButtons = (opts.actions || []).map((a) => {
      const b = button({
        label: a.label, icon: a.icon, variant: a.variant, disabled: a.disabled,
        tip: a.tip || [a.label, a.value === null || a.value === false ? t('common.dismissTip') : null],
        onClick: () => (a.onClick ? a.onClick(api) : close(a.value)),
      });
      if (a.autofocus) b.autofocus = true;
      if (a.ref) a.ref(b);
      return b;
    });

    dlg.append(
      h('div', { class: 'dialog-body' },
        h('h2', { id: titleId }, opts.icon ? icon(opts.icon) : null, opts.title),
        body),
      h('div', { class: 'dialog-foot' },
        h('div', { class: 'grow' }, opts.footStart || null),
        actionButtons),
    );

    dlg.addEventListener('cancel', (e) => { e.preventDefault(); close(null); });
    dlg.addEventListener('click', (e) => {
      // click on the backdrop (outside the dialog box)
      if (e.target !== dlg) return;
      const r = dlg.getBoundingClientRect();
      const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      if (!inside && opts.dismissable !== false) close(null);
    });

    document.body.appendChild(dlg);
    dlg.showModal();
    const auto = dlg.querySelector('[autofocus]') || actionButtons[actionButtons.length - 1];
    auto?.focus();
  });
}

/** Simple confirmation. Resolves true/false. */
export async function confirmDialog({ title, text, confirmLabel, cancelLabel, danger, icon: ic }) {
  const v = await openDialog({
    title,
    icon: ic || (danger ? 'warning' : 'info'),
    body: h('p', null, text),
    actions: [
      { label: cancelLabel, value: false },
      { label: confirmLabel, value: true, variant: danger ? 'primary' : 'primary', autofocus: true },
    ],
  });
  return v === true;
}
