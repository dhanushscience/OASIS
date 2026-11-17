// The guide's caption panel (../ui/guide.html) is written by hand and not built by Vite, so the two
// files it shares with the Svelte pages are copied beside it as they are: Mr. Science and his lines.
import { copyFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

for (const name of ['mrscience.js', 'lines.js']) {
  copyFileSync(fileURLToPath(new URL(`../src/guide/${name}`, import.meta.url)), fileURLToPath(new URL(`../../ui/${name}`, import.meta.url)));
}
