// One confirmation pop-up for every delete: `await sure({ title, body, yes })` resolves true only on the red button.
export const confirmUi = $state({ open: false, title: '', body: '', yes: 'Delete', keep: 'Keep it' });
/** @type {((ok: boolean) => void) | null} */
let resolve = null;

/** @param {{ title: string, body?: string, yes?: string, keep?: string }} o */
export function sure(o) {
  resolve?.(false); // a second ask replaces an unanswered first
  Object.assign(confirmUi, { body: '', yes: 'Delete', keep: 'Keep it', ...o, open: true });
  return new Promise((/** @type {(ok: boolean) => void} */ r) => (resolve = r));
}

/** @param {boolean} ok */
export function answer(ok) {
  confirmUi.open = false;
  resolve?.(ok);
  resolve = null;
}
