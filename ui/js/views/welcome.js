// First-run welcome card (three points), dismissal remembered in localStorage.

import { h, animateOut } from '../dom.js';
import { icon } from '../icons.js';
import { t } from '../i18n.js';
import { local } from '../state.js';
import { iconButton } from '../components/widgets.js';

export function welcomeCard() {
  if (local.get('welcomeDismissed', false)) return null;
  const card = h('section', { class: 'welcome', 'aria-labelledby': 'welcome-title' });
  const close = iconButton({
    icon: 'close',
    label: t('welcome.dismiss'),
    tip: [t('welcome.dismiss'), t('welcome.dismissTip')],
    onClick: () => {
      local.set('welcomeDismissed', true);
      animateOut(card, () => card.remove(), 260);
    },
  });
  card.append(
    h('div', { class: 'welcome-head' },
      h('img', { src: 'assets/logo.svg', alt: '', width: 28, height: 28 }),
      h('h2', { id: 'welcome-title' }, t('welcome.title')),
      close),
    h('ul', { class: 'welcome-points' },
      h('li', null, icon('search'), h('span', null, t('welcome.p1'))),
      h('li', null, icon('shield'), h('span', null, t('welcome.p2'))),
      h('li', null, icon('help'), h('span', null, t('welcome.p3')))),
  );
  return card;
}
