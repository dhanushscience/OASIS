// Old hashed bundles would pile up in ../ui/assets; remove them before each build.
import { rmSync } from 'node:fs';
rmSync(new URL('../../ui/assets', import.meta.url), { recursive: true, force: true });
