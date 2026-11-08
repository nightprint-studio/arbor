/**
 * Markdown documentation, rendered for the hover card.
 *
 * The card's other renderer reads Javadoc, which is HTML. A language server (rust-analyzer) and
 * the Patina package answer in **markdown** instead — `///` comments with backticks, `**`, fenced
 * examples and `# Examples` headings — and read as Javadoc those marks came through as typed: the
 * reader got `` `x` `` and three backticks around every example. The engine that produced the doc
 * says which of the two it is (`doc_format` on the wire); this file is the markdown half.
 *
 * Two layers, on purpose. {@link parseMarkdown} turns the text into a small tree and touches no
 * DOM, so it is what the tests exercise. {@link renderMarkdown} walks that tree into elements.
 *
 * ## The subset
 *
 * What documentation uses, not all of CommonMark: paragraphs, ATX and `===` headings, fenced and
 * indented code, bullet and ordered lists (nested), block quotes, rules; inline code, strong,
 * emphasis, strikethrough, links, autolinks, images (as their alt text), hard breaks, backslash
 * escapes and entities. Tables and raw HTML blocks are not rendered — they stay as their text, which
 * is legible, rather than as a guess.
 *
 * ## Nothing here is markup that runs
 *
 * The same rule as the Javadoc renderer: no `innerHTML`, every element created by name, every
 * string through `textContent`. A link is styled text, never an `<a>` — a click in a Tauri webview
 * navigates the app itself.
 */

import { highlightToDom } from '$lib/utils/highlight';
import { fenceGrammarName } from '$lib/utils/fence-info';
import { decodeEntities } from './doc-entities';

// ── The tree ─────────────────────────────────────────────────────────────────────

export type MdInline =
  | { t: 'text'; text: string }
  | { t: 'code'; text: string }
  | { t: 'strong' | 'em' | 'del' | 'link'; children: MdInline[] }
  | { t: 'br' };

export type MdBlock =
  | { t: 'p'; inlines: MdInline[] }
  | { t: 'heading'; inlines: MdInline[] }
  | { t: 'code'; lang: string | null; text: string }
  | { t: 'list'; ordered: boolean; start: number; items: MdBlock[][] }
  | { t: 'quote'; blocks: MdBlock[] }
  | { t: 'hr' };

// ── Blocks ───────────────────────────────────────────────────────────────────────

