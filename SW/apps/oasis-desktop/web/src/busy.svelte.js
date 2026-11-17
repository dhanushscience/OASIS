// Where the window is not safe to restart under the user: a screen with changes not saved yet.
// Each editing screen reports its own state; restart.svelte.js asks (VOICE_MODEL_SETUP.md section 5).

/** @type {Record<string, true>} */
const unsaved = $state({});

/**
 * Report that screen `id` has (or no longer has) changes not saved yet.
 * @param {string} id
 * @param {boolean} on
 */
export function reportUnsaved(id, on) {
  if (on) unsaved[id] = true;
  else delete unsaved[id];
}

/**
 * Keep `id` reported while `read()` is true; a screen calls this once, from its script.
 * @param {string} id
 * @param {() => boolean} read
 */
export function trackUnsaved(id, read) {
  $effect(() => {
    reportUnsaved(id, !!read());
    return () => reportUnsaved(id, false);
  });
}

export const hasUnsaved = () => Object.keys(unsaved).length > 0;
