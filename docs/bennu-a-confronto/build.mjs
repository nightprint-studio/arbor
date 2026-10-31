// node docs/bennu-a-confronto/build.mjs  →  docs/bennu-a-confronto.html
// Reads data.json and style.css, writes the page. See README.md for how the data is laid out.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderPage } from './lib/render.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const data = JSON.parse(readFileSync(join(here, 'data.json'), 'utf8'));
const style = readFileSync(join(here, 'style.css'), 'utf8');

const out = process.argv[2] ?? join(here, '..', 'bennu-a-confronto.html');
writeFileSync(out, renderPage(data, style));
console.log(`wrote ${out}`);