const FENCE = /^( {0,3})(`{3,}|~{3,})(.*)$/;
const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE = /^ {0,3}> ?(.*)$/;
const ITEM = /^( {0,3})([-*+]|(\d{1,9})[.)])([ \t]+(.*))?$/;
const SETEXT = /^ {0,3}=+[ \t]*$/;
const REF_DEF = /^ {0,3}\[[^\]]+\]:[ \t]*\S/;
const INDENTED = /^(?: {4}|\t)/;

const blank = (l: string) => l.trim() === '';

/** Whether `line` opens a block that ends the paragraph above it. */
function interrupts(line: string): boolean {
  if (FENCE.test(line) || RULE.test(line) || QUOTE.test(line)) return true;
  if (HEADING.test(line)) return true;
  // A list item interrupts a paragraph only when it has content — `-` alone is more likely text.
  const item = ITEM.exec(line);
  return !!item && !!item[5]?.trim();
}

/** Parse markdown into blocks. Pure: no DOM, safe to call anywhere. */
export function parseMarkdown(src: string): MdBlock[] {
  return parseBlocks(src.replace(/\r\n?/g, '\n').split('\n'));
}

function parseBlocks(lines: string[]): MdBlock[] {
  const out: MdBlock[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (blank(line)) {
      i++;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence && !(fence[2][0] === '`' && fence[3].includes('`'))) {
      const [, indent, marker, info] = fence;
      const body: string[] = [];
      i++;
      while (i < lines.length) {
        const close = /^ {0,3}(`{3,}|~{3,})[ \t]*$/.exec(lines[i]);
        if (close && close[1][0] === marker[0] && close[1].length >= marker.length) break;
        body.push(dedent(lines[i], indent.length));
        i++;
      }
      i++; // the closing fence; past the end when it was never closed, which keeps the body
      out.push({ t: 'code', lang: info.trim() || null, text: body.join('\n') });
      continue;
    }

    if (RULE.test(line)) {
      out.push({ t: 'hr' });
      i++;
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      out.push({ t: 'heading', inlines: parseInline(heading[2] ?? '') });
      i++;
      continue;
    }

    if (QUOTE.test(line)) {
      const inner: string[] = [];
      while (i < lines.length && !blank(lines[i])) {
        const q = QUOTE.exec(lines[i]);
        inner.push(q ? q[1] : lines[i]); // a lazy continuation line belongs to the quote
        i++;
      }
      out.push({ t: 'quote', blocks: parseBlocks(inner) });
      continue;
    }

    if (ITEM.test(line)) {
      const [list, next] = parseList(lines, i);
      out.push(list);
      i = next;
      continue;
    }

    if (INDENTED.test(line)) {
      const body: string[] = [];
      while (i < lines.length && (INDENTED.test(lines[i]) || blank(lines[i]))) {
        body.push(lines[i].replace(INDENTED, ''));
        i++;
      }
      while (body.length && blank(body[body.length - 1])) body.pop();
      out.push({ t: 'code', lang: null, text: body.join('\n') });
      continue;
    }

    // A reference definition is where a link points, not text: the link itself carries the label.
    if (REF_DEF.test(line)) {
      i++;
      continue;
    }

    // A paragraph: everything up to a blank line or a line that opens another block. An `===`
    // underline makes it a heading; a `---` one stays a rule, which in a doc is what it means.
    const para: string[] = [line];
    let underlined = false;
    i++;
    while (i < lines.length && !blank(lines[i]) && !interrupts(lines[i])) {
      if (SETEXT.test(lines[i])) {
        underlined = true;
        i++;
        break;
      }
      para.push(lines[i]);
      i++;
    }
    out.push({ t: underlined ? 'heading' : 'p', inlines: parseInline(para.join('\n')) });
  }
  return out;
}

/** A list starting at `lines[from]`: its items, and the index of the first line after it. */
function parseList(lines: string[], from: number): [MdBlock, number] {
  const first = ITEM.exec(lines[from])!;
  const ordered = first[3] !== undefined;
  const bullet = first[2].slice(-1);
  const items: MdBlock[][] = [];
  let i = from;

  while (i < lines.length) {
    const m = ITEM.exec(lines[i]);
    if (!m || (m[3] !== undefined) !== ordered || m[2].slice(-1) !== bullet) break;
    // Content lines of an item are indented to where its text starts.
    const width = m[1].length + m[2].length + Math.max(1, Math.min(4, (m[4] ?? ' ').length - (m[5] ?? '').length));
    const body: string[] = [m[5] ?? ''];
    i++;
    while (i < lines.length) {
      const l = lines[i];
      if (blank(l)) {
        // A blank line continues the item only if what follows is indented into it.
        const nextText = lines.slice(i + 1).find((x) => !blank(x));
        if (nextText !== undefined && indentOf(nextText) >= width) {
          body.push('');
          i++;
          continue;
        }
        break;
      }
      if (indentOf(l) >= width) {
        body.push(dedent(l, width));
        i++;
        continue;
      }
      // A lazy continuation of the item's paragraph — but not the next item or another block.
      if (ITEM.test(l) || interrupts(l)) break;
      body.push(l);
      i++;
    }
    items.push(parseBlocks(body));
    // Blank lines between items keep the list going.
    while (i < lines.length && blank(lines[i]) && lines[i + 1] !== undefined && ITEM.test(lines[i + 1])) i++;
  }
  return [{ t: 'list', ordered, start: ordered ? Number(first[3]) : 1, items }, i];
}

