import test from 'node:test';
import assert from 'node:assert/strict';
import { GRID, COLOURS, svg } from './mrscience.js';

test('every row is as wide as the first and uses only known letters', () => {
  for (const [theme, colours] of Object.entries(COLOURS)) {
    GRID.forEach((row, y) => {
      assert.equal(row.length, GRID[0].length, `row ${y}`);
      for (const letter of row) assert.ok(letter === '.' || letter in colours, `row ${y}: "${letter}" has no ${theme} colour`);
    });
  }
});

// The window's --surface in each theme (tokens.css). His coat is black, so the outline is what must show.
const SURFACE = { light: '#F8FCFF', dark: '#141414' };
const luminance = (/** @type {string} */ hex) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

test('his outline stands out from the window in both themes (at least 3 to 1)', () => {
  for (const theme of /** @type {const} */ (['light', 'dark'])) {
    const [a, b] = [luminance(COLOURS[theme].O), luminance(SURFACE[theme])].sort((x, y) => y - x);
    assert.ok((a + 0.05) / (b + 0.05) >= 3, theme);
  }
});

test('the drawing has two eyes a page can blink', () => {
  assert.equal(svg('light').split('class="eye"').length - 1, 4); // two pixels each
});
