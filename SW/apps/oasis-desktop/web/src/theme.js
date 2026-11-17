// The theme is a window preference, not a setting: it is 'system' (follow Windows, the default), 'light' or 'dark'.
// Only an explicit light/dark is saved; no saved value means system. app.html sets the first value before paint.
const KEY = 'oasis-theme';
const dark = () => window.matchMedia?.('(prefers-color-scheme: dark)');

export function currentTheme() {
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
}

/** @returns {'system' | 'light' | 'dark'} */
export function themeChoice() {
  try { const t = localStorage.getItem(KEY); if (t === 'light' || t === 'dark') return t; } catch { /* blocked storage: system */ }
  return 'system';
}

/** @param {'light' | 'dark'} theme */
function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
}

/**
 * Switch theme like an e-paper page redraw. The panel is written line by line, top to bottom, in very
 * fine lines (about one per few pixels, so it reads as a steady scan with a faint raster, not a jump);
 * what is drawn first is flat grey, and the colour settles in once the whole page is down. The old page
 * greys out underneath, as ghosting does. Only cheap properties move (a clip and one grayscale), so it
 * stays smooth on a full-size window.
 * Built on the browser's view transitions; without them, or with "reduce motion" on, it simply changes.
 */
/** The theme being changed to while a transition runs; null at rest. */
let changingTo = /** @type {'light' | 'dark' | null} */ (null);

function animateTo(/** @type {'light' | 'dark'} */ next) {
  // A second click while the first is still playing is ignored: two view transitions at once can bring
  // the web view down.
  if (changingTo || next === currentTheme()) return;
  const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  const start = /** @type {any} */ (document).startViewTransition?.bind(document);
  if (!start || reduced) {
    setTheme(next);
    return;
  }
  const run = (/** @type {string} */ part, /** @type {Keyframe[]} */ keyframes) =>
    document.documentElement.animate(keyframes, { duration: 1200, fill: 'both', pseudoElement: `::view-transition-${part}(root)` });
  const GREY = 'grayscale(1)';
  changingTo = next;
  const transition = start(() => setTheme(next));
  const done = () => { changingTo = null; };
  transition.finished.then(done, done);
  transition.ready.then(() => {
    // 1. the lines go down: 64 fine bands in the first half
    run('new', [
      { offset: 0, clipPath: 'inset(0 0 100% 0)', easing: 'steps(64, end)' },
      { offset: 0.5, clipPath: 'inset(0 0 0 0)' },
      { offset: 1, clipPath: 'inset(0 0 0 0)' },
    ]);
    // 2. what has been drawn is grey; in the second half the colour eases in
    run('new', [
      { offset: 0, filter: GREY, easing: 'linear' },
      { offset: 0.5, filter: GREY, easing: 'ease-in-out' },
      { offset: 1, filter: 'grayscale(0)' },
    ]);
    // 3. the page being replaced greys out underneath
    run('old', [
      { offset: 0, filter: 'none', easing: 'ease-out' },
      { offset: 0.5, filter: 'grayscale(1)' },
      { offset: 1, filter: 'grayscale(1)' },
    ]);
  }).catch(() => {});
}

/** @param {'system' | 'light' | 'dark'} choice */
export function chooseTheme(choice) {
  try { if (choice === 'system') localStorage.removeItem(KEY); else localStorage.setItem(KEY, choice); } catch { /* it still changes */ }
  animateTo(choice === 'system' ? (dark()?.matches ? 'dark' : 'light') : choice);
}

// While the choice is system, Windows switching between light and dark switches OASIS with it.
dark()?.addEventListener('change', (e) => { if (themeChoice() === 'system') animateTo(e.matches ? 'dark' : 'light'); });
