// Inline line icons (24×24, 1.6 px strokes, Fluent-like). Drawn for MAJestic.

function gearPath() {
  const cx = 12, cy = 12, teeth = 8, rOut = 9.4, rIn = 7.2;
  const pts = [];
  for (let i = 0; i < teeth; i++) {
    const a = (i * 360) / teeth;
    for (const [da, r] of [[-22.5, rIn], [-13, rIn], [-8, rOut], [8, rOut], [13, rIn]]) {
      const rad = ((a + da) * Math.PI) / 180;
      pts.push(`${(cx + r * Math.cos(rad)).toFixed(2)} ${(cy + r * Math.sin(rad)).toFixed(2)}`);
    }
  }
  return `<path d="M${pts.join('L')}Z"/><circle cx="12" cy="12" r="3"/>`;
}

const DOT = (cx, cy, r = 0.95) => `<circle cx="${cx}" cy="${cy}" r="${r}" fill="currentColor" stroke="none"/>`;

const PATHS = {
  updates: '<path d="M12 16.5V8m0 0L8.5 11.5M12 8l3.5 3.5"/><path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 3.8v4.4h-4.4"/>',
  history: '<path d="M3.6 12a8.4 8.4 0 1 0 2.5-6"/><path d="M3.2 4.2v4.2h4.2"/><path d="M12 7.5V12l3 2"/>',
  settings: gearPath(),
  search: '<circle cx="10.5" cy="10.5" r="6.2"/><path d="M15.2 15.2L20 20"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  close: '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>',
  'chevron-down': '<path d="M6 9.5l6 6 6-6"/>',
  'chevron-right': '<path d="M9.5 6l6 6-6 6"/>',
  'chevron-left': '<path d="M14.5 6l-6 6 6 6"/>',
  info: `<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5"/>${DOT(12, 7.8)}`,
  help: `<circle cx="12" cy="12" r="9"/><path d="M9.6 9.6a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1.1.9-1.1 1.6v.5"/>${DOT(12, 16.9)}`,
  shield: '<path d="M12 3.2l7 2.6v5.5c0 4.3-2.9 7.9-7 9.5-4.1-1.6-7-5.2-7-9.5V5.8z"/><path d="M12 3.4v17.2M5.1 11.4h13.8"/>',
  download: '<path d="M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5"/><path d="M5 19.5h14"/>',
  stop: '<rect x="6.5" y="6.5" width="11" height="11" rx="2.2"/>',
  refresh: '<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3"/><path d="M19.6 4.4v4.4h-4.4"/>',
  external: '<path d="M14 4h6v6"/><path d="M20 4l-8.5 8.5"/><path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3c2.4 2.5 3.7 5.6 3.7 9s-1.3 6.5-3.7 9c-2.4-2.5-3.7-5.6-3.7-9S9.6 5.5 12 3z"/>',
  store: '<path d="M4 8.5h16l-1.1 11a1.6 1.6 0 0 1-1.6 1.5H6.7a1.6 1.6 0 0 1-1.6-1.5z"/><path d="M8.5 8.5V7a3.5 3.5 0 0 1 7 0v1.5"/>',
  package: '<path d="M12 3l8 4.5v9L12 21l-8-4.5v-9z"/><path d="M4 7.5l8 4.5 8-4.5M12 12v9"/><path d="M8 5.3l8 4.5"/>',
  choco: '<rect x="5.5" y="3.5" width="13" height="17" rx="2"/><path d="M5.5 9.2h13M5.5 14.8h13M12 3.5v17"/>',
  scoop: '<path d="M5 10h14l-1.7 9.2a2 2 0 0 1-2 1.6H8.7a2 2 0 0 1-2-1.6z"/><path d="M8 10a4 4 0 0 1 8 0"/><path d="M12 3.5V6"/>',
  registry: '<rect x="3.5" y="4" width="17" height="16" rx="2.5"/><path d="M3.5 9h17M8 13h8M8 16.5h5"/>',
  warning: `<path d="M10.3 4.7a2 2 0 0 1 3.4 0l7.2 12.6A2 2 0 0 1 19.2 20H4.8a2 2 0 0 1-1.7-2.7z"/><path d="M12 9.5v4.5"/>${DOT(12, 17)}`,
  error: '<circle cx="12" cy="12" r="9"/><path d="M9.2 9.2l5.6 5.6M14.8 9.2l-5.6 5.6"/>',
  success: '<circle cx="12" cy="12" r="9"/><path d="M8 12.3l2.8 2.8 5.4-5.5"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.2 2"/>',
  sparkle: '<path d="M11 3.5l1.7 4.8 4.8 1.7-4.8 1.7L11 16.5l-1.7-4.8L4.5 10l4.8-1.7z"/><path d="M18 14.5l.8 2 2 .8-2 .8-.8 2-.8-2-2-.8 2-.8z"/>',
  crown: '<path d="M4.5 17L3.5 7.5l4.8 4 3.7-6.3 3.7 6.3 4.8-4-1 9.5z"/><path d="M5 20.5h14"/>',
  pin: '<path d="M15 3.5l5.5 5.5-2.8 1.2-3.4 3.4.4 3.9-1.8 1.8-4.3-4.3L4 20l5-5-4.3-4.3 1.8-1.8 3.9.4 3.4-3.4z"/>',
  'eye-off': '<path d="M3.5 3.5l17 17"/><path d="M10.4 6.1c.5-.1 1-.1 1.6-.1 5 0 8.5 4.4 9.5 6-.5.8-1.5 2.2-3 3.4M6.6 7.4C4.7 8.7 3.3 10.6 2.5 12c1 1.6 4.5 6 9.5 6 1.5 0 2.9-.4 4.1-1"/><path d="M9.9 10a3 3 0 0 0 4.1 4.2"/>',
  eye: '<path d="M2.5 12c1-1.6 4.5-6 9.5-6s8.5 4.4 9.5 6c-1 1.6-4.5 6-9.5 6s-8.5-4.4-9.5-6z"/><circle cx="12" cy="12" r="3"/>',
  undo: '<path d="M9 14.5L4 9.5l5-5"/><path d="M4 9.5h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  trash: '<path d="M4 6.5h16"/><path d="M9.5 6.5V5c0-.8.6-1.5 1.4-1.5h2.2c.8 0 1.4.7 1.4 1.5v1.5"/><path d="M6 6.5l1 12.7c.1 1 .9 1.8 2 1.8h6c1.1 0 1.9-.8 2-1.8l1-12.7"/><path d="M10 11v5.5M14 11v5.5"/>',
  folder: '<path d="M3.5 7.5c0-1.1.9-2 2-2h4l2 2.2h7c1.1 0 2 .9 2 2v8.8c0 1.1-.9 2-2 2h-13c-1.1 0-2-.9-2-2z"/>',
  gamepad: `<path d="M7.2 7h9.6a4.8 4.8 0 0 1 4.7 5.8l-.7 3.5a2.4 2.4 0 0 1-4.2 1.1L14.8 15.5H9.2l-1.8 1.9a2.4 2.4 0 0 1-4.2-1.1l-.7-3.5A4.8 4.8 0 0 1 7.2 7z"/><path d="M7.5 9.8v4M5.5 11.8h4"/>${DOT(15.8, 10.6)}${DOT(17.8, 12.8)}`,
  webapp: `<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="M3 9h18"/>${DOT(6, 6.8, 0.7)}${DOT(8.4, 6.8, 0.7)}<path d="M8.5 14.5l2.5 2 4.5-4.5"/>`,
  autoupdate: '<path d="M20 12a8 8 0 0 1-14.2 5"/><path d="M4 12a8 8 0 0 1 14.2-5"/><path d="M18.6 3.4v3.9h-3.9M5.4 20.6v-3.9h3.9"/><path d="M12 9v3.2l2 1.3"/>',
  chip: '<rect x="7" y="7" width="10" height="10" rx="1.6"/><path d="M10 3.5V7M14 3.5V7M10 17v3.5M14 17v3.5M3.5 10H7M3.5 14H7M17 10h3.5M17 14h3.5"/><rect x="10" y="10" width="4" height="4" rx=".6"/>',
  component: '<path d="M12 3.2l8.8 4.6L12 12.4 3.2 7.8z"/><path d="M3.2 12.2l8.8 4.6 8.8-4.6"/><path d="M3.2 16.4l8.8 4.6 8.8-4.6"/>',
  app: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.8"/>',
  activity: '<path d="M3 12h4l2.5-6 5 12 2.5-6h4"/>',
  power: '<path d="M12 3.5v7.5"/><path d="M7.4 6.2a7.5 7.5 0 1 0 9.2 0"/>',
  restart: '<path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3"/><path d="M4.4 3.8v4.4h4.4"/><path d="M12 8.5v4"/>',
  list: `<path d="M8.5 6.5h12M8.5 12h12M8.5 17.5h12"/>${DOT(4.5, 6.5, 1)}${DOT(4.5, 12, 1)}${DOT(4.5, 17.5, 1)}`,
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>',
  moon: '<path d="M19.5 14.6A7.8 7.8 0 0 1 9.4 4.5a7.8 7.8 0 1 0 10.1 10.1z"/>',
  contrast: '<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5a8.5 8.5 0 0 1 0 17z" fill="currentColor"/>',
  calendar: '<rect x="3.5" y="5" width="17" height="15.5" rx="2.5"/><path d="M3.5 10h17M8 3v4M16 3v4"/>',
  bell: '<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>',
  language: '<path d="M4 5.5h9M8.5 3.5v2M6 5.5c.6 2.8 2.6 5.3 5.5 6.8M11 5.5c-.6 3.3-3 6.2-6.5 7.7"/><path d="M12.5 21l4-10 4 10M14 17.5h5"/>',
  link: '<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>',
  notes: '<path d="M6 3.5h8.2L18 7.3V20.5H6z"/><path d="M14 3.5v4h4M9 12h6M9 15.5h6"/>',
  home: '<path d="M4 10.5L12 4l8 6.5V20h-5.5v-6h-5v6H4z"/>',
  support: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3.5"/><path d="M5.6 5.6l3.9 3.9M14.5 14.5l3.9 3.9M18.4 5.6l-3.9 3.9M9.5 14.5l-3.9 3.9"/>',
  tag: `<path d="M3.5 12.3V4.5c0-.6.4-1 1-1h7.8l8.2 8.2-8.8 8.8z"/>${DOT(8, 8, 1.3)}`,
  wrench: '<path d="M14.6 4.2a4.5 4.5 0 0 0-4.1 6.1l-6.2 6.2a2 2 0 0 0 2.9 2.9l6.2-6.2a4.5 4.5 0 0 0 6.1-4.1l-2.7 2.7-2.9-.3-.3-2.9z"/>',
  filter: '<path d="M4 5h16l-6.2 7.4V19l-3.6 1.5v-8.1z"/>',
  hourglass: '<path d="M7 3.5h10M7 20.5h10"/><path d="M8 3.5c0 4.2 8 4.8 8 8.5s-8 4.3-8 8.5M16 3.5c0 4.2-8 4.8-8 8.5s8 4.3 8 8.5"/>',
  'select-all': '<rect x="3.5" y="3.5" width="17" height="17" rx="3.5"/><path d="M8 12.2l2.8 2.8L16 9.8"/>',
  'select-none': '<rect x="3.5" y="3.5" width="17" height="17" rx="3.5"/>',
  'arrow-right': '<path d="M5 12h14m-5-5l5 5-5 5"/>',
  'arrow-up': '<path d="M12 19.5V5m0 0l-6 6m6-6l6 6"/>',
  lock: '<rect x="5" y="10.5" width="14" height="10" rx="2.2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/><path d="M12 14.5v2"/>',
  bug: '<path d="M9 8a3 3 0 0 1 6 0"/><rect x="7" y="8" width="10" height="12" rx="5"/><path d="M12 11v9M3.5 13.5H7M17 13.5h3.5M4.5 8.5l3 2M19.5 8.5l-3 2M4.5 19.5l3-2M19.5 19.5l-3-2"/>',
  duplicate: '<rect x="8" y="8" width="12" height="12" rx="2.2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
  user: '<circle cx="12" cy="8" r="4"/><path d="M4.5 20.5a7.5 7.5 0 0 1 15 0"/>',
  users: '<circle cx="9" cy="8.5" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M15.5 5.2a3.5 3.5 0 0 1 0 6.6M17.5 14.2a6.5 6.5 0 0 1 4 5.8"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  play: '<path d="M7.5 4.8v14.4L19 12z"/>',
  file: '<path d="M6 3.5h8.2L18 7.3V20.5H6z"/><path d="M14 3.5v4h4"/>',
  target: '<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.5"/>' + DOT(12, 12, 1.2),
  wifi: '<path d="M3 9.5a13 13 0 0 1 18 0M6 12.8a8.6 8.6 0 0 1 12 0M9 16a4.3 4.3 0 0 1 6 0"/>' + DOT(12, 19, 1.1),
  'wifi-off': '<path d="M3.5 3.5l17 17M3 9.5a13 13 0 0 1 4-2.6M10.6 5.6A13 13 0 0 1 21 9.5M6 12.8a8.6 8.6 0 0 1 3-1.9M14.5 10.6a8.6 8.6 0 0 1 3.5 2.2M9 16a4.3 4.3 0 0 1 6 0"/>' + DOT(12, 19, 1.1),
  disk: '<rect x="3.5" y="13" width="17" height="7" rx="2"/><path d="M5.5 13l2-8h9l2 8"/>' + DOT(16.5, 16.5, 1),
  cloud: '<path d="M7 18.5a4.5 4.5 0 0 1-.5-9 5.5 5.5 0 0 1 10.6 1.4A3.8 3.8 0 0 1 17 18.5z"/>',
  keyboard: '<rect x="2.5" y="6" width="19" height="12" rx="2.2"/><path d="M6 10h.01M9.3 10h.01M12.6 10h.01M15.9 10h.01M7.5 14h9"/>',
  window: '<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="M3 9h18"/>',
  rocket: '<path d="M12.5 15.5l-4-4c1.6-4.6 5-7.6 11-8-.4 6-3.4 9.4-7 12z"/><path d="M8.5 11.5l-3.5.5-2 2.5 4 .5M12.5 15.5l-.5 3.5-2.5 2-.5-4"/>' + DOT(15.3, 8.7, 1.3),
};

/** Returns markup for an icon (to be used with `html:` or innerHTML). */
export function iconSvg(name, cls = '') {
  const body = PATHS[name] || PATHS.app;
  return `<svg class="icon ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${body}</svg>`;
}

/** Returns an SVG element. */
export function icon(name, cls = '') {
  const tpl = document.createElement('template');
  tpl.innerHTML = iconSvg(name, cls);
  return tpl.content.firstElementChild;
}

export const ICON_NAMES = Object.keys(PATHS);
