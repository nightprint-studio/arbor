// Writes Imber's documentation as plain Markdown, for readers that are not the app: another AI assistant
// (Claude Code, or any tool that can read files) that has to explain Imber to a user, or a person browsing the repo.
//
// The Svelte pages in `src/lib/components/bennu/docs/imber/` stay the single source of truth. This reads them,
// in the order and with the summaries/keywords/related pages of `imber-pages.json`, and writes
//
//   docs/imber/NN-<slug>.md   one page each, with a small front matter
//   docs/imber/llms.txt       an index in the llms.txt style: title, one-line description, link per page
//   docs/imber/ALL.md         every page in one file, for tools that want everything at once
//
// Output is deterministic (stable order, no timestamps) so a regeneration only produces a diff when a page changed.
// It runs at the end of `node scripts/build-help-index.mjs`, or alone:   node scripts/imber-docs-md.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const PAGES_DIR = 'src/lib/components/bennu/docs/imber';
const OUT_DIR = 'docs/imber';
const BANNER = 'Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`.';
const INDEX_INTRO =
  'Imber is the AI assistant built into the Bennu code editor (part of the Arbor suite). It runs on Claude with the user\'s own Anthropic API key and works on the open project through Bennu\'s own tools. These pages describe how it behaves, what it asks before doing, and every setting.';

const ENTITIES = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#39;': "'", '&nbsp;': ' ', '&rarr;': '→', '&larr;': '←', '&middot;': '·', '&hellip;': '…', '&mdash;': '—', '&ndash;': '–', '&rsaquo;': '›' };
const decode = (s) => s.replace(/&[#\w]+;/g, (m) => ENTITIES[m] ?? (m.startsWith('&#') ? String.fromCharCode(Number(m.slice(2, -1))) : m));

/** Inline markup to Markdown: keys joined into one code span, code, bold, italics; every other tag dropped. */
function inline(html) {
  let s = html
    .replace(/(?:<kbd>[^<]*<\/kbd>\s*\+\s*)*<kbd>[^<]*<\/kbd>/g, (run) => {
      const keys = [...run.matchAll(/<kbd>([^<]*)<\/kbd>/g)].map((m) => m[1].trim());
      return `\`${decode(keys.join('+'))}\``;
    })
    .replace(/<code>([\s\S]*?)<\/code>/g, (_m, c) => {
      const text = decode(c.replace(/<[^>]+>/g, ''));
      return text.includes('`') ? `\`\` ${text} \`\`` : `\`${text}\``;
    })
    .replace(/<strong>([\s\S]*?)<\/strong>/g, '**$1**')
    .replace(/<em>([\s\S]*?)<\/em>/g, '*$1*')
    .replace(/<br\s*\/?>/g, ' ')
    .replace(/<[^>]+>/g, '');
  return decode(s).replace(/\s+/g, ' ').trim();
}

function table(inner) {
  const rows = [...inner.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)].map((r) =>
    [...r[1].matchAll(/<t[dh][^>]*>([\s\S]*?)<\/t[dh]>/g)].map((c) => inline(c[1]).replace(/\|/g, '\\|')),
  );
  if (!rows.length) return '';
  const width = Math.max(...rows.map((r) => r.length));
  const pad = (r) => [...r, ...Array(width - r.length).fill('')];
  const line = (r) => `| ${pad(r).map((c) => c || ' ').join(' | ')} |`;
  return [line(rows[0]), `| ${Array(width).fill('---').join(' | ')} |`, ...rows.slice(1).map(line)].join('\n');
}

function list(inner, ordered) {
  return [...inner.matchAll(/<li[^>]*>([\s\S]*?)<\/li>/g)]
    .map((li, i) => `${ordered ? `${i + 1}.` : '-'} ${inline(li[1])}`)
    .join('\n');
}

/** A page's blocks (headings, paragraphs, lists, tables, code, callouts) as Markdown. */
function toMarkdown(source) {
  const html = source
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<span class="eyebrow">[\s\S]*?<\/span>/g, '');
  const blocks = [];
  const re = /<(h1|h2|h3|p|ul|ol|table|pre|Callout)\b([^>]*)>([\s\S]*?)<\/\1>/g;
  for (const m of html.matchAll(re)) {
    const [, tag, attrs, inner] = m;
    switch (tag) {
      case 'h1': blocks.push(`# ${inline(inner)}`); break;
      case 'h2': blocks.push(`## ${inline(inner)}`); break;
      case 'h3': blocks.push(`### ${inline(inner)}`); break;
      case 'p': blocks.push(inline(inner)); break;
      case 'ul': blocks.push(list(inner, false)); break;
      case 'ol': blocks.push(list(inner, true)); break;
      case 'table': blocks.push(table(inner)); break;
      case 'pre': blocks.push('```\n' + decode(inner.replace(/<[^>]+>/g, '')).trim() + '\n```'); break;
      case 'Callout': {
        const title = attrs.match(/title="([^"]*)"/)?.[1];
        const body = inline(inner);
        blocks.push(`> ${title ? `**${decode(title)}** ` : ''}${body}`);
        break;
      }
    }
  }
  return blocks.filter(Boolean).join('\n\n');
}

