// The engine's status, polled once a second and refreshed on every engine event. The shell draws
// the pad card, the Live switch and the banners from this; the old screens poll for themselves.
const tauri = window.__TAURI__;

/** @type {{ engine: any, error: string | null, loaded: boolean }} */
export const status = $state({ engine: null, error: null, loaded: false });

export async function refreshStatus() {
  if (!tauri) return;
  try {
    const reply = await tauri.core.invoke('status');
    status.engine = reply.engine;
    status.error = reply.error;
  } catch (e) {
    status.engine = null;
    status.error = String(e);
  }
  status.loaded = true;
}

export function startStatus() {
  refreshStatus();
  setInterval(refreshStatus, 1000);
  tauri?.event.listen('engine-event', refreshStatus);
}

