// Which practice step an event on the textarea completes. Pure, so it is tested without a browser.
export const STEPS = [
  { id: 'type', say: 'Say a full sentence, like “this is a test of my voice”', done: 'It typed what you said' },
  { id: 'all', say: 'select all', done: 'Everything is selected' },
  { id: 'copy', say: 'copy', done: 'Copied' },
  { id: 'paste', say: 'paste', done: 'Pasted', hint: 'If the text is still selected, paste replaces it. Or click at the end first.' },
  { id: 'undo', say: 'undo', done: 'Undone' },
  { id: 'save', say: 'save this', done: 'Saved' },
  { id: 'chord', say: 'control z', done: 'Same undo, said as the keys', optional: true },
];

/**
 * @param {{ type: string, inputType?: string, key?: string, ctrlKey?: boolean, metaKey?: boolean, text?: string, start?: number, end?: number, length?: number }} e
 * @param {Record<string, boolean>} [done] steps already ticked
 * @returns {string | null} the step id the event completes
 */
export function stepFor(e, done = {}) {
  const mod = !!(e.ctrlKey || e.metaKey);
  const key = (e.key ?? '').toLowerCase();
  // Dictated text reaches the box by a paste, so a paste before "copy" is the typed sentence, not the paste command.
  const pasted = done.copy ? 'paste' : 'type';
  if (e.type === 'copy') return 'copy';
  if (e.type === 'paste') return pasted;
  if (e.type === 'input') {
    if (e.inputType === 'historyUndo') return 'undo';
    if (e.inputType === 'insertFromPaste') return pasted;
    return e.text ? 'type' : null;
  }
  if (e.type === 'keydown' && mod) {
    if (key === 's') return 'save';
    if (key === 'z') return 'undo';
  }
  if (e.type === 'select' || e.type === 'keyup' || e.type === 'keydown') {
    if ((e.length ?? 0) > 0 && e.start === 0 && e.end === e.length) return 'all';
  }
  return null;
}

/**
 * Ticks after an event; a second undo also ticks the optional "control z" step.
 * @param {Record<string, boolean>} done
 * @param {string | null} step
 */
export function tick(done, step) {
  if (!step) return done;
  const next = { ...done, [step]: true };
  if (step === 'undo' && done.undo) next.chord = true;
  return next;
}
