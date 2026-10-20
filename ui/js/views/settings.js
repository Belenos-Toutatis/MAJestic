// Settings (get_settings / save_settings), applied immediately like Windows Settings.

import { h, clear } from '../dom.js';
import { icon } from '../icons.js';
import { t } from '../i18n.js';
import { state, bus, settings, itemByKey } from '../state.js';
import { saveSettings, unignore, openLogs } from '../actions.js';
import { toggle, segmented, select, button, infoDot, iconButton } from '../components/widgets.js';
import { toast } from '../components/toast.js';

export function mountSettings(container) {
  const view = h('div', { class: 'view settings-view' });
  const body = h('div', { class: 'page-body settings-body' });
  view.append(h('div', { class: 'view-scroll' },
    h('div', { class: 'page-head' },
      h('div', { class: 'grow' },
        h('h1', null, t('settings.title')),
        h('p', { class: 'muted' }, t('settings.subtitle')))),
    body));
  container.append(view);

  function row({ ic, title, desc, control, info, id, disabled }) {
    return h('div', { class: ['set-row', disabled ? 'is-disabled' : null] },
      h('span', { class: 'set-ic' }, icon(ic)),
      h('div', { class: 'grow set-text' },
        h(id ? 'label' : 'div', { class: 'set-title', for: id }, title),
        desc ? h('p', { class: 'caption muted' }, desc) : null),
      info ? infoDot(info[0], info[1]) : null,
      h('div', { class: 'set-control' }, control));
  }

  function group(title, ...rows) {
    return h('section', { class: 'set-group' }, h('h2', { class: 'set-group-title' }, title), h('div', { class: 'set-card' }, rows));
  }

  function sw(key, label, tipText) {
    return toggle({
      checked: !!settings()[key], label, id: `set-${key}`,
      tip: [label, tipText],
      onChange: (v) => saveSettings({ [key]: v }),
    });
  }

  function render() {
    clear(body);
    const s = settings();

    // Appearance
    body.append(group(t('settings.appearance'),
      row({
        ic: 'language', title: t('settings.language'), desc: t('settings.languageDesc'), id: 'set-language',
        control: select({
          id: 'set-language', label: t('settings.language'),
          value: s.language || 'auto',
          tip: [t('settings.language'), t('settings.languageTip')],
          options: [
            { value: 'auto', label: t('settings.languageAuto') },
            { value: 'fr', label: 'Français' },
            { value: 'en', label: 'English' },
          ],
          onChange: (v) => saveSettings({ language: v === 'auto' ? null : v }),
        }),
      }),
      row({
        ic: 'contrast', title: t('settings.theme'), desc: t('settings.themeDesc'),
        control: segmented({
          label: t('settings.theme'),
          idPrefix: 'set-theme',
          value: ['system', 'light', 'dark'].includes(s.theme) ? s.theme : 'system',
          options: [
            { value: 'system', label: t('settings.themeSystem'), icon: 'contrast', tip: [t('settings.themeSystem'), t('settings.themeSystemTip')] },
            { value: 'light', label: t('settings.themeLight'), icon: 'sun', tip: [t('settings.themeLight'), t('settings.themeLightTip')] },
            { value: 'dark', label: t('settings.themeDark'), icon: 'moon', tip: [t('settings.themeDark'), t('settings.themeDarkTip')] },
          ],
          onChange: (v) => saveSettings({ theme: v }, { quiet: true }),
        }),
      })));

    // Scan
    body.append(group(t('settings.scan'),
      row({ ic: 'search', id: 'set-scanOnStart', title: t('settings.scanOnStart'), desc: t('settings.scanOnStartDesc'), control: sw('scanOnStart', t('settings.scanOnStart'), t('settings.scanOnStartTip')) }),
      row({ ic: 'globe', id: 'set-onlineIntel', title: t('settings.onlineIntel'), desc: t('settings.onlineIntelDesc'), info: [t('settings.onlineIntel'), t('settings.onlineIntelInfo')], control: sw('onlineIntel', t('settings.onlineIntel'), t('settings.onlineIntelTip')) }),
      row({ ic: 'search', id: 'set-lookupUnmanaged', title: t('settings.lookupUnmanaged'), desc: t('settings.lookupUnmanagedDesc'), info: [t('settings.lookupUnmanaged'), t('settings.lookupUnmanagedInfo')], control: sw('lookupUnmanaged', t('settings.lookupUnmanaged'), t('settings.lookupUnmanagedTip')) }),
      row({ ic: 'help', id: 'set-includeUnknown', title: t('settings.includeUnknown'), desc: t('settings.includeUnknownDesc'), info: [t('groups.uncertain.title'), t('groups.uncertain.info')], control: sw('includeUnknown', t('settings.includeUnknown'), t('settings.includeUnknownTip')) }),
      row({ ic: 'choco', id: 'set-useChocolatey', title: t('settings.useChocolatey'), desc: t('settings.useChocolateyDesc'), info: [t('sources.chocolatey.label'), t('sources.chocolatey.tip')], control: sw('useChocolatey', t('settings.useChocolatey'), t('settings.useChocolateyTip')) }),
      row({ ic: 'scoop', id: 'set-useScoop', title: t('settings.useScoop'), desc: t('settings.useScoopDesc'), info: [t('sources.scoop.label'), t('sources.scoop.tip')], control: sw('useScoop', t('settings.useScoop'), t('settings.useScoopTip')) }),
      row({ ic: 'shield', id: 'set-windowsUpdate', title: t('settings.windowsUpdate'), desc: t('settings.windowsUpdateDesc'), info: [t('wu.title'), t('wu.info')], control: sw('windowsUpdate', t('settings.windowsUpdate'), t('settings.windowsUpdateTip')) })));

    // Installation
    const policy = ['ask', 'always', 'never'].includes(s.closeApps) ? s.closeApps : 'ask';
    body.append(group(t('settings.install'),
      row({
        ic: 'activity', id: 'set-closeApps', title: t('settings.closeApps'), desc: t(`settings.closeAppsDesc.${policy}`),
        info: [t('settings.closeApps'), t('settings.closeAppsInfo')],
        control: select({
          id: 'set-closeApps', label: t('settings.closeApps'), value: policy,
          tip: [t('settings.closeApps'), t('settings.closeAppsTip')],
          options: ['ask', 'always', 'never'].map((v) => ({ value: v, label: t(`settings.closeAppsOpt.${v}`) })),
          onChange: (v) => saveSettings({ closeApps: v }),
        }),
      }),
      row({
        ic: 'shield', title: t('settings.admin'), desc: adminText(),
        info: [t('settings.admin'), t('settings.adminInfo')],
        control: h('span', { class: 'badge outline', tabindex: 0, tip: [t('settings.admin'), t('settings.adminInfo')] }, state.info?.elevated ? t('settings.adminElevated') : t('settings.adminStandard')),
      })));

    // Schedule
    const sched = s.schedule && typeof s.schedule === 'object' ? s.schedule : null;
    const freq = sched ? (['daily', 'weekly', 'monthly'].includes(sched.frequency) ? sched.frequency : 'weekly') : 'off';
    const timeInput = h('input', {
      type: 'time', class: 'input', id: 'set-time', value: sched?.time || '12:00', disabled: !sched,
      'aria-label': t('settings.scheduleTime'), tip: [t('settings.scheduleTime'), t('settings.scheduleTimeTip')],
    });
    timeInput.addEventListener('change', () => {
      if (!/^\d{2}:\d{2}$/.test(timeInput.value)) return;
      saveSettings({ schedule: { ...currentSchedule(), time: timeInput.value } });
    });
    body.append(group(t('settings.schedule'),
      row({
        ic: 'calendar', id: 'set-frequency', title: t('settings.scheduleFrequency'), desc: t('settings.scheduleDesc'),
        info: [t('settings.schedule'), t('settings.scheduleInfo')],
        control: select({
          id: 'set-frequency', label: t('settings.scheduleFrequency'), value: freq,
          tip: [t('settings.scheduleFrequency'), t('settings.scheduleFrequencyTip')],
          options: ['off', 'daily', 'weekly', 'monthly'].map((v) => ({ value: v, label: t(`settings.freq.${v}`) })),
          onChange: (v) => saveSettings({ schedule: v === 'off' ? null : { ...currentSchedule(), frequency: v } }),
        }),
      }),
      row({ ic: 'clock', id: 'set-time', title: t('settings.scheduleTime'), desc: t('settings.scheduleTimeDesc'), control: timeInput, disabled: !sched }),
      row({
        ic: 'bell', id: 'set-notify', title: t('settings.scheduleNotify'), desc: t('settings.scheduleNotifyDesc'), disabled: !sched,
        control: (() => {
          const el = select({
            id: 'set-notify', label: t('settings.scheduleNotify'), value: sched?.notify === 'all' ? 'all' : 'important',
            tip: [t('settings.scheduleNotify'), t('settings.scheduleNotifyTip')],
            options: [{ value: 'important', label: t('settings.notify.important') }, { value: 'all', label: t('settings.notify.all') }],
            onChange: (v) => saveSettings({ schedule: { ...currentSchedule(), notify: v } }),
          });
          el.disabled = !sched;
          return el;
        })(),
      })));

    // Ignored
    const rules = Array.isArray(s.ignored) ? s.ignored.filter((r) => r && r.key) : [];
    const ignoredCard = rules.length
      ? rules.map((r) => h('div', { class: 'set-row ignored-row' },
        h('span', { class: 'set-ic' }, icon('eye-off')),
        h('div', { class: 'grow set-text' },
          h('div', { class: 'set-title' }, r.name || itemByKey(r.key)?.name || r.key),
          h('p', { class: 'caption muted' }, r.version ? t('settings.ignoredVersion', { version: r.version }) : t('settings.ignoredAll'))),
        button({
          label: t('settings.ignoredRemove'), icon: 'close', size: 'small', variant: 'subtle',
          tip: [t('settings.ignoredRemove'), t('settings.ignoredRemoveTip', { name: r.name || r.key })],
          onClick: () => unignoreRule(r),
        })))
      : [h('div', { class: 'set-empty' }, icon('eye'), h('div', null, h('div', { class: 'strong' }, t('settings.ignoredEmpty')), h('p', { class: 'caption muted' }, t('settings.ignoredEmptyText'))))];
    body.append(h('section', { class: 'set-group' },
      h('h2', { class: 'set-group-title' }, t('settings.ignored'), infoDot(t('settings.ignored'), t('settings.ignoredInfo'))),
      h('div', { class: 'set-card' }, ignoredCard)));

    // About
    const env = state.result?.environment || {};
    const info = state.info || {};
    const dataDir = info.dataDir || '—';
    body.append(group(t('settings.about'),
      h('div', { class: 'set-row about-row' },
        h('img', { src: 'assets/logo.svg', alt: '', width: 40, height: 40 }),
        h('div', { class: 'grow set-text' },
          h('div', { class: 'set-title' }, t('app.name'), ' ', h('span', { class: 'num muted' }, info.appVersion || env.appVersion || '?')),
          h('p', { class: 'caption muted' }, t('app.tagline'))),
        envBadges(env)),
      row({
        ic: 'folder', title: t('settings.dataDir'), desc: dataDir,
        info: [t('settings.dataDir'), t('settings.dataDirInfo')],
        control: dataDir !== '—' ? iconButton({ icon: 'duplicate', label: t('settings.copyPath'), tip: [t('settings.copyPath'), t('settings.copyPathTip')], onClick: () => copy(dataDir) }) : null,
      }),
      row({
        ic: 'notes', title: t('settings.logs'), desc: t('settings.logsDesc'),
        control: button({ label: t('common.openLogs'), icon: 'folder', size: 'small', tip: [t('common.openLogs'), t('common.openLogsTip')], onClick: openLogs }),
      })));
  }

  function adminText() {
    const info = state.info || {};
    if (info.elevated) return t('settings.adminElevatedDesc');
    if (info.adminMember) return t('settings.adminMemberDesc');
    return t('settings.adminStandardDesc');
  }

  function envBadges(env) {
    const parts = [];
    if (env.winget) parts.push(h('span', { class: 'badge outline', tabindex: 0, tip: [t('sources.winget.label'), t('settings.envWinget', { version: env.winget })] }, `winget ${env.winget}`));
    if (env.chocolatey) parts.push(h('span', { class: 'badge outline', tabindex: 0, tip: [t('sources.chocolatey.label'), t('settings.envChoco', { version: env.chocolatey })] }, `Chocolatey ${env.chocolatey}`));
    if (env.scoop) parts.push(h('span', { class: 'badge outline', tabindex: 0, tip: [t('sources.scoop.label'), t('settings.envScoop')] }, 'Scoop'));
    if (env.os) parts.push(h('span', { class: 'badge outline', tabindex: 0, tip: [t('settings.envOsTitle'), env.os] }, env.os));
    return parts.length ? h('div', { class: 'row-flex wrap end env-badges' }, parts) : null;
  }

  function currentSchedule() {
    const sc = settings().schedule;
    return {
      frequency: ['daily', 'weekly', 'monthly'].includes(sc?.frequency) ? sc.frequency : 'weekly',
      time: /^\d{2}:\d{2}$/.test(sc?.time || '') ? sc.time : '12:00',
      notify: sc?.notify === 'all' ? 'all' : 'important',
    };
  }

  async function unignoreRule(rule) {
    const rest = (settings().ignored || []).filter((r) => !(r.key === rule.key && (r.version ?? null) === (rule.version ?? null)));
    if (rest.some((r) => r.key === rule.key)) {
      await saveSettings({ ignored: rest });
    } else {
      await unignore(rule.key);
    }
  }

  async function copy(text) {
    try { await navigator.clipboard.writeText(text); toast(t('settings.copied'), { tone: 'success', duration: 1800 }); }
    catch { toast(t('settings.copyFailed'), { tone: 'danger' }); }
  }

  render();
  const off = bus.on('settings', () => {
    const focusedId = document.activeElement?.id;
    const scroller = view.querySelector('.view-scroll');
    const top = scroller.scrollTop;
    render();
    scroller.scrollTop = top;
    if (focusedId) document.getElementById(focusedId)?.focus({ preventScroll: true });
  });
  return () => off();
}
