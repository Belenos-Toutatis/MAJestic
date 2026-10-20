// Rows and texts of the "Windows Update" group (updates waiting in Windows
// Update, installed by Windows itself: MAJestic only shows them).

import { h } from '../dom.js';
import { t, fmt } from '../i18n.js';
import { openUrl } from '../actions.js';
import { badge, iconButton } from './widgets.js';
import { icon } from '../icons.js';

const KINDS = ['security', 'critical', 'feature', 'update', 'driver', 'definition'];
const KIND_TONES = { security: 'critical', critical: 'recommended', feature: 'major', update: 'normal', driver: 'neutral', definition: 'normal' };
const KIND_ICONS = { security: 'shield', critical: 'warning', feature: 'rocket', update: 'updates', driver: 'chip', definition: 'shield' };
const SEVERITIES = ['Critical', 'Important', 'Moderate', 'Low'];

export const kindOfWu = (u) => (KINDS.includes(u?.kind) ? u.kind : 'update');

/** Group tone: the most pressing pending update decides. */
export function wuTone(updates) {
  const kinds = new Set(updates.filter((u) => !u.optional).map(kindOfWu));
  if (kinds.has('security')) return 'critical';
  if (kinds.has('critical')) return 'recommended';
  if (kinds.has('feature')) return 'major';
  return updates.length ? 'normal' : 'success';
}

const focusable = (el) => { el.tabIndex = 0; return el; };

export function wuRow(u) {
  const kind = kindOfWu(u);
  const severity = SEVERITIES.includes(u.severity) ? u.severity : null;
  const meta = [
    ...(Array.isArray(u.kb) ? u.kb.slice(0, 3) : []),
    u.date ? t('wu.published', { date: fmt.date(u.date) }) : null,
    u.size ? fmt.bytes(u.size) : null,
  ].filter(Boolean).join(' · ');
  return h('li', { class: ['wu-row', `tone-${KIND_TONES[kind]}`] },
    h('span', { class: 'wu-ic', 'aria-hidden': 'true' }, icon(KIND_ICONS[kind])),
    h('div', { class: 'wu-main' },
      h('div', { class: 'wu-title', tip: [u.title, t(`wu.kind.${kind}Tip`)], tabindex: 0 }, u.title),
      meta ? h('div', { class: 'caption subtle' }, meta) : null),
    h('div', { class: 'row-flex wrap wu-badges' },
      focusable(badge({ text: t(`wu.kind.${kind}`), tone: KIND_TONES[kind] === 'neutral' ? 'uncertain' : KIND_TONES[kind], tip: [t(`wu.kind.${kind}`), t(`wu.kind.${kind}Tip`)] })),
      severity ? focusable(badge({ text: t(`wu.severity.${severity}`), outline: true, tip: [t('wu.severityTitle'), t('wu.severityTip', { level: t(`wu.severity.${severity}`) })] })) : null,
      u.optional ? focusable(badge({ text: t('wu.optional'), outline: true, tip: [t('wu.optional'), t('wu.optionalTip')] })) : null,
      u.downloaded ? focusable(badge({ text: t('wu.downloaded'), tone: 'success', icon: 'download', tip: [t('wu.downloaded'), t('wu.downloadedTip')] })) : null,
      u.reboot ? focusable(badge({ text: t('wu.reboot'), tone: 'gold', icon: 'restart', tip: [t('wu.reboot'), t('wu.rebootTip')] })) : null),
    u.supportUrl ? iconButton({ icon: 'external', label: t('wu.moreInfo'), tip: [t('wu.moreInfo'), u.supportUrl], onClick: () => openUrl(u.supportUrl) }) : null);
}
