// Mr. Science, the guide: pixel art kept as text so it can be read, diffed and fixed by hand.
// One letter per pixel. No imports and no DOM, so it runs in a test and loads as it is in the overlay page.

/** 16 wide, 20 tall. `.` is empty; the letters are named in COLOURS. */
export const GRID = [
  '....OOOOOOOO....',
  '...OHHHHHHHHO...',
  '..OHHHHHHHHHHO..',
  '..OHHSSSSSSHHO..',
  '..OHSSSSSSSSHO..',
  '..OSWWESSWWESO..',
  '..OSWWESSWWESO..',
  '..OSSSSSSSSSSO..',
  '...OSSSMMSSSO...',
  '....OSSSSSSO....',
  '...OKKWTTWKKO...',
  '..OKKKWTTWKKKO..',
  '.OKKKKWTTWKKKKO.',
  '.OKSKKKTTKKKSKO.',
  '.OOOKKKKKKKKOOO.',
  '...OKKKKKKKKO...',
  '...OKKKOOKKKO...',
  '...OPPO..OPPO...',
  '...OBBO..OBBO...',
  '...OOOO..OOOO...',
];

// O outline (the one colour that follows the theme, so the black coat shows on a dark window),
// H hair, S skin, W lens and shirt, E eye, M mouth, K coat, T tie, P trousers, B boots.
const FILL = { H: '#4a3226', S: '#f3cba8', W: '#ffffff', E: '#141414', M: '#a34a3c', K: '#1d1d22', T: '#e8500f', P: '#3d424e', B: '#26262b' };
export const COLOURS = {
  light: { ...FILL, O: '#141414' },
  dark: { ...FILL, O: '#ededed' },
};

/** The sprite as an SVG string, `scale` screen pixels to one drawn pixel. Eyes carry the class `eye` so a page can blink them. */
export function svg(/** @type {'light' | 'dark'} */ theme = 'light', scale = 4) {
  const colours = /** @type {Record<string, string>} */ (COLOURS[theme] ?? COLOURS.light);
  let cells = '';
  GRID.forEach((row, y) => [...row].forEach((letter, x) => {
    if (letter !== '.') cells += `<rect x="${x}" y="${y}" width="1" height="1" fill="${colours[letter]}"${letter === 'E' ? ' class="eye"' : ''}/>`;
  }));
  const w = GRID[0].length, h = GRID.length;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w * scale}" height="${h * scale}" shape-rendering="crispEdges" role="img" aria-label="Mr. Science">${cells}</svg>`;
}
