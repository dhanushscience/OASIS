import test from 'node:test';
import assert from 'node:assert/strict';
import { stepFor, tick } from './tryCommands.js';

test('typing, copy, paste, undo', () => {
  assert.equal(stepFor({ type: 'input', inputType: 'insertText', text: 'hi' }), 'type');
  assert.equal(stepFor({ type: 'input', inputType: 'insertText', text: '' }), null);
  assert.equal(stepFor({ type: 'copy' }), 'copy');
  assert.equal(stepFor({ type: 'paste' }, { copy: true }), 'paste');
  assert.equal(stepFor({ type: 'input', inputType: 'insertFromPaste', text: 'x' }, { copy: true }), 'paste');
  // dictation arrives as a paste: before "copy" it is the typed sentence
  assert.equal(stepFor({ type: 'paste' }), 'type');
  assert.equal(stepFor({ type: 'input', inputType: 'insertFromPaste', text: 'x' }), 'type');
  assert.equal(stepFor({ type: 'input', inputType: 'historyUndo', text: 'x' }), 'undo');
  assert.equal(stepFor({ type: 'keydown', key: 'z', ctrlKey: true }), 'undo');
});

test('select all needs the whole non-empty text selected', () => {
  assert.equal(stepFor({ type: 'keyup', key: 'a', start: 0, end: 5, length: 5 }), 'all');
  assert.equal(stepFor({ type: 'select', start: 0, end: 3, length: 5 }), null);
  assert.equal(stepFor({ type: 'keyup', start: 0, end: 0, length: 0 }), null);
});

test('save is Ctrl+S, other keys do nothing', () => {
  assert.equal(stepFor({ type: 'keydown', key: 'S', ctrlKey: true }), 'save');
  assert.equal(stepFor({ type: 'keydown', key: 's' }), null);
});

test('a second undo ticks the chord step', () => {
  let d = tick({}, 'undo');
  assert.equal(d.chord, undefined);
  d = tick(d, 'undo');
  assert.equal(d.chord, true);
  assert.deepEqual(tick({ a: true }, null), { a: true });
});
