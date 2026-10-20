// Settings (get_settings / save_settings), applied immediately like Windows Settings.

import { h, clear } from '../dom.js';
import { icon } from '../icons.js';
import { t, fmt } from '../i18n.js';
import { invoke, errorText } from '../api.js';
import { state, bus, settings, itemByKey, runActive } from '../state.js';
import { saveSettings, unignore, openLogs } from '../actions.js';
import { toggle, segmented, select, button, infoDot, iconButton, badge, banner, spinner } from '../components/widgets.js';
import { toast } from '../components/toast.js';
import { checkAppUpdate, appUpdateChecking, openAppUpdate } from '../appupdate.js';

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

  let schedHost = null;
  let schedStatus = null;   // schedule_status
  let schedDraft = null;    // settings being edited, applied by the buttons only
  let schedBusy = false;
  let appUpdateRowEl = null;
  let leftovers = null;     // leftover_downloads (null while measuring)
  let cleaning = false;
  let leftoverRowEl = null;

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
      }),
      (leftoverRowEl = leftoverRow())));

    // Schedule: summary of what is set, then an explicit button.
    schedHost = h('section', { class: 'set-group sched-group' });
    body.append(schedHost);
    renderSchedule();

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
      (appUpdateRowEl = appUpdateRow()),
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

  // ---------------------------------------------------------------- leftover installers

  function leftoverRow() {
    const bytes = leftovers?.bytes || 0;
    const busy = cleaning || runActive();
    const desc = leftovers === null ? t('settings.leftovers.loading')
      : bytes ? t('settings.leftovers.found', { size: fmt.bytes(bytes) })
        : t('settings.leftovers.none');
    return row({
      ic: 'disk', title: t('settings.leftovers.title'), desc,
      info: [t('settings.leftovers.title'), t('settings.leftovers.info')],
      control: h('div', { class: 'row-flex' },
        cleaning ? spinner(true) : null,
        button({
          label: bytes ? t('settings.leftovers.clean', { size: fmt.bytes(bytes) }) : t('settings.leftovers.cleanNone'),
          icon: 'trash', size: 'small', disabled: !bytes || busy,
          tip: [t('settings.leftovers.title'), runActive() ? t('settings.leftovers.busy') : bytes ? t('settings.leftovers.cleanTip') : t('settings.leftovers.noneTip')],
          onClick: cleanLeftovers,
        })),
    });
  }

  function refreshLeftoverRow() {
    if (!leftoverRowEl?.isConnected) return;
    const hadFocus = leftoverRowEl.contains(document.activeElement);
    const next = leftoverRow();
    leftoverRowEl.replaceWith(next);
    leftoverRowEl = next;
    if (hadFocus) (next.querySelector('.set-control button:not([disabled])') || next.querySelector('.info-dot, button'))?.focus({ preventScroll: true });
  }

  async function loadLeftovers() {
    try { leftovers = await invoke('leftover_downloads'); } catch (e) { console.warn('[leftovers]', e); leftovers = { bytes: 0, entries: 0 }; }
    refreshLeftoverRow();
  }

  async function cleanLeftovers() {
    if (cleaning) return;
    cleaning = true;
    refreshLeftoverRow();
    try {
      const r = await invoke('clean_downloads');
      if (r.failed) toast(t('settings.leftovers.partly', { count: r.failed, size: fmt.bytes(r.freed) }), { tone: 'warning', duration: 7000 });
      else toast(t('settings.leftovers.done', { size: fmt.bytes(r.freed) }), { tone: 'success', duration: 5000 });
    } catch (e) {
      const text = errorText(e);
      toast(text === 'busy' ? t('settings.leftovers.busy') : t('settings.leftovers.failed', { error: text }), { tone: 'danger', duration: 7000 });
    }
    cleaning = false;
    await loadLeftovers();
  }

  // ---------------------------------------------------------------- MAJestic's own updates

  function appUpdateStatus() {
    const check = state.appUpdateCheck;
    if (!settings().onlineIntel) return t('settings.appUpdate.offline');
    if (appUpdateChecking()) return t('settings.appUpdate.checking');
    if (state.appUpdate) return t('settings.appUpdate.available', { version: state.appUpdate.version, current: state.info?.appVersion || '?' });
    if (check?.error) return t('settings.appUpdate.failed', { when: fmt.relative(check.at) });
    if (check?.at) return t('settings.appUpdate.upToDate', { when: fmt.relative(check.at) });
    return t('settings.appUpdate.never');
  }

  function appUpdateRow() {
    const online = !!settings().onlineIntel;
    const busy = appUpdateChecking();
    const control = state.appUpdate && !busy
      ? button({
        label: t('settings.appUpdate.install'), icon: 'download', variant: 'royal', size: 'small',
        tip: [t('settings.appUpdate.install'), t('settings.appUpdate.installTip', { version: state.appUpdate.version })],
        onClick: openAppUpdate,
      })
      : h('div', { class: 'row-flex' },
        busy ? spinner(true) : null,
        button({
          label: t('settings.appUpdate.check'), icon: 'refresh', size: 'small', disabled: !online,
          'aria-busy': busy ? 'true' : null,
          tip: [t('settings.appUpdate.check'), online ? t('settings.appUpdate.checkTip') : t('settings.appUpdate.offline')],
          onClick: manualAppUpdateCheck,
        }));
    return row({
      ic: 'sparkle', title: t('settings.appUpdate.title'), desc: appUpdateStatus(),
      info: [t('settings.appUpdate.title'), t('settings.appUpdate.info')], control,
    });
  }

  function refreshAppUpdateRow() {
    if (!appUpdateRowEl?.isConnected) return;
    const hadFocus = appUpdateRowEl.contains(document.activeElement);
    const next = appUpdateRow();
    appUpdateRowEl.replaceWith(next);
    appUpdateRowEl = next;
    if (hadFocus) next.querySelector('.set-control button:not([disabled])')?.focus({ preventScroll: true });
  }

  async function manualAppUpdateCheck() {
    if (appUpdateChecking()) return;
    const release = await checkAppUpdate({ manual: true });
    if (release) openAppUpdate();
    else if (state.appUpdateCheck?.error) toast(t('settings.appUpdate.failedToast'), { tone: 'danger', duration: 6000 });
    else toast(t('settings.appUpdate.upToDateToast', { version: state.info?.appVersion || '' }), { tone: 'success', duration: 4000 });
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

  // ---------------------------------------------------------------- scheduled check

  function defaultDraft() {
    const now = new Date();
    return { frequency: 'weekly', weekday: ((now.getDay() + 6) % 7) + 1, day: Math.min(now.getDate(), 28), time: '12:00', notify: 'important' };
  }

  function draftFrom(sc) {
    const d = defaultDraft();
    if (!sc || typeof sc !== 'object') return d;
    const wd = Number(sc.weekday);
    const day = Number(sc.day);
    return {
      frequency: ['daily', 'weekly', 'monthly'].includes(sc.frequency) ? sc.frequency : 'weekly',
      weekday: wd >= 1 && wd <= 7 ? wd : d.weekday,
      day: day >= 1 && day <= 28 ? day : d.day,
      time: /^\d{2}:\d{2}$/.test(sc.time || '') ? sc.time : '12:00',
      notify: sc.notify === 'all' ? 'all' : 'important',
    };
  }

  const sameDraft = (a, b) => ['frequency', 'weekday', 'day', 'time', 'notify'].every((k) => a[k] === b[k]);
  const dayLabel = (d) => (d === 1 ? t('settings.sched.first') : String(d));
  const summaryText = (d) => t(`settings.sched.summary.${d.frequency}`, {
    time: d.time, weekday: t(`settings.sched.weekday.${d.weekday}`), day: dayLabel(d.day),
  });

  async function loadSchedule() {
    try {
      schedStatus = await invoke('schedule_status');
    } catch (e) {
      schedStatus = { error: errorText(e), schedule: settings().schedule || null, task: 'unreadable' };
    }
    if (!schedDraft) schedDraft = draftFrom(schedStatus.schedule);
    renderSchedule();
  }

  function lastCheckText(lc) {
    if (!lc || !lc.at) return t('settings.sched.lastCheckNever');
    const parts = [t('settings.sched.result', { count: Number(lc.updates) || 0 })];
    if (lc.important) parts.push(t('settings.sched.important', { count: lc.important }));
    if (lc.windowsSecurity) parts.push(t('settings.sched.windowsSecurity', { count: lc.windowsSecurity }));
    return `${fmt.dateTime(lc.at)} — ${parts.join(', ')}${lc.notified ? ` · ${t('settings.sched.notified')}` : ''}`;
  }

  function fact(ic, label, value, tip, mono) {
    return h('div', { class: 'sched-fact' },
      h('span', { class: 'sched-fact-ic' }, icon(ic)),
      h('dt', null, label),
      h('dd', { class: mono ? 'mono sched-path' : null, tip: [label, tip], tabindex: 0 }, value));
  }

  function renderSchedule() {
    if (!schedHost) return;
    clear(schedHost);
    schedHost.append(h('h2', { class: 'set-group-title' }, t('settings.schedule'), infoDot(t('settings.schedule'), t('settings.scheduleInfo'))));
    const card = h('div', { class: 'set-card' });
    schedHost.append(card);
    if (!schedStatus) {
      card.append(h('div', { class: 'set-row' }, spinner(true), h('span', { class: 'muted' }, t('settings.sched.loading'))));
      return;
    }
    const st = schedStatus;
    const on = !!st.schedule;
    const current = draftFrom(st.schedule);
    const broken = on && (st.task === 'missing' || st.task === 'other');
    const mode = !on ? 'off' : broken ? 'repair' : 'on';
    const tones = { on: 'success', repair: 'warning', off: 'uncertain' };
    const icons = { on: 'success', repair: 'warning', off: 'minus' };

    // Summary of what is set.
    card.append(h('div', { class: ['sched-recap', `is-${mode}`] },
      h('div', { class: 'sched-head' },
        badge({ text: t(`settings.sched.status.${mode}`), tone: tones[mode], icon: icons[mode], tip: [t(`settings.sched.status.${mode}`), t(`settings.sched.status.${mode}Tip`)], cls: 'sched-badge' }),
        h('div', { class: 'strong grow' }, on ? summaryText(current) : t('settings.sched.offText'))),
      h('dl', { class: 'sched-facts' },
        on && st.nextRun ? fact('calendar', t('settings.sched.nextRun'), fmt.dateTime(st.nextRun), t('settings.sched.nextRunTip')) : null,
        fact('history', t('settings.sched.lastCheck'), lastCheckText(st.lastCheck), t('settings.sched.lastCheckTip')),
        on ? fact('bell', t('settings.sched.notifyLabel'), t(`settings.notify.${current.notify}`), t('settings.scheduleNotifyTip')) : null,
        fact('file', t('settings.sched.program'), st.exe || '—', t('settings.sched.programTip'), true))));

    if (on && st.task === 'missing') card.append(h('div', { class: 'sched-note' }, banner({ tone: 'warning', text: t('settings.sched.missing') })));
    if (on && st.task === 'other') card.append(h('div', { class: 'sched-note' }, banner({ tone: 'warning', text: t('settings.sched.other', { path: st.taskCommand || '?' }) })));
    if (st.locationWarning) card.append(h('div', { class: 'sched-note' }, banner({ tone: st.locationWarning === 'temp' ? 'warning' : 'info', icon: 'folder', text: t(`settings.sched.location.${st.locationWarning}`) })));
    if (st.error) card.append(h('div', { class: 'sched-note' }, banner({ tone: 'danger', text: t('settings.sched.statusFailed', { error: st.error }) })));

    // The settings to apply.
    const d = schedDraft;
    const update = (patch) => { schedDraft = { ...schedDraft, ...patch }; renderSchedule(); };
    const timeInput = h('input', {
      type: 'time', class: 'input', id: 'set-time', value: d.time,
      'aria-label': t('settings.scheduleTime'), tip: [t('settings.scheduleTime'), t('settings.scheduleTimeTip')],
    });
    timeInput.addEventListener('change', () => { if (/^\d{2}:\d{2}$/.test(timeInput.value)) update({ time: timeInput.value }); });
    card.append(
      row({
        ic: 'calendar', id: 'set-frequency', title: t('settings.scheduleFrequency'), desc: t('settings.scheduleDesc'),
        control: select({
          id: 'set-frequency', label: t('settings.scheduleFrequency'), value: d.frequency,
          tip: [t('settings.scheduleFrequency'), t('settings.scheduleFrequencyTip')],
          options: ['daily', 'weekly', 'monthly'].map((v) => ({ value: v, label: t(`settings.freq.${v}`) })),
          onChange: (v) => update({ frequency: v }),
        }),
      }),
      d.frequency === 'daily' ? null : row({
        ic: 'calendar', id: 'set-day', title: t('settings.sched.day'), desc: t(d.frequency === 'weekly' ? 'settings.sched.dayWeeklyDesc' : 'settings.sched.dayMonthlyDesc'),
        control: select({
          id: 'set-day', label: t('settings.sched.day'), value: String(d.frequency === 'weekly' ? d.weekday : d.day),
          tip: [t('settings.sched.day'), t('settings.sched.dayTip')],
          options: d.frequency === 'weekly'
            ? [1, 2, 3, 4, 5, 6, 7].map((n) => ({ value: String(n), label: t(`settings.sched.weekdayCap.${n}`) }))
            : Array.from({ length: 28 }, (_, i) => ({ value: String(i + 1), label: dayLabel(i + 1) })),
          onChange: (v) => update(d.frequency === 'weekly' ? { weekday: Number(v) } : { day: Number(v) }),
        }),
      }),
      row({ ic: 'clock', id: 'set-time', title: t('settings.scheduleTime'), desc: t('settings.scheduleTimeDesc'), control: timeInput }),
      row({
        ic: 'bell', id: 'set-notify', title: t('settings.scheduleNotify'), desc: t('settings.scheduleNotifyDesc'),
        control: select({
          id: 'set-notify', label: t('settings.scheduleNotify'), value: d.notify,
          tip: [t('settings.scheduleNotify'), t('settings.scheduleNotifyTip')],
          options: [{ value: 'important', label: t('settings.notify.important') }, { value: 'all', label: t('settings.notify.all') }],
          onChange: (v) => update({ notify: v }),
        }),
      }));

    // Explicit actions: the Windows task is only written here.
    const dirty = on && !sameDraft(current, d);
    const actions = [];
    if (!on) {
      actions.push(button({ label: t('settings.sched.activate'), icon: 'calendar', variant: 'primary', disabled: schedBusy, tip: [t('settings.sched.activate'), t('settings.sched.activateTip')], onClick: () => applySchedule(d) }));
    } else {
      if (broken) actions.push(button({ label: t('settings.sched.repair'), icon: 'wrench', variant: 'primary', disabled: schedBusy, tip: [t('settings.sched.repair'), t('settings.sched.repairTip')], onClick: () => applySchedule(d) }));
      else actions.push(button({ label: t('settings.sched.save'), icon: 'check', variant: 'primary', disabled: schedBusy || !dirty, tip: [t('settings.sched.save'), dirty ? t('settings.sched.saveTip') : t('settings.sched.saveNothing')], onClick: () => applySchedule(d) }));
      actions.push(button({ label: t('settings.sched.disable'), icon: 'close', variant: 'subtle', disabled: schedBusy, tip: [t('settings.sched.disable'), t('settings.sched.disableTip')], onClick: () => applySchedule(null) }));
    }
    card.append(h('div', { class: 'sched-actions' },
      h('p', { class: 'caption muted grow' }, icon('shield'), ' ', t('settings.sched.defender')),
      schedBusy ? spinner(true) : null,
      actions));
  }

  async function applySchedule(draft) {
    const wasOn = !!schedStatus?.schedule;
    schedBusy = true;
    renderSchedule();
    try {
      schedStatus = await invoke('set_schedule', { schedule: draft ? { ...draft } : null });
      // Keep the shared settings in sync: a later save must not send a stale schedule.
      state.settings = { ...state.settings, schedule: schedStatus.schedule || null };
      schedDraft = draftFrom(schedStatus.schedule);
      const when = schedStatus.nextRun ? fmt.dateTime(schedStatus.nextRun) : '—';
      toast(draft ? t(wasOn ? 'settings.sched.saved' : 'settings.sched.activated', { when }) : t('settings.sched.disabled'), { tone: 'success', duration: 6000 });
    } catch (e) {
      toast(t('settings.sched.failed', { error: errorText(e) }), { tone: 'danger', duration: 9000 });
    }
    schedBusy = false;
    renderSchedule();
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
  loadSchedule();
  loadLeftovers();
  const offs = [
    bus.on('settings', () => {
      const focusedId = document.activeElement?.id;
      const scroller = view.querySelector('.view-scroll');
      const top = scroller.scrollTop;
      render();
      scroller.scrollTop = top;
      if (focusedId) document.getElementById(focusedId)?.focus({ preventScroll: true });
    }),
    bus.on('app-update', refreshAppUpdateRow),
    bus.on('app-update-check', refreshAppUpdateRow),
  ];
  return () => offs.forEach((off) => off());
}
