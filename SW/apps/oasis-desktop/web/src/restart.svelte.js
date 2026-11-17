// A restart that waits for a quiet moment (VOICE_MODEL_SETUP.md section 5). The app asks for one
// (`restart-needed`); this decides when it is safe, once a minute and at the first chance:
//   - no screen has changes not saved yet            (busy.svelte.js)
//   - no dialog, pop-up or editor is open
//   - nothing typed or clicked in the window for the last 60 seconds
//   - the engine is not recording, transcribing, running an action or a Quick Launch setup
// While any of these fails nothing restarts; the strip offers "Restart now" and "Later".

import { hasUnsaved } from './busy.svelte.js';

const tauri = window.__TAURI__;
const QUIET_MS = 60_000;
const CHECK_EVERY_MS = 60_000;

/** @type {{ reason: string | null, later: boolean, error: string | null }} */
export const restart = $state({ reason: null, later: false, error: null });

let lastInput = Date.now();
let started = false;
let checking = false;

/** A dialog, pop-up or text field in use means the person is in the middle of something. */
function somethingOpen() {
  if (document.querySelector('[role="dialog"], [aria-modal="true"], dialog[open]')) return true;
  const el = document.activeElement;
  return el instanceof HTMLElement && el.matches('input:not([type="radio"]):not([type="checkbox"]):not([type="range"]), textarea, select, [contenteditable]');
}

/** Whether a restart would interrupt nothing right now. */
export async function quietNow() {
  if (hasUnsaved() || somethingOpen() || Date.now() - lastInput < QUIET_MS) return false;
  try {
    return !(await tauri.core.invoke('engine_busy'));
  } catch {
    return false;
  }
}

async function tryRestart() {
  if (!restart.reason || checking || !(await quietNow())) return;
  checking = true;
  try {
    await tauri.core.invoke('restart_oasis');
  } catch (e) {
    // The restart was refused (an administrator copy, say): say so, and stop trying every minute.
    restart.error = String(e);
    restart.later = false;
    restart.reason = null;
  } finally {
    checking = false;
  }
}

/** The "Restart now" button: the person chose it, so only the engine's work is waited for. */
export async function restartNow() {
  try {
    await tauri.core.invoke('restart_oasis');
  } catch (e) {
    restart.error = String(e);
  }
}

export function startRestart() {
  if (started || !tauri) return;
  started = true;
  for (const type of ['keydown', 'pointerdown', 'input']) window.addEventListener(type, () => (lastInput = Date.now()), true);
  tauri.core.invoke('restart_pending').then((/** @type {string | null} */ r) => (restart.reason = r));
  tauri.event.listen('restart-needed', (/** @type {{ payload: string }} */ e) => {
    restart.reason = e.payload;
    restart.later = false;
    restart.error = null;
    tryRestart();
  });
  setInterval(tryRestart, CHECK_EVERY_MS);
}
