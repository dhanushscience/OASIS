// The first-run guide, as a page sees it. The one copy of its progress lives in the app (Rust);
// this is a reactive mirror of it, so no page keeps its own idea of the step.
const tauri = /** @type {any} */ (window).__TAURI__;

export const guide = $state({
  loaded: false,
  status: 'dismissed',
  step: 'done',
  /** @type {boolean | null} */ has_device: null,
  /** @type {string | null} */ blocked: null,
  ask: false,
  button_used: false,
  keyboard: true,
  chord: '',
});

const take = (/** @type {object} */ view) => Object.assign(guide, view, { loaded: true });

let started = false;
export async function startGuide() {
  if (started || !tauri) return;
  started = true;
  tauri.event.listen('onboarding-changed', (/** @type {{ payload: object }} */ e) => take(e.payload));
  try { take(await tauri.core.invoke('onboarding_state')); } catch { /* no guide: it stays dismissed */ }
}

/** Tell the guide something happened: `send('device_answer', true)`. */
export const send = (/** @type {string} */ type, /** @type {unknown} */ value = null) =>
  tauri?.core.invoke('onboarding_event', { event: { type, value } }).catch(() => {});

/** The guide is on screen step by step, and at this step. */
export const at = (/** @type {string} */ step) => (guide.status === 'running' || guide.status === 'new') && guide.step === step;
