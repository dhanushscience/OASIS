// What Mr. Science says. One file, so the wording can be changed without touching a component.
// No imports: the caption panel over other programs loads this file as it is.
// A line is at most 90 characters (lines.test.js), so it fits two rows of the caption panel.

export const PITCH = ['Talk to your PC', 'and get things done.'];
export const PITCH_PARTS = ['Hold a key,', 'say it,', 'and OASIS types it or does it.'];
/** The second scene of the opening: key 2 opens an app by voice. */
export const PITCH_PARTS_OPEN = ['Hold the second key,', 'say "open Notes",', 'and the app opens.'];

/** By step id. Line 0 is what he says on arriving at the step. */
export const LINES = {
  welcome: ["Hi! I'm Mr. Science.", 'First things first: do you already have an OASIS pad?'],
  layout: ['Do you want a vertical or a horizontal pop-up?'],
  browser: ["When no browser is open, I'll open pages in this one."],
  microphone: ["Which microphone should I listen with? Your PC's default is picked for you."],
  groq: ['This PC is a little light for the voice model. Groq can do the listening online.'],
  exit: ["That's the setup done. Now let's press a button."],
  device: ['Switch your pad on. I am looking for it.', 'Found it!'],
  first_press: ['Press button 2 and watch what happens.', 'Press it once more.'],
  return: ['That was button 2 opening a website. Click OASIS to carry on.'],
  voice_key: ['Button 1 is your talk key. Hold it down while you speak.'],
  voice_open: [
    'Hold button 1 and say "open Notepad". Or click OASIS to stop.',
    'Hold it a little longer, then say it.',
    'I only heard quiet. Say it a little louder.',
    'I cannot hear a microphone. Check it in Windows sound settings.',
    'It asks first, once: press button 1 to allow it, or button 2 to cancel.',
    'That did not open Notepad. Say "open Notepad" while I watch.',
  ],
  voice_type: [
    'Click inside Notepad, then hold button 1 and say a sentence.',
    'Hold it a little longer, then say it.',
    'I only heard quiet. Say it a little louder.',
    'I cannot hear a microphone. Check it in Windows sound settings.',
    'It asks first, once: press button 1 to allow it, or button 2 to cancel.',
    'That went to another window. Click inside Notepad first.',
  ],
  voice_close: [
    'Now say "close Notepad". Closing always asks first: press button 1 to approve.',
    'Hold it a little longer, then say it.',
    'I only heard quiet. Say it a little louder.',
    'I cannot hear a microphone. Check it in Windows sound settings.',
    'Press button 1 to approve closing it, or button 2 to cancel.',
    'That did not close Notepad. Say "close Notepad" while I watch.',
  ],
  try_commands: ['Your turn: click the box, hold your talk key, say each one.'],
  done: ["That's it: open, type, close. If Notepad asks to save, that is its own question."],
};

/** Why a step cannot go on. */
export const BLOCKED = {
  offline: 'There is no internet right now, so Groq cannot be reached.',
  bluetooth: 'Bluetooth is off or missing, so I cannot look for your pad.',
  device_lost: 'Your pad dropped. Switch it on again, or use the keyboard instead.',
  integrity: 'Your profiles file was changed outside OASIS, so buttons cannot run yet.',
  download: 'The voice model is still downloading. This part starts when it is here.',
  no_model: 'There is no voice model on this PC yet, so the voice part has to wait.',
  no_microphone: 'I cannot hear a microphone. Check it in Windows sound settings.',
};

/** The model download, by the state the downloader reports. */
export const DOWNLOAD = {
  checking: 'Checking what this PC can run.',
  cleaning: 'Clearing out old voice models.',
  downloading: 'Downloading the voice model in the background.',
  paused: 'The voice model download is paused.',
  metered: 'On a metered connection: the voice model download is waiting.',
  offline: 'No internet: the voice model download is waiting.',
  failed: 'The voice model download failed. OASIS will try again.',
  testing: 'Testing the voice model on this PC.',
  ready: 'The voice model is ready.',
  none: 'No voice model is set to download.',
};

/** When button 2 refuses to run. Told apart by how the engine's message starts. */
export const REFUSED = {
  integrity: 'Buttons cannot run until the profiles file is restored. You can skip this part.',
  off: 'Actions are switched off, so the button did nothing. Switch them on, or skip this part.',
  other: 'That press did not run. Try once more, or skip this part.',
};

export const NOTICE = 'Voice is ready. Shall I show you how to talk to your PC?';

/** The caption for a step, by its place in that step's lines. */
export const line = (/** @type {string} */ step, i = 0) => /** @type {Record<string, string[]>} */ (LINES)[step]?.[i] ?? '';
