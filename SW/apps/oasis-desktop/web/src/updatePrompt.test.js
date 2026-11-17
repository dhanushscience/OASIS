import test from 'node:test';
import assert from 'node:assert/strict';
import { isNewer, shouldPrompt, DAY_MS } from './updatePrompt.js';

test('isNewer compares versions part by part', () => {
  assert.equal(isNewer('0.1.3', '0.1.2'), true);
  assert.equal(isNewer('0.1.10', '0.1.9'), true);
  assert.equal(isNewer('1.0', '0.9.9'), true);
  assert.equal(isNewer('0.1.2', '0.1.2'), false);
  assert.equal(isNewer('0.1.1', '0.1.2'), false);
  assert.equal(isNewer('0.1.2', '0.1'), true);
});

const info = { current: '0.1.2', available: '0.1.3', progress: null };
const done = { loaded: true, status: 'complete' };
const now = 1_000 * DAY_MS;

test('shows for a newer version once Getting Started is over', () => {
  assert.equal(shouldPrompt(info, done, null, now), true);
  assert.equal(shouldPrompt(info, { loaded: true, status: 'dismissed' }, null, now), true);
});

test('never during first run or before the guide state is known', () => {
  for (const status of ['new', 'running', 'waiting_for_voice', 'voice_offered']) {
    assert.equal(shouldPrompt(info, { loaded: true, status }, null, now), false, status);
  }
  assert.equal(shouldPrompt(info, { loaded: false, status: 'dismissed' }, null, now), false);
});

test('nothing newer, or already installing: no pop-up', () => {
  assert.equal(shouldPrompt({ ...info, available: null }, done, null, now), false);
  assert.equal(shouldPrompt({ ...info, available: '0.1.2' }, done, null, now), false);
  assert.equal(shouldPrompt({ ...info, progress: 40 }, done, null, now), false);
});

test('once a day per version: Later snoozes it for a day, a newer version shows at once', () => {
  const seen = { version: '0.1.3', at: now - 1000 };
  assert.equal(shouldPrompt(info, done, seen, now), false);
  assert.equal(shouldPrompt(info, done, seen, seen.at + DAY_MS), true);
  assert.equal(shouldPrompt({ ...info, available: '0.1.4' }, done, seen, now), true);
});
