// The three buttons from the keyboard, inside the OASIS window.
//
// Outside the window the system-wide key listener (the engine) turns a chosen key into a button press.
// Windows does not give it the keys typed while the OASIS window itself is in front, so there the window
// matches its own keys and tells the engine: the key runs its button, and does not do what it normally
// does here (Tab moving to the next box, a backtick being typed).
//
// Plain letters and punctuation stay typing while a text box has the keyboard; a shortcut being recorded
// takes every key. Tab, Caps Lock, function keys and chords with Ctrl, Alt or Win are not typing.
import { data } from './profiles.svelte.js';
import { chordFromKeyEvent, sameChord } from './actionKinds.js';

const invoke = window.__TAURI__?.core?.invoke;

const EDITABLE = 'input:not([type=checkbox], [type=radio], [type=button], [type=file]), textarea, select, [contenteditable]';

/** 0 nothing being typed into, 1 a text box, 2 a shortcut being recorded (a read-only box). */
function typingMode() {
  const el = document.activeElement;
  if (!(el instanceof HTMLElement) || !el.matches(EDITABLE)) return 0;
  return el instanceof HTMLInputElement && el.readOnly ? 2 : 1;
}

// The engine's own listener leaves the window's keys alone when this says so (see `set_typing`).
let told = 0;
function tellEngine() {
  const now = document.hasFocus() && !document.hidden ? typingMode() : 0;
  if (now === told) return;
  told = now;
  invoke?.('set_typing', { mode: now }).catch(() => {});
}
const soon = () => setTimeout(tellEngine, 0); // focusout fires before the next element has focus
['focusin', 'focusout'].forEach((e) => document.addEventListener(e, soon));
['focus', 'blur'].forEach((e) => window.addEventListener(e, soon));
document.addEventListener('visibilitychange', soon);

/** The physical key (`event.code`) holding each button down, so its release is ours too. */
const held = new Map();
const send = (/** @type {number} */ button, /** @type {boolean} */ down) => invoke?.('press_button', { button, down }).catch(() => {});

/** Until when the Caps Lock events in the window are our own (see below), as a time from performance.now(). */
let ownCapsUntil = 0;

function onKey(/** @type {KeyboardEvent} */ e) {
  if (!data.input.keyboard) return;
  const code = e.code;
  if (code === 'CapsLock' && performance.now() < ownCapsUntil) return; // our own taps putting the lock back

  if (e.type === 'keyup') {
    const button = held.get(code);
    if (button === undefined) return;
    held.delete(code);
    e.preventDefault();
    e.stopPropagation();
    send(button, false);
    return;
  }

  if (e.repeat) {
    if (held.has(code)) e.preventDefault(); // holding the key is one press
    return;
  }
  const mode = typingMode();
  if (mode === 2) return; // recording a shortcut
  const chord = chordFromKeyEvent(e, true);
  if (!chord) return;
  const key = chord.split('+').pop() ?? '';
  const printable = key.length === 1 || key === 'Space';
  if (mode === 1 && printable && !e.ctrlKey && !e.altKey && !e.metaKey) return; // typing
  const button = data.input.keys.findIndex((k) => sameChord(k, chord));
  if (button < 0) return;
  if (held.has(code)) send(held.get(code), false); // a release we never heard: let that press go first
  held.set(code, button);
  e.preventDefault();
  e.stopPropagation();
  send(button, true);
  // The page cannot keep Caps Lock from switching the lock (the engine's listener does, elsewhere), and
  // Windows switches it as the key goes down, so put it back now, not when the key comes up: the lock
  // is wrong for a few milliseconds instead of for as long as the key is held. The tap that puts it
  // back is a Caps Lock key too, so its own events are left alone (see above), or it would press the
  // button again and put the lock back again, for ever.
  if (code === 'CapsLock') {
    ownCapsUntil = performance.now() + 120;
    invoke?.('restore_caps_lock').catch(() => {});
  }
}

['keydown', 'keyup'].forEach((t) => window.addEventListener(t, /** @type {EventListener} */ (onKey), true));
// Leaving the window with a key down: let its button go, or it would stay pressed.
window.addEventListener('blur', () => {
  for (const button of held.values()) send(button, false);
  held.clear();
});
