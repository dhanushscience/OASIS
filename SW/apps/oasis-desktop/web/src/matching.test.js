import test from 'node:test';
import assert from 'node:assert/strict';
import { tokens, overlaps } from './matching.js';

test('names are compared without case, extension, scheme, www or path', () => {
  assert.deepEqual(tokens('Figma.exe, https://www.Netflix.com/browse'), ['figma', 'netflix.com']);
  assert.deepEqual(tokens('*'), []);
  assert.deepEqual(tokens(''), []);
});

test('the same app or site overlaps, different ones and the star do not', () => {
  assert.ok(overlaps('Code', 'code.exe'));
  assert.ok(overlaps('chrome, code', 'Code'));
  assert.ok(overlaps('netflix.com', 'https://www.netflix.com/title'));
  assert.ok(!overlaps('code', 'chrome'));
  assert.ok(!overlaps('*', '*'));
});
