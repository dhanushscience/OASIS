import test from 'node:test';
import assert from 'node:assert/strict';
import { LINES, BLOCKED, DOWNLOAD, REFUSED, NOTICE, line } from './lines.js';

// The same names as oasis_core::onboarding and the downloader's phases.
const STEPS = ['welcome', 'layout', 'browser', 'microphone', 'groq', 'exit', 'device', 'first_press', 'return', 'voice_key', 'voice_open', 'voice_type', 'voice_close', 'try_commands', 'done'];
const REASONS = ['offline', 'bluetooth', 'device_lost', 'integrity', 'download', 'no_model', 'no_microphone'];
const PHASES = ['checking', 'cleaning', 'downloading', 'paused', 'metered', 'offline', 'failed', 'testing', 'ready', 'none'];

test('every step, blocked reason and download state has something to say', () => {
  for (const step of STEPS) assert.ok(line(step), step);
  for (const reason of REASONS) assert.ok(/** @type {Record<string, string>} */ (BLOCKED)[reason], reason);
  for (const phase of PHASES) assert.ok(/** @type {Record<string, string>} */ (DOWNLOAD)[phase], phase);
});

test('no line is longer than 90 characters, and none uses a long dash', () => {
  const all = [...Object.values(LINES).flat(), ...Object.values(BLOCKED), ...Object.values(DOWNLOAD), ...Object.values(REFUSED), NOTICE];
  for (const text of all) {
    assert.ok(text.length <= 90, `${text.length}: ${text}`);
    assert.ok(!text.includes('—'), text);
  }
});