function indentOf(line: string): number {
  let n = 0;
  for (const c of line) {
    if (c === ' ') n++;
    else if (c === '\t') n += 4 - (n % 4);
    else break;
  }
  return n;
}

/** Take up to `n` columns of leading whitespace off `line`. */
function dedent(line: string, n: number): string {
  let i = 0;
  let col = 0;
  while (i < line.length && col < n && (line[i] === ' ' || line[i] === '\t')) {
    col += line[i] === '\t' ? 4 - (col % 4) : 1;
    i++;
  }
  return line.slice(i);
}

// ── Inlines ──────────────────────────────────────────────────────────────────────

const PUNCT = /[!-/:-@[-`{-~]/;
const ALNUM = /[\p{L}\p{N}]/u;

/** Parse one paragraph's text into inline nodes. */
export function parseInline(src: string): MdInline[] {
  const out: MdInline[] = [];
  let text = '';
  const flush = () => {
    if (text) out.push({ t: 'text', text: decodeEntities(text) });
    text = '';
  };

  let i = 0;
  while (i < src.length) {
    const c = src[i];

    if (c === '\\') {
      const next = src[i + 1];
      if (next === '\n') {
        flush();
        out.push({ t: 'br' });
        i += 2;
        continue;
      }
      if (next !== undefined && PUNCT.test(next)) {
        // Escaped: the character as itself, kept out of entity decoding.
        flush();
        out.push({ t: 'text', text: next });
        i += 2;
        continue;
      }
    }

    if (c === '`') {
      const code = codeSpan(src, i);
      if (code) {
        flush();
        out.push({ t: 'code', text: code.text });
        i = code.end;
        continue;
      }
      // An unmatched run of backticks is literal, all of it.
      const run = /^`+/.exec(src.slice(i))![0];
      text += run;
      i += run.length;
      continue;
    }

    if (c === '\n') {
      // Two trailing spaces are a hard break; any other newline is a space.
      const hard = text.endsWith('  ');
      text = text.replace(/[ \t]+$/, '');
      if (hard) {
        flush();
        out.push({ t: 'br' });
      } else {
        text += ' ';
      }
      i++;
      continue;
    }

    if (c === '!' && src[i + 1] === '[') {
      const link = linkAt(src, i + 1);
      if (link) {
        // An image in a tooltip is its alt text.
        text += plainOf(parseInline(link.label));
        i = link.end;
        continue;
      }
    }

    if (c === '[') {
      const link = linkAt(src, i);
      if (link) {
        flush();
        out.push({ t: 'link', children: parseInline(link.label) });
        i = link.end;
        continue;
      }
    }

    if (c === '<') {
      const auto = /^<((?:https?|ftp|mailto):[^\s<>]+)>/i.exec(src.slice(i));
      if (auto) {
        flush();
        out.push({ t: 'link', children: [{ t: 'text', text: auto[1] }] });
        i += auto[0].length;
        continue;
      }
      const br = /^<br\s*\/?>/i.exec(src.slice(i));
      if (br) {
        flush();
        out.push({ t: 'br' });
        i += br[0].length;
        continue;
      }
    }

    if (c === '*' || c === '_' || c === '~') {
      const em = emphasisAt(src, i);
      if (em) {
        flush();
        out.push({ t: em.kind, children: parseInline(em.inner) });
        i = em.end;
        continue;
      }
    }

    text += c;
    i++;
  }
  flush();
  return out;
}

/** The code span opening at `src[at]`: its text and where it ends. `null` when unmatched. */
function codeSpan(src: string, at: number): { text: string; end: number } | null {
  const run = /^`+/.exec(src.slice(at))![0];
  let j = at + run.length;
  while (j < src.length) {
    const k = src.indexOf(run, j);
    if (k < 0) return null;
    // The closing run must be exactly as long — a longer one is not it.
    let e = k + run.length;
    if (src[e] === '`' || src[k - 1] === '`') {
      while (src[e] === '`') e++;
      j = e;
      continue;
    }
    let text = src.slice(at + run.length, k).replace(/\n/g, ' ');
    if (text.length > 2 && text.startsWith(' ') && text.endsWith(' ') && text.trim()) text = text.slice(1, -1);
    return { text, end: e };
  }
  return null;
}

/** Skip a code span or an escape at `j` so a delimiter search does not look inside it. */
function skipOpaque(src: string, j: number): number {
  if (src[j] === '\\') return j + 2;
  if (src[j] === '`') {
    const code = codeSpan(src, j);
    if (code) return code.end;
    return j + /^`+/.exec(src.slice(j))![0].length;
  }
  return j + 1;
}

/**
 * The link whose `[` is at `src[at]`: its label and where it ends.
 *
 * Inline `[label](url)`, full and collapsed references `[label][ref]` / `[label][]`, and the
 * shortcut `[label]` — but a shortcut only when its label is code (`` [`Vec`] ``): that is
 * rustdoc's intra-doc link, while a bracketed word in prose is just a bracketed word.
 */
function linkAt(src: string, at: number): { label: string; end: number } | null {
  let depth = 0;
  let j = at;
  let close = -1;
  while (j < src.length) {
    const ch = src[j];
    if (ch === '\\' || ch === '`') {
      j = skipOpaque(src, j);
      continue;
    }
    if (ch === '[') depth++;
    else if (ch === ']' && --depth === 0) {
      close = j;
      break;
    }
    j++;
  }
  if (close < 0) return null;
  const label = src.slice(at + 1, close);
  const after = src.slice(close + 1);

  const inline = /^\((?:[^()\s]|\([^()\s]*\))*(?:\s+(?:"[^"]*"|'[^']*'|\([^)]*\)))?\s*\)/.exec(after);
  if (inline) return { label, end: close + 1 + inline[0].length };
  const ref = /^\[[^\]]*\]/.exec(after);
  if (ref) return { label, end: close + 1 + ref[0].length };
  if (/^`[^`]+`$/.test(label.trim())) return { label, end: close + 1 };
  return null;
}

/**
 * The emphasis opening at `src[at]`: which kind, its inner text and where it ends.
 *
 * Deliberately simpler than CommonMark's delimiter algorithm, and sufficient for prose: an opener
 * is followed by non-space, a closer is preceded by non-space, and `_` never opens or closes inside
 * a word — so `snake_case_name` stays one name, which in documentation is the case that matters.
 */
function emphasisAt(src: string, at: number): { kind: 'strong' | 'em' | 'del'; inner: string; end: number } | null {
  const c = src[at];
  const double = src[at + 1] === c;
  if (c === '~' && !double) return null;
  const delim = double ? c + c : c;
  const kind = c === '~' ? 'del' : double ? 'strong' : 'em';

  const afterOpen = src[at + delim.length];
  if (afterOpen === undefined || /\s/.test(afterOpen)) return null;
  if (c === '_' && at > 0 && ALNUM.test(src[at - 1])) return null;

  let j = at + delim.length;
  while (j < src.length) {
    if (src[j] === '\\' || src[j] === '`') {
      j = skipOpaque(src, j);
      continue;
    }
    if (src.startsWith(delim, j) && j > at + delim.length) {
      const before = src[j - 1];
      const after = src[j + delim.length];
      const closes = !/\s/.test(before)
        && !(c === '_' && after !== undefined && ALNUM.test(after))
        // A single `*` touching another `*` is half of a `**`, not a closer.
        && !(!double && (src[j + 1] === c || before === c));
      if (closes) return { kind, inner: src.slice(at + delim.length, j), end: j + delim.length };
      j += delim.length;
      continue;
    }
    // A strong span may hold an emphasis, so step over a nested run whole.
    j++;
  }
  return null;
}

