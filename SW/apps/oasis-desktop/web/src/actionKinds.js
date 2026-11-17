// What the one action box knows: whether text is keys, a website or an app, which common actions
// to offer, and how a real key press turns into the text the engine reads. The key names mirror
// crates/oasis-core/src/shortcut.rs (`parse_key`); the engine stays the judge when you save.

const MODIFIERS = new Set(['ctrl', 'control', 'shift', 'alt', 'win', 'windows']);
const NAMED = new Set([
  'space', 'enter', 'return', 'esc', 'escape', 'tab', 'back', 'backspace', 'delete', 'del', 'up', 'down', 'left', 'right',
  'home', 'end', 'pageup', 'pgup', 'pagedown', 'pgdn', 'insert', 'ins', 'volumeup', 'volup', 'volumedown', 'voldown', 'volumemute', 'mute', 'capslock', 'caps',
  'playpause', 'play', 'pause', 'nexttrack', 'next', 'prevtrack', 'previoustrack', 'previous', 'prev', 'mediastop',
]);
const PUNCT = new Set([
  '.', 'period', 'oemperiod', ',', 'comma', 'oemcomma', '/', 'slash', 'oemquestion', ';', 'semicolon', 'oem1',
  "'", 'quote', 'oem7', 'oemquotes', '"', 'doublequote', '[', 'lbracket', 'oem4', ']', 'rbracket', 'oem6',
  '\\', 'backslash', 'oem5', '-', 'minus', 'dash', 'oemminus', '=', 'equals', 'equal', 'oemplus', '`', 'grave', 'backtick', 'oem3',
]);

/** Whether the engine knows this one key name. */
export function isKeyName(/** @type {string} */ name) {
  const k = name.trim().toLowerCase();
  return MODIFIERS.has(k) || NAMED.has(k) || PUNCT.has(k) || /^[a-z0-9]$/.test(k) || /^f([1-9]|1\d|2[0-4])$/.test(k);
}

/** The parts of a chord, or null when any part is not a key the engine knows (or there are none). */
export function parseChord(/** @type {string} */ text) {
  const parts = text.split('+').map((p) => p.trim()).filter(Boolean);
  return parts.length && parts.every(isKeyName) ? parts : null;
}

/**
 * Whether typing this should flip the box to Keys. Stricter than `parseChord`: "c" or "end" could
 * be the start of an app name, so a lone key counts only if it is a function key, and a longer
 * chord needs a modifier or a function key.
 */
export function looksLikeKeys(/** @type {string} */ text) {
  const parts = parseChord(text);
  if (!parts) return false;
  const lower = parts.map((p) => p.toLowerCase());
  const fn = (/** @type {string} */ k) => /^f([1-9]|1\d|2[0-4])$/.test(k);
  if (parts.length === 1) return fn(lower[0]);
  return lower.some((k) => MODIFIERS.has(k) || fn(k));
}

/** A website rather than an app: a web address, or a bare domain with no spaces or folders. */
export function looksLikeSite(/** @type {string} */ value) {
  const t = value.trim();
  return /^https?:\/\//i.test(t) || (/^[\w-]+(\.[\w-]+)*\.(com|org|net|io|so|app|dev|co|ai|me|tv|gg|in|uk|de)(\/\S*)?$/i.test(t) && !t.includes('\\'));
}

/** `chrome|https://example.com`: a site in a named browser, the form the engine's "open app" already runs. */
export function splitBrowser(/** @type {string} */ value) {
  const at = value.indexOf('|');
  const browser = value.slice(0, Math.max(at, 0)).trim(), url = value.slice(at + 1).trim();
  return browser && looksLikeSite(url) ? { browser, url } : null;
}

/** @param {string} text @param {string[]} apps */
export function looksLikeApp(text, apps) {
  const t = text.trim();
  if (!t) return false;
  return /\.exe$/i.test(t) || /^[a-z]:\\/i.test(t) || t.startsWith('shell:') || apps.some((a) => a.toLowerCase() === t.toLowerCase());
}

/**
 * Which of the box's kinds typed text belongs to, when it is clear: `keys`, `site`, `app`, or null.
 * @param {string} text @param {string[]} apps
 */
export function detect(text, apps) {
  if (looksLikeKeys(text)) return 'keys';
  if (looksLikeSite(text)) return 'site';
  if (looksLikeApp(text, apps)) return 'app';
  return null;
}

/** The media keys: the pad sends the same key a keyboard's Fn media row does, so any player answers. */
export const MEDIA = [
  { name: 'Play / Pause', keys: 'PlayPause' },
  { name: 'Next track', keys: 'NextTrack' },
  { name: 'Previous track', keys: 'PrevTrack' },
  { name: 'Stop', keys: 'MediaStop' },
];

