/**
 * docs-export.ts
 * Converts the rendered DocsPanel HTML to a clean Markdown README.
 */

// ── Inline elements ──────────────────────────────────────────────────────────

// Decorative whitespace characters used in the rendered HTML (e.g. `&ensp;`,
// `&nbsp;`) survive `textContent` extraction verbatim, which leaves literal
// `<0x2002>`-looking codepoints in the exported markdown. Collapse them to
// regular ASCII spaces.
function normalizeText(s: string): string {
  return s
    .replace(/ /g, ' ')                                  // &nbsp;
    .replace(/[ -   　]/g, ' ')       // en/em/thin/hair/ideographic spaces
    .replace(/[​-‍﻿]/g, '');                   // zero-width
}

function inlineToMd(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return normalizeText(node.textContent ?? '');
  if (node.nodeType !== Node.ELEMENT_NODE) return '';

  const el   = node as HTMLElement;
  const tag  = el.tagName.toLowerCase();
  const inner = () => Array.from(el.childNodes).map(inlineToMd).join('');

  switch (tag) {
    case 'strong': case 'b': return `**${inner()}**`;
    case 'em':     case 'i': return `*${inner()}*`;
    case 'code':             return `\`${el.textContent ?? ''}\``;
    case 'kbd':              return `\`${el.textContent ?? ''}\``;
    case 'a': {
      const href = el.getAttribute('href') ?? '';
      return `[${inner()}](${href})`;
    }
    case 'br': return '\n';
    case 'span': {
      // Badge / eyebrow / chip-row → keep inline text, surrounded by backticks for emphasis
      if (el.classList.contains('badge') || el.classList.contains('eyebrow')) {
        const t = (el.textContent ?? '').trim();
        return t ? `\`${t}\`` : '';
      }
      return inner();
    }
    default:   return inner();
  }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

// Render an element's children as inline markdown, preserving `<code>`,
// `<strong>`, etc. Use this instead of `textContent` for any context (headers,
// table cells, list items) where inline formatting matters — `textContent`
// silently strips entity-encoded angle brackets (`<code>&lt;h1&gt;</code>`
// becomes the literal `<h1>`, which markdown then interprets as HTML).
function inlineChildren(el: Element): string {
  return Array.from(el.childNodes).map(inlineToMd).join('');
}

// ── Table ────────────────────────────────────────────────────────────────────

function tableToMd(table: HTMLTableElement): string {
  const rows = Array.from(table.querySelectorAll('tr'));
  if (rows.length === 0) return '';

  const cellText = (row: HTMLTableRowElement) =>
    Array.from(row.querySelectorAll('th, td')).map(
      cell => inlineChildren(cell).trim().replace(/\s+/g, ' ').replace(/\|/g, '\\|'),
    );

  const headers = cellText(rows[0] as HTMLTableRowElement);
  if (headers.length === 0) return '';

  const lines = [
    `| ${headers.join(' | ')} |`,
    `| ${headers.map(() => '---').join(' | ')} |`,
    ...rows.slice(1).map(r => `| ${cellText(r as HTMLTableRowElement).join(' | ')} |`),
  ];

  return lines.join('\n') + '\n\n';
}

// ── Block elements ───────────────────────────────────────────────────────────

function blockToMd(el: HTMLElement, listDepth = 0): string {
  const tag = el.tagName.toLowerCase();

  switch (tag) {
    case 'h1': return `# ${inlineChildren(el).trim()}\n\n`;
    case 'h2': return `## ${inlineChildren(el).trim()}\n\n`;
    case 'h3': return `### ${inlineChildren(el).trim()}\n\n`;
    case 'h4': return `#### ${inlineChildren(el).trim()}\n\n`;

    case 'p': {
      const text = Array.from(el.childNodes).map(inlineToMd).join('').trim();
      return text ? `${text}\n\n` : '';
    }

    case 'pre': {
      // Strip any Prism highlight spans — use raw textContent.
      const codeEl = el.querySelector('code');
      const text   = (codeEl ?? el).textContent ?? '';
      // Language hint: Prism-style `language-<lang>` class on the <pre> or
      // its inner <code>. Skips `language-none` / `language-plain`.
      const langCls = [...Array.from(el.classList), ...Array.from(codeEl?.classList ?? [])]
        .find(c => c.startsWith('language-'));
      let lang = langCls ? langCls.slice('language-'.length) : '';
      if (lang === 'none' || lang === 'plain') lang = '';
      return `\`\`\`${lang}\n${text.trimEnd()}\n\`\`\`\n\n`;
    }

    case 'ul': {
      const pad = '  '.repeat(listDepth);
      const items = Array.from(el.children)
        .filter(c => c.tagName.toLowerCase() === 'li')
        .map(li => {
          const text = Array.from(li.childNodes).map(inlineToMd).join('').trim();
          return `${pad}- ${text}`;
        });
      return items.join('\n') + '\n\n';
    }

    case 'ol': {
      const pad = '  '.repeat(listDepth);
      const items = Array.from(el.children)
        .filter(c => c.tagName.toLowerCase() === 'li')
        .map((li, i) => {
          const text = Array.from(li.childNodes).map(inlineToMd).join('').trim();
          return `${pad}${i + 1}. ${text}`;
        });
      return items.join('\n') + '\n\n';
    }

    case 'dl': {
      // Render dt/dd pairs as a 2-column Markdown table for readability.
      const children = Array.from(el.children);
      const rows: Array<[string, string]> = [];
      for (let i = 0; i < children.length; i++) {
        const c = children[i] as HTMLElement;
        if (c.tagName.toLowerCase() !== 'dt') continue;
        const key   = Array.from(c.childNodes).map(inlineToMd).join('').trim();
        const next  = children[i + 1] as HTMLElement | undefined;
        const value = next && next.tagName.toLowerCase() === 'dd'
          ? Array.from(next.childNodes).map(inlineToMd).join('').trim().replace(/\|/g, '\\|')
          : '';
        rows.push([key.replace(/\|/g, '\\|'), value]);
      }
      if (rows.length === 0) return '';
      return (
        '| Field | Value |\n| --- | --- |\n' +
        rows.map(([k, v]) => `| ${k} | ${v} |`).join('\n') +
        '\n\n'
      );
    }

    case 'table': return tableToMd(el as HTMLTableElement);

    case 'hr': return '---\n\n';

    // Transparent wrappers — but handle design-system classes specially
    case 'div': case 'section': case 'article': case 'details': {
      // Callout → blockquote. Use mixedContentToMd so loose text nodes
      // sitting between `<strong>` / `<code>` element siblings (the typical
      // callout shape) are preserved.
      if (el.classList.contains('callout')) {
        const inner = mixedContentToMd(el, listDepth).trim();
        if (!inner) return '';
        return inner.split('\n').map(l => `> ${l}`).join('\n') + '\n\n';
      }
      // Language-colour legend → compact comma-separated list (the grid
      // shape with one item per language renders as a wall of single-word
      // paragraphs in plain markdown).
      if (el.classList.contains('lang-legend-grid')) {
        const items = Array.from(el.querySelectorAll('.lang-legend-item'))
          .map(item => {
            const labelEl = Array.from(item.querySelectorAll('span'))
              .find(s => !s.classList.contains('lang-legend-dot'));
            return (labelEl?.textContent ?? item.textContent ?? '').trim();
          })
          .filter(Boolean);
        return items.length ? `Supported languages: ${items.join(', ')}.\n\n` : '';
      }
      // Feature grid → bulleted list of cards
      if (el.classList.contains('feature-grid')) {
        const cards = Array.from(el.querySelectorAll('.feature-card'));
        const items = cards.map(c => {
          const title = (c.querySelector('.fc-title')?.textContent ?? '').trim();
          const desc  = (c.querySelector('.fc-desc')?.textContent ?? '').trim().replace(/\s+/g, ' ');
          return title ? `- **${title}** — ${desc}` : '';
        }).filter(Boolean);
        return items.join('\n') + '\n\n';
      }
      // Individual card (when not inside a grid) → skip, handled above
      if (el.classList.contains('feature-card')) return '';
      // Hint → blockquote with a simple "Note:" prefix
      if (el.classList.contains('hint')) {
        const text = mixedContentToMd(el, listDepth).trim();
        return text ? `> ℹ ${text}\n\n` : '';
      }
      // Stat row → compact bullet list
      if (el.classList.contains('stat-row')) {
        const stats = Array.from(el.querySelectorAll('.stat'));
        const items = stats.map(s => {
          const v = (s.querySelector('.stat-value')?.textContent ?? '').trim();
          const l = (s.querySelector('.stat-label')?.textContent ?? '').trim();
          return l ? `- **${v}** — ${l}` : `- ${v}`;
        });
        return items.join('\n') + '\n\n';
      }
      // Indicator list items
      if (el.classList.contains('indicator-list')) {
        const items = Array.from(el.querySelectorAll('li')).map(li => {
          const text = Array.from(li.childNodes)
            .filter(n => !(n instanceof Element && n.classList.contains('ind')))
            .map(n => inlineToMd(n)).join('').trim();
          return `- ${text}`;
        });
        return items.join('\n') + '\n\n';
      }
      return childrenToMd(el, listDepth);
    }

    // Skip purely structural / invisible elements
    case 'script': case 'style': case 'head': return '';

    default: {
      // Unknown block-level: try to render inline content.
      const text = Array.from(el.childNodes).map(inlineToMd).join('').trim();
      return text ? `${text}\n\n` : '';
    }
  }
}

function childrenToMd(el: HTMLElement, listDepth = 0): string {
  return Array.from(el.children)
    .map(child => blockToMd(child as HTMLElement, listDepth))
    .join('');
}

// Tags that introduce their own block in markdown — anything else (strong,
// em, code, span, a, br, plus raw text) is treated as inline content.
const BLOCK_TAGS = new Set([
  'p', 'ul', 'ol', 'dl', 'pre', 'table', 'blockquote', 'hr',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'div', 'section', 'article', 'details',
]);

// Renders a container whose immediate children mix loose text nodes with
// element children (the typical shape of `.callout` / `.hint`). `childrenToMd`
// skips text nodes entirely, which silently drops the body of any callout
// whose authored markup puts its prose directly inside the wrapper instead of
// wrapping it in `<p>`.
function mixedContentToMd(parent: HTMLElement, listDepth = 0): string {
  let result    = '';
  let inlineBuf = '';
  const flush   = () => {
    const trimmed = inlineBuf.replace(/\s+/g, ' ').trim();
    if (trimmed) result += trimmed + '\n\n';
    inlineBuf = '';
  };

  for (const node of Array.from(parent.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) {
      inlineBuf += normalizeText(node.textContent ?? '');
      continue;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) continue;
    const el  = node as HTMLElement;
    const tag = el.tagName.toLowerCase();
    if (BLOCK_TAGS.has(tag)) {
      flush();
      result += blockToMd(el, listDepth);
    } else {
      inlineBuf += inlineToMd(el);
    }
  }
  flush();
  return result;
}

// ── HTML export ──────────────────────────────────────────────────────────────

const HTML_CSS = `
:root {
  --bg-base:      #1e1f22;
  --bg-elevated:  #25262a;
  --bg-card:      #2b2d31;
  --bg-code:      #18191c;
  --text-primary: #dcdfe4;
  --text-body:    #b5b8be;
  --text-muted:   #6b6f78;
  --accent:       #5c8fe8;
  --accent-dim:   rgba(92,143,232,0.15);
  --border:       #3a3c41;
  --border-sub:   #2e3035;
  --font-code:    'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
  --font-ui:      'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}

*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

body {
  background: var(--bg-elevated);
  color: var(--text-body);
  font-family: var(--font-ui);
  font-size: 14px;
  line-height: 1.7;
}

/* ── Layout ──
   Mirrors the in-app DocsPanel: bg-elevated page reveals as a 4px
   gap around floating bg-base panel cards (sidebar + main). */
.page    { display: flex; min-height: 100vh; padding: 4px; gap: 4px; }
.sidebar {
  position: sticky; top: 4px; height: calc(100vh - 8px); overflow-y: auto;
  width: 220px; flex-shrink: 0;
  background: var(--bg-base);
  border-radius: 12px;
  padding: 24px 0 32px;
  scrollbar-width: thin;
  scrollbar-color: var(--border) transparent;
}
.main {
  flex: 1; min-width: 0;
  background: var(--bg-base);
  border-radius: 12px;
  padding: 40px 56px 72px;
}

/* ── Sidebar nav ── */
.toc-title {
  font-size: 10px; font-weight: 700; letter-spacing: 0.8px;
  text-transform: uppercase; color: var(--text-muted);
  padding: 0 18px 10px;
}
.toc-group-label {
  font-size: 10px; font-weight: 700; letter-spacing: 0.6px;
  text-transform: uppercase; color: var(--text-muted);
  padding: 14px 18px 4px;
  border-top: 1px solid var(--border-sub);
  margin-top: 6px;
}
.toc-link {
  display: block; padding: 5px 18px;
  color: var(--text-muted); text-decoration: none;
  font-size: 12.5px;
  transition: color 0.15s;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.toc-link:hover { color: var(--text-body); }
.toc-link.child { padding-left: 30px; }

/* ── Section dividers ── */
.section { margin-bottom: 64px; }
.section-sep { border: none; border-top: 1px solid var(--border-sub); margin: 56px 0; }

/* ── Typography ── */
h1 {
  font-size: 22px; font-weight: 700; color: var(--text-primary);
  padding-bottom: 12px; border-bottom: 1px solid var(--border);
  margin-bottom: 18px;
}
h2 {
  font-size: 11px; font-weight: 700; letter-spacing: 0.7px;
  text-transform: uppercase; color: var(--text-muted);
  margin: 26px 0 10px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--border-sub);
}
h2 code { text-transform: none; letter-spacing: 0; font-size: 11px; }
h3 {
  font-size: 13px; font-weight: 700; color: var(--text-primary);
  margin: 18px 0 8px; letter-spacing: 0.1px;
}
h4 {
  font-size: 12px; font-weight: 600; color: var(--text-muted);
  margin: 14px 0 6px; text-transform: uppercase; letter-spacing: 0.4px;
}
p  { color: var(--text-body); margin-bottom: 12px; }
ul, ol { padding-left: 22px; margin-bottom: 14px; display: flex; flex-direction: column; gap: 5px; }
li { color: var(--text-body); }
strong { color: var(--text-primary); font-weight: 600; }
a  { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }

/* ── Code ── */
code {
  font-family: var(--font-code); font-size: 11.5px;
  background: var(--bg-card); color: var(--accent);
  padding: 1px 5px; border-radius: 4px;
}
kbd {
  font-family: var(--font-code); font-size: 10.5px;
  background: var(--bg-card); color: var(--text-primary);
  border: 1px solid var(--border); border-bottom-width: 2px;
  padding: 1px 6px; border-radius: 4px; white-space: nowrap;
}
pre {
  background: var(--bg-code);
  border: 1px solid var(--border-sub);
  border-radius: 6px; padding: 14px 16px;
  overflow-x: auto; margin-bottom: 16px;
}
pre code {
  background: none; padding: 0; color: var(--text-body);
  font-size: 12px; border-radius: 0;
}

/* ── Table ── */
table {
  width: 100%; border-collapse: collapse;
  font-size: 12.5px; margin-bottom: 16px;
}
th {
  text-align: left; padding: 7px 12px;
  font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: 0.5px;
  color: var(--text-muted);
  border-bottom: 1px solid var(--border);
}
td {
  padding: 7px 12px; color: var(--text-body);
  border-bottom: 1px solid var(--border-sub); vertical-align: top;
}
tr:last-child td { border-bottom: none; }

/* ── Scrollbar ── */
::-webkit-scrollbar { width: 6px; height: 6px; }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
::-webkit-scrollbar-track { background: transparent; }

/* ── Doc lead paragraph ── */
.doc-lead {
  font-size: 14.5px;
  border-left: 3px solid var(--accent);
  padding: 8px 0 8px 16px;
  margin-bottom: 20px;
  color: var(--text-body);
  line-height: 1.75;
}

/* ── Callout boxes ── */
.callout {
  padding: 12px 16px;
  border-radius: 0 6px 6px 0;
  border-left: 3px solid;
  margin: 14px 0;
  font-size: 13px;
  color: var(--text-body);
  line-height: 1.65;
}
.callout > strong:first-child {
  display: block;
  margin-bottom: 6px;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  font-weight: 700;
}
.callout.tip     { background: rgba(95,173,86,0.07);   border-color: rgba(95,173,86,0.5);  }
.callout.tip     > strong:first-child { color: #5fad56; }
.callout.info    { background: rgba(92,143,232,0.07);  border-color: rgba(92,143,232,0.45);}
.callout.info    > strong:first-child { color: var(--accent); }
.callout.warning { background: rgba(204,167,50,0.08);  border-color: rgba(204,167,50,0.5); }
.callout.warning > strong:first-child { color: #cca73a; }
.callout.danger  { background: rgba(204,77,77,0.07);   border-color: rgba(204,77,77,0.5);  }
.callout.danger  > strong:first-child { color: #cc4d4d; }

/* ── Feature grid ── */
.feature-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 10px;
  margin: 14px 0;
}
.feature-grid.two-col { grid-template-columns: repeat(2, 1fr); }
.feature-card {
  background: var(--bg-card);
  border: 1px solid var(--border-sub);
  border-radius: 8px;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.fc-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
.fc-desc  { font-size: 12px; color: var(--text-muted); line-height: 1.6; }

/* ── Step list ── */
ol.step-list {
  padding-left: 0;
  list-style: none;
  counter-reset: step-counter;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 14px 0;
}
ol.step-list > li {
  counter-increment: step-counter;
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 10px 16px 10px 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-sub);
  border-radius: 6px;
  font-size: 13px;
  color: var(--text-body);
  line-height: 1.6;
}
ol.step-list > li::before {
  content: counter(step-counter);
  flex-shrink: 0;
  width: 22px; height: 22px;
  background: var(--accent);
  color: white;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 700;
  margin-top: 1px;
}

/* ── Indicator legend ── */
.indicator-list {
  list-style: none;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 12px 0;
}
.indicator-list > li {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
  color: var(--text-body);
}
.ind {
  width: 13px; height: 13px;
  border-radius: 50%;
  flex-shrink: 0;
  display: inline-block;
}
.ind-bright  { background: var(--accent); box-shadow: 0 0 0 2px rgba(92,143,232,0.25); }
.ind-dimmed  { background: #444; }
.ind-head    { background: transparent; box-shadow: 0 0 0 2px var(--accent); }
.ind-merge   { background: var(--accent); border-radius: 2px; transform: rotate(45deg); width: 11px; height: 11px; }
.ind-wip     { background: transparent; border: 2px dashed #555; }
.ind-amber   { background: #cc9a3c; }

/* ── Branch chips ── */
.chip        { display: inline-block; font-family: var(--font-code); font-size: 11px; padding: 1px 7px; border-radius: 3px; font-weight: 500; }
.chip-local  { background: rgba(92,143,232,0.18);  color: var(--accent); }
.chip-remote { background: rgba(204,130,50,0.18);  color: #cc8432; }
.chip-tag    { background: rgba(155,100,200,0.18); color: #9876aa; }
.chip-head   { background: rgba(95,173,86,0.18);   color: #5fad56; }

/* ── Eyebrow ── */
.eyebrow {
  display: inline-flex; align-items: center; gap: 6px;
  font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: 0.7px;
  color: var(--accent); background: rgba(92,143,232,0.12);
  padding: 3px 8px; border-radius: 10px;
  margin: 0 0 6px;
}

/* ── Badges ── */
.badge {
  display: inline-flex; align-items: center; gap: 3px;
  font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: 0.4px;
  padding: 1px 6px; border-radius: 10px;
  line-height: 1.6; vertical-align: middle; white-space: nowrap;
}
.badge-req    { background: rgba(204,77,77,0.14);   color: #e27777; }
.badge-opt    { background: rgba(130,130,130,0.18); color: var(--text-muted); }
.badge-destr  { background: rgba(204,77,77,0.18);   color: #e27777; }
.badge-async  { background: rgba(155,100,200,0.18); color: #b588d0; }
.badge-new    { background: rgba(95,173,86,0.18);   color: #5fad56; }
.badge-beta   { background: rgba(204,167,50,0.18);  color: #cca73a; }
.badge-accent { background: rgba(92,143,232,0.18);  color: var(--accent); }

/* ── Meta grid ── */
dl.meta-grid {
  display: grid; grid-template-columns: max-content 1fr;
  margin: 12px 0 16px;
  background: var(--bg-card);
  border: 1px solid var(--border-sub);
  border-radius: 6px;
  padding: 4px 16px;
}
dl.meta-grid > dt {
  color: var(--text-muted); font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: 0.4px;
  padding: 9px 18px 9px 0;
  border-bottom: 1px dashed var(--border-sub);
  align-self: center;
}
dl.meta-grid > dd {
  color: var(--text-body); font-size: 13px;
  margin: 0; padding: 9px 0;
  border-bottom: 1px dashed var(--border-sub);
  line-height: 1.55;
}
dl.meta-grid > dt:last-of-type,
dl.meta-grid > dd:last-of-type { border-bottom: none; }

/* ── Prop list ──
 * Float-based layout so inline \`<code>\` / \`<strong>\` inside descriptions
 * don't get auto-placed into grid cells (which would shatter the row). */
ul.prop-list {
  list-style: none; padding: 0; margin: 12px 0 16px;
  display: flex; flex-direction: column; gap: 5px;
}
ul.prop-list > li {
  padding: 9px 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-sub);
  border-radius: 6px;
  font-size: 13px; color: var(--text-body);
  line-height: 1.55;
}
ul.prop-list > li::after { content: ""; display: block; clear: both; }
ul.prop-list > li > code:first-child,
ul.prop-list > li > strong:first-child {
  float: left;
  width: 140px;
  margin-right: 14px;
  color: var(--accent); font-size: 12px; font-weight: 700;
  padding-top: 1px;
}

/* ── Matrix table ── */
table.matrix td.yes     { color: #5fad56; font-weight: 700; text-align: center; }
table.matrix td.no      { color: var(--text-muted); text-align: center; }
table.matrix td.partial { color: #cca73a; font-weight: 700; text-align: center; }
table.matrix th:not(:first-child),
table.matrix td:not(:first-child) { text-align: center; }

/* ── Hint ── */
.hint {
  display: flex; align-items: flex-start; gap: 8px;
  padding: 8px 14px;
  background: rgba(92,143,232,0.06);
  border-left: 2px solid rgba(92,143,232,0.45);
  border-radius: 0 4px 4px 0;
  font-size: 12px; color: var(--text-body);
  margin: 10px 0; line-height: 1.55;
}
.hint::before {
  content: 'i'; color: var(--accent); font-weight: 700; font-style: italic;
  font-family: var(--font-code);
  flex-shrink: 0; width: 14px; height: 14px;
  background: rgba(92,143,232,0.18);
  border-radius: 50%;
  display: inline-flex; align-items: center; justify-content: center;
  font-size: 10px; margin-top: 2px;
}

/* ── Stat row ── */
.stat-row { display: flex; gap: 10px; flex-wrap: wrap; margin: 12px 0 16px; }
.stat {
  flex: 1 1 130px;
  background: var(--bg-card);
  border: 1px solid var(--border-sub);
  border-radius: 6px;
  padding: 12px 14px;
  display: flex; flex-direction: column; gap: 4px;
}
.stat-value { font-size: 17px; font-weight: 700; color: var(--text-primary); font-family: var(--font-code); }
.stat-label { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-muted); }

/* ── Feature card accent variant ── */
.feature-card.accent { border-top: 2px solid var(--accent); padding-top: 12px; }
.fc-eyebrow {
  font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: 0.5px;
  color: var(--accent); margin-bottom: -2px;
}

/* ── Divider ── */
.divider {
  display: flex; align-items: center; gap: 10px;
  margin: 20px 0 12px;
  font-size: 10px; font-weight: 700;
  text-transform: uppercase; letter-spacing: 0.7px;
  color: var(--text-muted);
}
.divider::before, .divider::after {
  content: ''; flex: 1; height: 1px; background: var(--border-sub);
}

/* ── Chip row ── */
.chip-row { display: inline-flex; flex-wrap: wrap; gap: 4px; align-items: center; vertical-align: middle; }

/* ── Prism token colours ── */
.token.comment, .token.prolog   { color: #6a9153; font-style: italic; }
.token.string, .token.attr-value { color: #6aab73; }
.token.keyword, .token.boolean  { color: #cc7832; }
.token.number                   { color: #6897bb; }
.token.function, .token.class-name { color: #ffc66d; }
.token.property, .token.attr-name  { color: #9876aa; }
.token.operator                 { color: #a9b7c6; }
.token.punctuation              { color: #6c6c6c; }
.token.builtin                  { color: #6897bb; }
`;

export interface HtmlSectionEntry {
  id:       string;
  label:    string;
  groupLabel?: string;   // set on first item of a group
  html:     string;
}

export function buildHtmlExport(
  sections: HtmlSectionEntry[],
  plugins:  PluginDocEntry[],
): string {
  // ── Sidebar TOC ───────────────────────────────────────────────────────────
  let tocHtml = `<p class="toc-title">Contents</p>\n`;
  for (const { id, label, groupLabel } of sections) {
    if (groupLabel) {
      tocHtml += `<p class="toc-group-label">${groupLabel}</p>\n`;
    }
    tocHtml += `<a class="toc-link" href="#${id}">${label}</a>\n`;
  }
  if (plugins.length > 0) {
    tocHtml += `<p class="toc-group-label">Plugins</p>\n`;
    for (const p of plugins) {
      tocHtml += `<a class="toc-link child" href="#plugin-${p.name}">${p.name}</a>\n`;
    }
  }

  // ── Section bodies ────────────────────────────────────────────────────────
  let bodyHtml = '';
  for (let i = 0; i < sections.length; i++) {
    const { id, html } = sections[i];
    bodyHtml += `<section class="section" id="${id}">\n${html}\n</section>\n`;
    if (i < sections.length - 1) bodyHtml += `<hr class="section-sep">\n`;
  }

  if (plugins.length > 0) {
    bodyHtml += `<hr class="section-sep">\n`;
    for (const p of plugins) {
      bodyHtml += `<section class="section" id="plugin-${p.name}">\n${p.doc}\n</section>\n`;
    }
  }

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Arbor — Documentation</title>
  <style>${HTML_CSS}</style>
</head>
<body>
<div class="page">
  <nav class="sidebar">\n${tocHtml}</nav>
  <main class="main">\n${bodyHtml}</main>
</div>
</body>
</html>
`;
}

// ── Public API ───────────────────────────────────────────────────────────────

/**
 * Convert an HTMLElement's children to Markdown.
 * Collapses runs of 3+ blank lines to at most 2.
 */
export function htmlElementToMarkdown(el: HTMLElement): string {
  return childrenToMd(el)
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

/**
 * Build the full README markdown from an ordered list of
 * { id, label, el } section entries plus optional plugin docs.
 */
export interface SectionEntry {
  id:    string;
  label: string;
  el:    HTMLElement;
}

export interface PluginDocEntry {
  name: string;
  doc:  string; // raw HTML string
}

export function buildReadme(
  sections: SectionEntry[],
  plugins:  PluginDocEntry[],
): string {
  const parts: string[] = [];

  // ── Header ────────────────────────────────────────────────────────────────
  parts.push(`# Arbor — Documentation\n\n`);
  parts.push(`> Auto-generated from the in-app documentation panel.\n\n`);

  // ── Table of contents ─────────────────────────────────────────────────────
  parts.push(`## Table of Contents\n\n`);
  for (const { label } of sections) {
    const anchor = label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
    parts.push(`- [${label}](#${anchor})\n`);
  }
  if (plugins.length > 0) {
    parts.push(`- [Plugins](#plugins)\n`);
    for (const p of plugins) {
      const anchor = `plugin-${p.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
      parts.push(`  - [${p.name}](#${anchor})\n`);
    }
  }
  parts.push('\n---\n\n');

  // ── Sections ──────────────────────────────────────────────────────────────
  for (const { el } of sections) {
    parts.push(htmlElementToMarkdown(el));
    parts.push('\n\n---\n\n');
  }

  // ── Plugin docs ───────────────────────────────────────────────────────────
  if (plugins.length > 0) {
    parts.push(`## Plugins\n\n`);
    for (const { name, doc } of plugins) {
      const wrapper = document.createElement('div');
      wrapper.innerHTML = doc;
      parts.push(`### ${name}\n\n`);
      parts.push(htmlElementToMarkdown(wrapper));
      parts.push('\n\n---\n\n');
    }
  }

  return parts.join('').replace(/\n{3,}/g, '\n\n').trimEnd() + '\n';
}