/** The text of inline nodes, with their marks dropped. */
function plainOf(nodes: MdInline[]): string {
  return nodes
    .map((n) => (n.t === 'text' || n.t === 'code' ? n.text : n.t === 'br' ? ' ' : plainOf(n.children)))
    .join('');
}

// ── DOM ──────────────────────────────────────────────────────────────────────────

/**
 * The fence words that are rustdoc **attributes**, not a language: ```` ```no_run ```` is a Rust
 * example that does not run, and reading it as the language `no_run` would leave it uncoloured.
 */
const RUSTDOC_ATTRIBUTES = new Set([
  'ignore', 'no_run', 'should_panic', 'compile_fail', 'test_harness', 'standalone_crate',
  'edition2015', 'edition2018', 'edition2021', 'edition2024',
]);

/** The grammar a code block is coloured with: what its fence names, else the documented language. */
function blockLanguage(info: string | null, fallback: string | null | undefined): string | null {
  const named = fenceGrammarName(info);
  if (!named || RUSTDOC_ATTRIBUTES.has(named)) return fallback ?? null;
  return named;
}

/**
 * Render markdown documentation into `root`, as DOM.
 *
 * `codeLanguage` colours a code block whose fence names no language — the language of the thing
 * documented, as with the Javadoc renderer.
 */
