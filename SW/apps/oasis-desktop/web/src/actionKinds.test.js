import test from 'node:test';
import assert from 'node:assert/strict';
import { parseChord, looksLikeKeys, looksLikeSite, splitBrowser, detect, suggestions, chordFromKeyEvent, QUICK } from './actionKinds.js';

test('a site saved with a browser splits back into the two; an app or a bare site does not', () => {
  assert.deepEqual(splitBrowser('chrome|https://example.com'), { browser: 'chrome', url: 'https://example.com' });
  assert.deepEqual(splitBrowser(' brave | github.com/x '), { browser: 'brave', url: 'github.com/x' });
  for (const no of ['notepad', 'https://example.com', '|https://example.com', 'chrome|notepad', 'C:\\a|b\\app.exe']) assert.equal(splitBrowser(no), null, no);
});

const press = (/** @type {object} */ o) => ({ key: '', code: '', ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...o });

test('chords follow the engine: every part must be a known key, empty parts are dropped', () => {
  assert.deepEqual(parseChord('Ctrl + Shift + P'), ['Ctrl', 'Shift', 'P']);
  assert.deepEqual(parseChord('win+l'), ['win', 'l']);
  assert.deepEqual(parseChord('Ctrl+'), ['Ctrl']); // the engine accepts this one too
  assert.deepEqual(parseChord('Ctrl+.'), ['Ctrl', '.']);
  assert.equal(parseChord('Ctrl+Foo'), null);
  assert.equal(parseChord(''), null);
  assert.equal(parseChord('chrome'), null);
});

test('typing flips the box to Keys only when it cannot be the start of an app name', () => {
  for (const yes of ['Ctrl+Shift+P', 'alt+f4', 'Win+D', 'F5', 'f12', 'Ctrl+c']) assert.ok(looksLikeKeys(yes), yes);
  for (const no of ['c', 'end', 'tab', 'k', 'chrome', 'figma', 'a+b', 'Ctrl+Foo', '']) assert.ok(!looksLikeKeys(no), no);
});

test('websites: addresses and bare domains, not folders or plain words', () => {
  for (const yes of ['https://example.com', 'mail.google.com', 'github.com/x/y', 'HTTP://a.b']) assert.ok(looksLikeSite(yes), yes);
  for (const no of ['notepad', 'C:\\x\\foo.com', 'my.app notes', 'google']) assert.ok(!looksLikeSite(no), no);
});

test('detect says keys, site or app when it is clear, and nothing when it is not', () => {
  const apps = ['Notepad', 'Figma'];
  assert.equal(detect('Ctrl+Shift+P', apps), 'keys');
  assert.equal(detect('youtube.com', apps), 'site');
  assert.equal(detect('notepad', apps), 'app');
  assert.equal(detect('C:\\Tools\\x.exe', apps), 'app');
  assert.equal(detect('fig', apps), null);
  assert.equal(detect('', apps), null);
});

test('suggestions: apps first (names that start with the text first), then a website guess, then common actions', () => {
  const apps = ['OneNote', 'Notepad++', 'Notepad', 'Snote'];
  const s = suggestions('note', apps, 1);
  assert.deepEqual(s.map((x) => x.name), ['Notepad', 'Notepad++', 'Snote', 'OneNote']);
  assert.equal(s[0].name, 'Notepad');
  assert.ok(s.every((x) => x.kind === 'app'));

  const web = suggestions('github.com', [], 1);
  assert.deepEqual(web.map((x) => x.kind), ['site']);

  const quick = suggestions('lock', [], 0);
  assert.deepEqual(quick, [{ kind: 'quick', name: 'Lock PC', value: 'Win+L', note: 'Win+L' }]);
  assert.deepEqual(suggestions('', ['A'], 1), []);
  assert.deepEqual(suggestions('lock', ['A'], 2), []);
});

test('every common action is a chord the engine can read', () => {
  for (const q of QUICK) assert.ok(parseChord(q.keys), `${q.name}: ${q.keys}`);
});

test('a real key press becomes chord text; plain typing does not', () => {
  assert.equal(chordFromKeyEvent(press({ key: 'p', code: 'KeyP', ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+P');
  assert.equal(chordFromKeyEvent(press({ key: 'F4', code: 'F4', altKey: true })), 'Alt+F4');
  assert.equal(chordFromKeyEvent(press({ key: 'F5', code: 'F5' })), 'F5');
  assert.equal(chordFromKeyEvent(press({ key: '1', code: 'Digit1', ctrlKey: true })), 'Ctrl+1');
  assert.equal(chordFromKeyEvent(press({ key: '.', code: 'Period', ctrlKey: true })), 'Ctrl+.');
  assert.equal(chordFromKeyEvent(press({ key: 'ArrowLeft', code: 'ArrowLeft', altKey: true })), 'Alt+Left');
  // no scan code (an on-screen keyboard, a test tool), and a layout where the physical place differs
  assert.equal(chordFromKeyEvent(press({ key: 'K', code: '', ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+K');
  assert.equal(chordFromKeyEvent(press({ key: 'ArrowLeft', code: '', altKey: true })), 'Alt+Left');
  assert.equal(chordFromKeyEvent(press({ key: 'a', code: 'KeyQ', ctrlKey: true })), 'Ctrl+A');
  assert.equal(chordFromKeyEvent(press({ key: '!', code: 'Digit1', ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+1');
  assert.equal(chordFromKeyEvent(press({ key: ',', code: '', altKey: true })), 'Alt+,');
  assert.equal(chordFromKeyEvent(press({ key: 'a', code: 'KeyA' })), null);
  assert.equal(chordFromKeyEvent(press({ key: 'Enter', code: 'Enter' })), null);
  assert.equal(chordFromKeyEvent(press({ key: 'Enter', code: 'Enter' }), true), 'Enter');
  assert.equal(chordFromKeyEvent(press({ key: 'a', code: 'KeyA' }), true), 'A');
  assert.equal(chordFromKeyEvent(press({ key: 'Tab', code: 'Tab' }), true), 'Tab');
  assert.equal(chordFromKeyEvent(press({ key: 'CapsLock', code: 'CapsLock' }), true), 'CapsLock');
  assert.equal(chordFromKeyEvent(press({ key: 'CapsLock', code: 'CapsLock' })), null, 'only while recording');
  assert.equal(chordFromKeyEvent(press({ key: 'Control', code: 'ControlLeft', ctrlKey: true }), true), null);
  assert.equal(chordFromKeyEvent(press({ key: 'Control', code: 'ControlLeft', ctrlKey: true })), null);
  assert.equal(chordFromKeyEvent(press({ key: 'Escape', code: 'Escape', ctrlKey: true })), null);
  // whatever it records, the engine reads
  for (const e of [press({ key: 'k', code: 'KeyK', ctrlKey: true, altKey: true }), press({ key: ',', code: 'Comma', altKey: true })]) {
    assert.ok(parseChord(/** @type {string} */ (chordFromKeyEvent(e))));
  }
});

test('two shortcut texts are the same keys whatever the order or case', async () => {
  const { sameChord } = await import('./actionKinds.js');
  assert.equal(sameChord('F2', 'f2'), true);
  assert.equal(sameChord('Ctrl+Shift+1', 'shift+ctrl+1'), true);
  assert.equal(sameChord('F1', 'F2'), false);
  assert.equal(sameChord('Ctrl+Q', 'Q'), false);
  assert.equal(sameChord('', ''), false, 'two empty boxes are not a clash');
});
