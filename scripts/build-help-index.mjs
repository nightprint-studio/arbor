// Builds the index the assistant searches when someone asks how to do something in Bennu.
//
// Bennu's own documentation is a set of Svelte pages (`src/lib/components/bennu/docs/*.svelte`): the right
// place to write it, and unreadable to the backend, which has no Svelte. So this reads the pages, strips them
// to text, cuts each at its `<h2>` sections, and writes `crates/products/bennu/assistant/help.json`, which the
// backend embeds. The assistant then asks for the few sections that match a question, instead of being sent
// hundreds of kilobytes of documentation — or, worse, answering from a model that has never heard of Bennu.
//
// Run it after editing the docs, before building:   node scripts/build-help-index.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeImberDocs } from './imber-docs-md.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const docsDir = path.join(root, 'src/lib/components/bennu/docs');
const windowFile = path.join(root, 'src/lib/components/bennu/BennuWindow.svelte');
const out = path.join(root, 'crates/products/bennu/assistant/help.json');

const ENTITIES = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#39;': "'", '&nbsp;': ' ', '&rarr;': '→', '&larr;': '←', '&middot;': '·', '&hellip;': '…', '&mdash;': '—', '&ndash;': '–', '&rsaquo;': '›' };

function decode(s) {
  return s.replace(/&[#\w]+;/g, (m) => ENTITIES[m] ?? (m.startsWith('&#') ? String.fromCharCode(Number(m.slice(2, -1))) : m));
}

/** A page's markup as plain text: headings kept as markers, table rows as `a — b — c`, list items as bullets. */
function toText(html) {
  let s = html
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/\{@html[^}]*\}/g, '')
    .replace(/\{[#/:][^}]*\}/g, '')
    .replace(/<kbd>\s*([^<]*?)\s*<\/kbd>(\s*\+\s*)?/g, (_m, k, plus) => (plus ? `${k}+` : k))
    .replace(/<\/?(strong|em|code|b|i|span|a|abbr|small)(\s[^>]*)?>/g, '')
    .replace(/<h1[^>]*>([\s\S]*?)<\/h1>/g, '\n\u0001H1 $1\n')
    .replace(/<h2[^>]*>([\s\S]*?)<\/h2>/g, '\n\u0001H2 $1\n')
    .replace(/<h3[^>]*>([\s\S]*?)<\/h3>/g, '\n\u0001H3 $1\n')
    .replace(/<tr[^>]*>([\s\S]*?)<\/tr>/g, (_m, row) => {
      const cells = [...row.matchAll(/<t[dh][^>]*>([\s\S]*?)<\/t[dh]>/g)].map((c) => c[1].replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim());
      return `\n${cells.filter(Boolean).join(' — ')}\n`;
    })
    .replace(/<li[^>]*>/g, '\n• ')
    .replace(/<\/(p|div|ul|ol|table|thead|tbody|section|Callout)>/g, '\n')
    .replace(/<br\s*\/?>/g, '\n')
    .replace(/<[^>]+>/g, '');
  s = decode(s)
    .split('\n')
    .map((l) => l.replace(/[ \t]+/g, ' ').trim())
    .filter((l, i, a) => l !== '' || (a[i - 1] ?? '') !== '')
    .join('\n');
  return s.trim();
}

function chunksOf(page, text) {
  const out = [];
  let title = page;
  let buf = [];
  const flush = () => {
    const body = buf.join('\n').trim();
    if (body) out.push({ page, title, text: body });
    buf = [];
  };
  for (const line of text.split('\n')) {
    const m = line.match(/^\u0001H([123]) (.*)$/);
    if (m) {
      if (m[1] === '3' && buf.join('\n').length < 1500) { buf.push(`${m[2]}:`); continue; }
      flush();
      title = m[1] === '1' ? m[2] : m[2];
      continue;
    }
    buf.push(line);
  }
  flush();
  return out;
}

/** Every page under the docs folder, subfolders included (Imber's pages live in `imber/`), in a stable order. */
function pageFiles(dir, prefix = '') {
  return fs.readdirSync(dir, { withFileTypes: true })
    .flatMap((e) => (e.isDirectory() ? pageFiles(path.join(dir, e.name), `${prefix}${e.name}/`) : e.name.endsWith('.svelte') ? [prefix + e.name] : []))
    .sort();
}

const pages = pageFiles(docsDir);
const chunks = [];
for (const file of pages) {
  const html = fs.readFileSync(path.join(docsDir, file), 'utf8');
  const text = toText(html);
  const h1 = text.match(/^\u0001H1 (.*)$/m)?.[1];
  const page = h1 ?? file.replace(/\.svelte$/, '').replace(/([a-z])([A-Z])/g, '$1 $2');
  chunks.push(...chunksOf(page, text));
}

// The window's own layout: which button opens which panel, and its shortcut, read from the window's source
// so the map cannot drift from what the window really has.
const win = fs.readFileSync(windowFile, 'utf8');
const buttons = [...win.matchAll(/tooltip:\s*'([^']+)'(?:,\s*shortcut:\s*'([^']+)')?/g)]
  .map((m) => (m[2] ? `${m[1]} — ${m[2]}` : m[1]))
  .filter((v, i, a) => a.indexOf(v) === i);
chunks.push({
  page: 'Window layout',
  title: 'Where things are in the Bennu window',
  text: [
    'The Bennu window has a title bar on top, the editor in the middle with its tabs (the editor can be split into two panes), and a status bar at the bottom.',
    'Three rails of buttons sit along the edges. The left rail opens the project tree, the structure outline and the dependencies. The right rail opens the build tool (Maven or Cargo), the tests, the syntax trees, the database check and the assistant. The bottom rail opens the build output, run output, problems, TODO list, terminal, forms and the DTO lab. Pressing a button opens its panel; pressing it again closes it.',
    'Buttons and their shortcuts:',
    ...buttons.map((b) => `• ${b}`),
  ].join('\n'),
});

fs.writeFileSync(out, JSON.stringify(chunks));
const kb = Math.round(fs.statSync(out).size / 1024);
console.log(`help index: ${chunks.length} sections from ${pages.length} pages, ${kb} KB → ${path.relative(root, out)}`);

// The same pages, for readers that are not the app: Imber's pages as plain Markdown under docs/imber.
await writeImberDocs(root);
