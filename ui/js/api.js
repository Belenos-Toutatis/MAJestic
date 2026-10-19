// Backend access: the real Tauri 2 backend when present, else the mock (demo in a browser).

const tauri = window.__TAURI__ || null;
export const isTauri = !!(tauri && tauri.core && tauri.event);

let mockModule = null;
async function mock() {
  if (!mockModule) mockModule = await import('./mock.js');
  return mockModule;
}

/** invoke('scan'), invoke('get_icon', { hint }). Rejects with a string on backend errors. */
export async function invoke(cmd, args = {}) {
  if (isTauri) return tauri.core.invoke(cmd, args);
  return (await mock()).invoke(cmd, args);
}

/** listen('job-progress', payload => ...). Resolves to an unlisten function. */
export async function listen(event, callback) {
  const handler = (e) => {
    try { callback(e ? e.payload : undefined); }
    catch (err) { console.error(`[api] handler for ${event}`, err); }
  };
  if (isTauri) return tauri.event.listen(event, handler);
  return (await mock()).listen(event, handler);
}

/** Keeps the native title bar in sync with the theme ('light' | 'dark' | null = follow Windows). */
export function setWindowTheme(theme) {
  if (!isTauri) return;
  try {
    const win = tauri.window?.getCurrentWindow?.();
    const p = win?.setTheme?.(theme);
    if (p && typeof p.catch === 'function') p.catch(() => {});
  } catch { /* ignored */ }
}

/** Turns any rejection value into a readable message. */
export function errorText(err) {
  if (!err) return '';
  if (typeof err === 'string') return err;
  if (err.message) return err.message;
  try { return JSON.stringify(err); } catch { return String(err); }
}
