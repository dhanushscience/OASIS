// The pad, as the window sees it. `pad.lit` is which of the three keys is down, for the key caps and
// the pad drawing to flash: in 120 ms, out 400 ms (DESIGN.md section 0).
const tauri = window.__TAURI__;

export const pad = $state({ lit: [false, false, false] });
/** Which drawing the Profiles screen shows when the pad and the keyboard are both usable (the switch in the top bar). */
export const view = $state({ keys: false, layout: savedLayout() });

/** How the keyboard is drawn: 'auto' (minimal in a small window), 'full' or 'minimal'. Saved in this window only. */
function savedLayout() {
  try { const l = localStorage.getItem('oasis-kb-layout'); if (l === 'full' || l === 'minimal') return l; } catch { /* blocked storage: auto */ }
  return 'auto';
}
export function setLayout(/** @type {'auto' | 'full' | 'minimal'} */ l) {
  view.layout = l;
  try { if (l === 'auto') localStorage.removeItem('oasis-kb-layout'); else localStorage.setItem('oasis-kb-layout', l); } catch { /* it still changes */ }
}

const FLASH_MIN_MS = 120; // a tap is shorter than the flash takes to arrive, so it is held for that long
const since = [0, 0, 0];
/** @type {ReturnType<typeof setTimeout>[]} */
const timers = [];

function light(/** @type {number} */ i, /** @type {boolean} */ down) {
  if (i < 0 || i > 2) return;
  clearTimeout(timers[i]);
  if (down) {
    pad.lit[i] = true;
    since[i] = performance.now();
  } else {
    timers[i] = setTimeout(() => (pad.lit[i] = false), Math.max(0, FLASH_MIN_MS - (performance.now() - since[i])));
  }
}

export function startPad() {
  tauri?.event.listen('engine-event', (/** @type {{ payload: any }} */ { payload: e }) => {
    if (e.type === 'pad_button') light(e.button - 1, e.down);
  });
}