/** Common things that are only a key combination, offered by name. */
export const QUICK = [
  { name: 'Lock PC', keys: 'Win+L' },
  { name: 'Show desktop', keys: 'Win+D' },
  { name: 'Screenshot', keys: 'Win+Shift+S' },
  { name: 'Task view', keys: 'Win+Tab' },
  { name: 'Emoji picker', keys: 'Win+.' },
  { name: 'Volume up', keys: 'VolumeUp' },
  { name: 'Volume down', keys: 'VolumeDown' },
  { name: 'Mute', keys: 'Mute' },
  ...MEDIA,
  { name: 'Copy', keys: 'Ctrl+C' },
  { name: 'Paste', keys: 'Ctrl+V' },
  { name: 'Undo', keys: 'Ctrl+Z' },
];

/**
 * What to offer under the box, in the order of DESIGN.md: installed apps first, then a website
 * guess, then the common actions. `kind` is what picking it does.
 * @param {string} text @param {string[]} apps @param {number} seg  0 keys, 1 app or site
 * @returns {{ kind: 'app' | 'site' | 'quick', name: string, value: string, note: string }[]}
 */
export function suggestions(text, apps, seg) {
  const q = text.trim().toLowerCase();
  if (!q) return [];
  /** @type {{ kind: 'app' | 'site' | 'quick', name: string, value: string, note: string }[]} */
  const out = [];
  if (seg === 1) {
    const hits = apps.filter((a) => a.toLowerCase().includes(q));
    // a name that starts with what was typed first, then shorter names
    hits.sort((a, b) => Number(!a.toLowerCase().startsWith(q)) - Number(!b.toLowerCase().startsWith(q)) || a.length - b.length);
    for (const name of hits.slice(0, 6)) out.push({ kind: 'app', name, value: name, note: 'App' });
    if (looksLikeSite(text) || (/^[\w-]+\.[a-z]{2,}(\/\S*)?$/i.test(q) && !q.includes('\\'))) {
      out.push({ kind: 'site', name: text.trim(), value: text.trim(), note: 'Website' });
    }
  }
  if (seg === 0 || seg === 1) {
    for (const a of QUICK.filter((x) => x.name.toLowerCase().includes(q)).slice(0, 3)) {
      out.push({ kind: 'quick', name: a.name, value: a.keys, note: a.keys });
    }
  }
  return out;
}

const CODE_KEYS = /** @type {Record<string, string>} */ ({
  Period: '.', Comma: ',', Slash: '/', Semicolon: ';', Quote: "'", BracketLeft: '[', BracketRight: ']',
  Backslash: '\\', Minus: '-', Equal: '=', Backquote: '`',
});
const NAMED_KEYS = /** @type {Record<string, string>} */ ({
  ' ': 'Space', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', PageUp: 'PageUp', PageDown: 'PageDown',
  Home: 'Home', End: 'End', Insert: 'Insert', Delete: 'Delete', Enter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', CapsLock: 'CapsLock',
  AudioVolumeUp: 'VolumeUp', AudioVolumeDown: 'VolumeDown', AudioVolumeMute: 'Mute',
  MediaPlayPause: 'PlayPause', MediaTrackNext: 'NextTrack', MediaTrackPrevious: 'PrevTrack', MediaStop: 'MediaStop',
});

/**
 * The chord text for a key press, or null when it is an ordinary typing key. A press records when it
 * has Ctrl or Alt held, or is a function key; plain letters and Enter, Esc, Tab, Backspace stay
 * typing, so the box can still be edited by hand.
 * @param {{ key: string, code: string, ctrlKey: boolean, altKey: boolean, shiftKey: boolean, metaKey: boolean }} e
 */
export function chordFromKeyEvent(e, bare = false) {
  if (['Control', 'Shift', 'Alt', 'Meta', 'AltGraph'].includes(e.key)) return null;
  const fn = /^F([1-9]|1\d|2[0-4])$/.test(e.key);
  // `bare`: the box is recording, so a single key such as Enter or A is a shortcut too.
  if (!bare && !fn && !e.ctrlKey && !e.altKey && !e.metaKey) return null;
  // Letters and digits come from what the layout types (an AZERTY "A" is VK_A, not the physical
  // KeyQ position); a shifted digit reports a symbol, so its physical key names it; punctuation and
  // the named keys go by position, then by the character itself.
  let key = null;
  if (fn) key = e.key;
  else if (/^[a-z0-9]$/i.test(e.key)) key = e.key.toUpperCase();
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else key = CODE_KEYS[e.code] ?? NAMED_KEYS[e.key] ?? (PUNCT.has(e.key) && e.key.length === 1 ? e.key : null);
  if (!key) return null;
  return [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win', key].filter(Boolean).join('+');
}

/** Whether two shortcut texts are the same keys, whatever the order or case: Ctrl+Shift+1 and shift+ctrl+1. */
export function sameChord(/** @type {string} */ a, /** @type {string} */ b) {
  const norm = (/** @type {string} */ s) => s.split('+').map((p) => p.trim().toLowerCase()).filter(Boolean).sort().join('+');
  return !!norm(a) && norm(a) === norm(b);
}