/** "Imber › Permissions and modes" in the prose becomes a link to that page's file. */
function linkPages(markdown, pages) {
  const byLabel = [...pages].sort((a, b) => b.label.length - a.label.length);
  return byLabel.reduce(
    (text, p) => text.split(`Imber › ${p.label}`).join(`[Imber › ${p.label}](${p.fileName})`),
    markdown,
  );
}

const yamlList = (items) => `[${items.map((i) => JSON.stringify(i)).join(', ')}]`;

export async function writeImberDocs(root) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, PAGES_DIR, 'imber-pages.json'), 'utf8'));
  const pages = manifest.map((p, i) => ({
    ...p,
    fileName: `${String(i).padStart(2, '0')}-${p.slug}.md`,
    source: `${PAGES_DIR}/${p.file}.svelte`,
  }));
  for (const p of pages) {
    p.body = toMarkdown(fs.readFileSync(path.join(root, p.source), 'utf8'));
    p.title = p.body.match(/^# (.*)$/m)?.[1] ?? p.label;
  }
  const titleOf = new Map(pages.map((p) => [p.id, p.title]));

  const outDir = path.join(root, OUT_DIR);
  fs.mkdirSync(outDir, { recursive: true });
  const keep = new Set([...pages.map((p) => p.fileName), 'llms.txt', 'ALL.md']);
  for (const f of fs.readdirSync(outDir)) {
    if (/^\d\d-.*\.md$/.test(f) && !keep.has(f)) fs.rmSync(path.join(outDir, f));
  }

  const write = (name, text) => fs.writeFileSync(path.join(outDir, name), text.replace(/\r\n/g, '\n').replace(/\n*$/, '\n'));

  for (const p of pages) {
    const related = p.related.map((id) => titleOf.get(id)).filter(Boolean);
    write(p.fileName, [
      '---',
      `title: ${JSON.stringify(p.title)}`,
      `summary: ${JSON.stringify(p.summary)}`,
      `keywords: ${yamlList(p.keywords)}`,
      `related: ${yamlList(related)}`,
      `source: ${p.source}`,
      '---',
      `<!-- ${BANNER} -->`,
      '',
      linkPages(p.body, pages),
    ].join('\n'));
  }

  write('llms.txt', [
    '# Imber',
    '',
    `> ${INDEX_INTRO}`,
    '',
    `<!-- ${BANNER} -->`,
    '',
    'Read the pages in this order, or open ALL.md for everything in one file. Each page starts with a one-sentence summary and stands on its own.',
    '',
    '## Docs',
    ...pages.map((p) => `- [${p.title}](${p.fileName}): ${p.summary}`),
    '',
    '## Optional',
    '- [All pages in one file](ALL.md): the same content, concatenated in reading order',
  ].join('\n'));

  write('ALL.md', [
    '# Imber documentation',
    '',
    `<!-- ${BANNER} -->`,
    '',
    `> ${INDEX_INTRO}`,
    '',
    '## Contents',
    ...pages.map((p) => `- ${p.title}: ${p.summary}`),
    '',
    ...pages.flatMap((p) => ['---', '', p.body, '']),
  ].join('\n'));

  console.log(`imber docs: ${pages.length} pages → ${path.relative(root, outDir)}/ (+ llms.txt, ALL.md)`);
}

// Run directly: regenerate without rebuilding the help index.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await writeImberDocs(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..'));
}
