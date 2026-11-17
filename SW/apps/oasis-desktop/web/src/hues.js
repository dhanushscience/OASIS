// One colour per button, the same on its card, on its keys and in the Configure panel
// (c: key, e: key edge, t/te: tint for the modifier keys of that button).
export const HUES = [
  { c: '#ee5a24', e: '#a63a12', t: '#f6bda5', te: '#bd8467', ink: '#5b2a12' },
  { c: '#1f9e8f', e: '#136b60', t: '#a9dcd5', te: '#6fa59e', ink: '#0f4a43' },
  { c: '#6a5de0', e: '#443aa5', t: '#c3bff0', te: '#8a84bf', ink: '#2a2370' },
];

/** The inline colours for a key: solid for the button's own key, a tint for a modifier of it. */
export const paint = (/** @type {number} */ i, /** @type {boolean} */ solid) => {
  const h = HUES[i];
  return solid ? `--c:${h.c};--e:${h.e}` : `--c:${h.t};--e:${h.te};color:${h.ink}`;
};