export function renderMarkdown(root: HTMLElement, src: string, codeLanguage?: string | null): void {
  appendBlocks(root, parseMarkdown(src), codeLanguage);
}

function appendBlocks(parent: HTMLElement, blocks: MdBlock[], codeLanguage?: string | null): void {
  for (const b of blocks) parent.appendChild(blockDom(b, codeLanguage));
}

function blockDom(b: MdBlock, codeLanguage?: string | null): HTMLElement {
  switch (b.t) {
    case 'p': {
      const p = document.createElement('p');
      appendInlines(p, b.inlines);
      return p;
    }
    case 'heading': {
      // One heading size for every level, as the Javadoc renderer does: a tooltip is skimmed.
      const h = document.createElement('h4');
      appendInlines(h, b.inlines);
      return h;
    }
    case 'code': {
      const pre = document.createElement('pre');
      pre.className = 'cm-hc-pre';
      pre.appendChild(highlightToDom(b.text, blockLanguage(b.lang, codeLanguage)));
      return pre;
    }
    case 'list': {
      const list = document.createElement(b.ordered ? 'ol' : 'ul');
      if (b.ordered && b.start !== 1) (list as HTMLOListElement).start = b.start;
      for (const item of b.items) {
        const li = document.createElement('li');
        // A one-paragraph item is the common case, and as a `<p>` it would carry a paragraph's
        // spacing between every bullet.
        const [head, ...rest] = item;
        if (head?.t === 'p') {
          appendInlines(li, head.inlines);
          appendBlocks(li, rest, codeLanguage);
        } else {
          appendBlocks(li, item, codeLanguage);
        }
        list.appendChild(li);
      }
      return list;
    }
    case 'quote': {
      const q = document.createElement('blockquote');
      appendBlocks(q, b.blocks, codeLanguage);
      return q;
    }
    case 'hr':
      return document.createElement('hr');
  }
}

function appendInlines(parent: HTMLElement, nodes: MdInline[]): void {
  for (const n of nodes) {
    switch (n.t) {
      case 'text':
        parent.appendChild(document.createTextNode(n.text));
        break;
      case 'code': {
        const code = document.createElement('code');
        code.textContent = n.text;
        parent.appendChild(code);
        break;
      }
      case 'br':
        parent.appendChild(document.createElement('br'));
        break;
      case 'link': {
        // Text, never a link — see the module doc.
        const a = document.createElement('span');
        a.className = 'cm-hc-anchor';
        appendInlines(a, n.children);
        parent.appendChild(a);
        break;
      }
      default: {
        const el = document.createElement(n.t === 'strong' ? 'strong' : n.t === 'em' ? 'em' : 's');
        appendInlines(el, n.children);
        parent.appendChild(el);
      }
    }
  }
}
