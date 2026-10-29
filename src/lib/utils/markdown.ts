// Lightweight Markdown → HTML renderer used by issue trackers (Linear/Jira)
// and merge-request views (GitHub PRs / GitLab MRs).
//
// Supports the subset that issue/PR descriptions actually use:
//   • Fenced code blocks (with Prism syntax highlighting when available)
//   • Inline code, bold (**), italic (*), links [text](url)
//   • Headings (# ## ###)
//   • Blockquotes (> )
//   • Unordered lists (- / *) and ordered lists (1. )
//   • Horizontal rule (--- / *** / ___)
//   • A safelist of inline/block HTML tags (GitHub PR bodies and Dependabot
//     descriptions are HTML-heavy: <details>/<summary>, <p>, <blockquote>,
//     <code>, <em>, <a>, <ul>/<li>, etc.). Dangerous tags (<script>, <style>,
//     <iframe>) and event handlers are stripped; <a> is rewritten to a
//     non-clickable <span class="md-link"> to stay consistent with how the
//     markdown [text](url) syntax is handled.
//
// Output classes (md-pre, md-code, md-inline-code, md-h1…h3, md-p, md-bq,
// md-ul, md-ol, md-hr, md-link, md-spacer) are styled per-modal — see
// `IssueDetailModal.svelte` and `MrModal.svelte`.

import Prism from 'prismjs';
// Load every grammar Arbor supports onto `Prism.languages`. Without this the
// renderer would only highlight whichever languages other modules happened
// to have already imported (DiffViewer, highlight.ts, …) — so the same
// Dependabot description could render with rust highlighting one minute and
// plain text the next, depending on what the user had visited first.
import './prism-shared';
import { replaceEmojiShortcodes } from './emoji';

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * Best-effort language detection for fenced blocks that arrive *without* a
 * language tag (Dependabot / GitLab Security Bot descriptions, raw issue
 * bodies pasted by humans, etc.). We only return a language that is actually
 * registered on `Prism.languages` (the grammars are bulk-imported via
 * `./prism-shared` at the top of this file) — otherwise highlighting would
 * silently fall through to plain text and the function would be pointless.
 *
 * The heuristic is intentionally cheap — first match wins. Add new branches
 * only when a real provider description forces our hand.
 */
function detectLang(code: string): string | null {
  const head = code.trimStart().slice(0, 400);
  if (!head) return null;

  // TOML — `[section]` (or `[[array]]`) followed by `key = value` lines.
  if (/^\[\[?[A-Za-z][\w.-]*\]?\]\s*\n[\s\S]*?[A-Za-z_][\w-]*\s*=/.test(head)) return 'toml';

  // Rust — distinctive top-level keywords / attribute syntax / `&[u8]`.
  if (
    /^\s*(fn|use|let|pub|mut|impl|struct|enum|trait|mod|const|static|unsafe|async)\b/.test(head)
    || /#!?\[[a-z_:]+/.test(head)
    || /&\[\s*u8\s*]/.test(head)
    || /::[A-Za-z_]/.test(head)
  ) return 'rust';

  // JSON — opening brace/bracket followed by a quoted key.
  if (/^\s*[{\[][\s\S]*?"[\w-]+"\s*:/.test(head)) return 'json';

  // YAML — top-level `key: value` lines, optional leading `---`.
  if (/^---\s*\n/.test(head) || /^[A-Za-z_][\w-]*:\s+\S/.test(head)) return 'yaml';

  // Shell — shebang, common CLI prefixes.
  if (/^#!\/(?:usr\/)?bin\/(?:env\s+)?(?:bash|sh|zsh)\b/.test(head)) return 'bash';
  if (/^\s*\$\s+\S/.test(head) || /^\s*(npm|yarn|pnpm|cargo|git|docker|kubectl)\s+/.test(head)) return 'bash';

  // TypeScript / JavaScript — imports, type declarations, arrow functions.
  if (/^\s*(import|export|type|interface|const|let|var|function|class|async)\b/.test(head)) {
    return /:\s*[A-Za-z_][\w.<>,\s|&[\]]*\s*[=){,;]/.test(head) || /\binterface\b|\btype\s+\w+\s*=/.test(head)
      ? 'typescript'
      : 'javascript';
  }

  // Markup — looks like HTML/XML.
  if (/^\s*<\/?[A-Za-z][\w-]*(\s|>)/.test(head)) return 'markup';

  return null;
}

const ALLOWED_HTML_TAGS = new Set([
  'a','abbr','b','blockquote','br','code','del','details','div','em',
  'h1','h2','h3','h4','h5','h6','hr','i','img','ins','kbd','li','mark',
  'ol','p','pre','q','s','small','span','strong','sub','summary','sup',
  'table','tbody','td','tfoot','th','thead','tr','u','ul',
]);

const BLOCK_HTML_TAGS = new Set([
  'blockquote','details','div','h1','h2','h3','h4','h5','h6','hr',
  'li','ol','p','pre','summary','table','tbody','td','tfoot','th','thead','tr','ul',
]);

function sanitizeHtml(s: string): string {
  s = s.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '');
  s = s.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '');
  s = s.replace(/<iframe\b[^>]*>[\s\S]*?<\/iframe>/gi, '');
  s = s.replace(/\son\w+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, '');
  s = s.replace(/<a\b[^>]*>/gi, '<span class="md-link">');
  s = s.replace(/<\/a\s*>/gi, '</span>');
  // `<img>` is intentionally NOT stripped here — it's normalized for inline
  // preview by `prepareImagesForPreview` (called at the end of `renderMarkdown`).
  return s;
}

function attrEsc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function attrDecode(s: string): string {
  return s
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

/** Inline-image source we're willing to render: http(s), protocol-/root-relative
 *  (resolved backend-side per provider), or a self-contained `data:image` URL.
 *  Everything else (javascript:, file:, …) is dropped. */
function isSafeImgUrl(u: string): boolean {
  const t = u.trim();
  return /^https?:\/\//i.test(t)
    || /^\/\//.test(t)
    || /^\//.test(t)
    || /^data:image\//i.test(t);
}

/**
 * Normalize every `<img>` in rendered HTML into the deferred-load form the
 * `previewImages` action understands: the source moves to `data-img-src` so the
 * WebView never tries to load a (possibly auth-gated) URL directly — the action
 * fetches it through the provider proxy and swaps in the result. `data:` images
 * keep a direct `src` (already self-contained, nothing to proxy). Unsafe schemes
 * are dropped.
 *
 * Exported so the issue detail modal can run it over Jira's pre-rendered HTML
 * descriptions/comments, which don't pass through `renderMarkdown`.
 */
export function prepareImagesForPreview(html: string): string {
  return html.replace(/<img\b([^>]*)>/gi, (_m, attrs: string) => {
    const rawSrc =
      attrs.match(/\bsrc\s*=\s*"([^"]*)"/i)?.[1]
      ?? attrs.match(/\bsrc\s*=\s*'([^']*)'/i)?.[1]
      ?? attrs.match(/\bdata-img-src\s*=\s*"([^"]*)"/i)?.[1]
      ?? '';
    const rawAlt =
      attrs.match(/\balt\s*=\s*"([^"]*)"/i)?.[1]
      ?? attrs.match(/\balt\s*=\s*'([^']*)'/i)?.[1]
      ?? '';
    const src = attrDecode(rawSrc);
    if (!src || !isSafeImgUrl(src)) return '';
    const altAttr = rawAlt ? ` alt="${attrEsc(attrDecode(rawAlt))}"` : '';
    if (/^data:image\//i.test(src)) {
      return `<img class="md-img" src="${attrEsc(src)}"${altAttr} />`;
    }
    return `<img class="md-img" data-img-src="${attrEsc(src)}"${altAttr} />`;
  });
}

function tagName(rawTag: string): string {
  const m = rawTag.match(/^<\/?\s*([a-zA-Z][a-zA-Z0-9-]*)/);
  return m ? m[1].toLowerCase() : '';
}

function extractAllowedTags(s: string, store: string[]): string {
  return s.replace(/<\/?[a-zA-Z][a-zA-Z0-9-]*\b[^>]*?\/?>/g, (m) => {
    if (ALLOWED_HTML_TAGS.has(tagName(m))) {
      store.push(m);
      return `\x00HT${store.length - 1}\x00`;
    }
    return m;
  });
}

/** Inline-only markdown → HTML (no block elements, no fenced code blocks).
 *  Exposed for consumers that want to render markdown inside an existing
 *  block container (list items, table cells, …) without `renderMarkdown`
 *  wrapping the output in `<p>` tags.  Supports the same inline subset as
 *  the block renderer: `**bold**`, `*em*`, `\`code\``, `[link](url)`, and
 *  emoji shortcodes. Output reuses the shared `md-*` classes. */
export function renderInlineMarkdown(s: string): string {
  return inline(s);
}

/** What an inline code span names, so a view can colour it the way the editor colours that thing. */
export type InlineCodeKind = 'annotation' | 'call' | 'type' | 'const' | 'path' | 'number' | 'keyword' | 'ident' | 'other';

const KEYWORDS = new Set([
  'public', 'private', 'protected', 'static', 'final', 'abstract', 'void', 'class', 'interface', 'enum', 'record',
  'extends', 'implements', 'return', 'new', 'throws', 'import', 'package', 'fn', 'pub', 'let', 'mut', 'impl', 'struct',
  'trait', 'async', 'await', 'const', 'use', 'mod', 'self', 'this', 'super', 'null', 'true', 'false',
  'int', 'long', 'short', 'byte', 'char', 'float', 'double', 'boolean', 'bool', 'String', 'str',
]);

/**
 * Classify the text of an inline code span by the shape of what it names. A guess from the text alone —
 * it never fails, it only declines: anything that is not clearly one thing is `other` and keeps the plain
 * look, which is better than a wrong colour.
 */
export function inlineCodeKind(code: string): InlineCodeKind {
  const t = code.trim();
  if (!t || /\s/.test(t.replace(/\(.*\)$/, ''))) return /^@\w/.test(t) ? 'annotation' : 'other';
  if (/^@[\w.$]+/.test(t)) return 'annotation';
  if (/\.(java|xml|rs|ts|js|svelte|toml|json|ya?ml|properties|md|html|jsp|sql|kt|gradle|css|lua)$/i.test(t) || /[\\/]/.test(t)) return 'path';
  if (/^[\w$.]*\w\(.*\)?$/.test(t)) return 'call';
  if (/^-?\d[\d_.]*[fFlLdD]?$/.test(t)) return 'number';
  if (KEYWORDS.has(t)) return 'keyword';
  if (/^[A-Z][A-Z0-9_]{2,}$/.test(t) || /\.[A-Z][A-Z0-9_]{2,}$/.test(t)) return 'const';
  if (/^[A-Z][\w$]*(<.*>)?(\.[A-Za-z_$][\w$]*)*$/.test(t)) return 'type';
  if (/^[a-z_$][\w$]*(\.[a-z_$][\w$]*)*$/.test(t)) return 'ident';
  return 'other';
}

function inline(s: string): string {
  const inlineCodes: string[] = [];
  // Pull inline-code segments out first — they must NOT be touched by the
  // emoji-shortcode pass (`:foo:` inside backticks should stay literal).
  s = s.replace(/`([^`]+)`/g, (_m, c) => {
    inlineCodes.push(`<code class="md-inline-code md-ic-${inlineCodeKind(c)}">${esc(c)}</code>`);
    return `\x00IC${inlineCodes.length - 1}\x00`;
  });
  s = replaceEmojiShortcodes(s);
  s = esc(s);
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  s = s.replace(/~~([^~]+)~~/g, '<s>$1</s>');
  s = s.replace(/\[([^\]]+)\]\([^)]+\)/g, '<span class="md-link">$1</span>');
  s = s.replace(/\x00IC(\d+)\x00/g, (_m, i) => inlineCodes[parseInt(i)]);
  return s;
}

/** One GFM table row split into its cells. Leading and trailing pipes are optional. */
function tableCells(row: string): string[] {
  let r = row.trim();
  if (r.startsWith('|')) r = r.slice(1);
  if (r.endsWith('|')) r = r.slice(0, -1);
  return r.split('|').map((c) => c.trim());
}

/** `| --- | :---: |` — the line under a table's header. */
const TABLE_RULE = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

/**
 * Replace each GFM table in `lines` with a placeholder line and push its HTML to `tables`. A table is a
 * row with at least one pipe, then a rule line, then rows until a line without a pipe.
 */
function extractTables(lines: string[], tables: string[]): string[] {
  const out: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    const head = lines[i];
    if (head.includes('|') && i + 1 < lines.length && TABLE_RULE.test(lines[i + 1]) && lines[i + 1].includes('-')) {
      const heads = tableCells(head);
      const aligns = tableCells(lines[i + 1]).map((c) =>
        c.startsWith(':') && c.endsWith(':') ? 'center' : c.endsWith(':') ? 'right' : '');
      let j = i + 2;
      const rows: string[][] = [];
      while (j < lines.length && lines[j].includes('|') && lines[j].trim() !== '') {
        rows.push(tableCells(lines[j]));
        j++;
      }
      const cell = (tag: 'th' | 'td', text: string, n: number) =>
        `<${tag}${aligns[n] ? ` style="text-align:${aligns[n]}"` : ''}>${inline(text)}</${tag}>`;
      const html =
        '<div class="md-table-wrap"><table class="md-table"><thead><tr>' +
        heads.map((h, n) => cell('th', h, n)).join('') +
        '</tr></thead><tbody>' +
        rows.map((r) => '<tr>' + heads.map((_, n) => cell('td', r[n] ?? '', n)).join('') + '</tr>').join('') +
        '</tbody></table></div>';
      tables.push(html);
      out.push(`\x00TBL${tables.length - 1}\x00`);
      i = j - 1;
    } else {
      out.push(head);
    }
  }
  return out;
}

export function renderMarkdown(md: string): string {
  if (!md) return '';

  const codeBlocks: string[] = [];
  let text = md.replace(/```(\w*)\n?([\s\S]*?)```/g, (_match, lang: string, code: string) => {
    const trimmed = code.replace(/\n$/, '');
    // If the fence has no language tag, try to infer one from the content so
    // Dependabot / security-bot bodies (which often skip the tag) still get
    // highlighted instead of degrading to a wall of plain monospaced text.
    const effectiveLang = lang || detectLang(trimmed) || '';
    let highlighted: string;
    if (effectiveLang && Prism.languages[effectiveLang]) {
      try { highlighted = Prism.highlight(trimmed, Prism.languages[effectiveLang], effectiveLang); }
      catch { highlighted = esc(trimmed); }
    } else {
      highlighted = esc(trimmed);
    }
    codeBlocks.push(
      `<pre class="md-pre language-${effectiveLang || 'text'}" data-lang="${effectiveLang}"><code class="md-code">${highlighted}</code></pre>`,
    );
    return `\x00BLK${codeBlocks.length - 1}\x00`;
  });

  // Strip HTML comments (`<!-- ... -->`) AFTER fenced-code extraction — bots
  // like GitLab Security Bot embed invisible markers (e.g.
  // `<!-- policy_violation_comment -->`) that should never surface in the
  // rendered output. Doing this after the code-block placeholder pass keeps
  // genuine HTML-comment syntax inside fenced blocks intact.
  text = text.replace(/<!--[\s\S]*?-->/g, '');

  // Markdown image syntax → `<img>`. Done before `extractAllowedTags` so the
  // generated tags flow through the same HTML path (and `<img>` is now an
  // allowed tag); `prepareImagesForPreview` normalizes them at the end.
  text = text.replace(
    /!\[([^\]]*)\]\(([^)\s]+)(?:\s+"([^"]*)")?\)/g,
    (_m, alt: string, url: string, title?: string) =>
      `<img src="${attrEsc(url)}" alt="${attrEsc(alt)}"${title ? ` title="${attrEsc(title)}"` : ''}>`,
  );

  text = sanitizeHtml(text);
  const htmlTags: string[] = [];
  text = extractAllowedTags(text, htmlTags);

  const tables: string[] = [];
  const lines = extractTables(text.split('\n'), tables);
  const out: string[] = [];
  let inUl = false, inOl = false;

  function closeList() {
    if (inUl) { out.push('</ul>'); inUl = false; }
    if (inOl) { out.push('</ol>'); inOl = false; }
  }

  for (const rawLine of lines) {
    const blkM = rawLine.match(/^\x00BLK(\d+)\x00$/);
    if (blkM) { closeList(); out.push(codeBlocks[parseInt(blkM[1])]); continue; }

    const tblM = rawLine.match(/^\x00TBL(\d+)\x00$/);
    if (tblM) { closeList(); out.push(tables[parseInt(tblM[1])]); continue; }

    const hM = rawLine.match(/^(#{1,6})\s+(.*)/);
    if (hM) { closeList(); const lvl = hM[1].length; out.push(`<h${lvl} class="md-h${Math.min(lvl, 3)}">${inline(hM[2])}</h${lvl}>`); continue; }

    const bqM = rawLine.match(/^>\s?(.*)/);
    if (bqM) { closeList(); out.push(`<blockquote class="md-bq">${inline(bqM[1])}</blockquote>`); continue; }

    if (/^[-*_]{3,}$/.test(rawLine.trim())) { closeList(); out.push('<hr class="md-hr" />'); continue; }

    // An indented item stays in the same list — nesting is flattened rather than shown as plain text.
    const ulM = rawLine.match(/^\s*[-*]\s+(.*)/);
    if (ulM) { if (inOl) { out.push('</ol>'); inOl = false; } if (!inUl) { out.push('<ul class="md-ul">'); inUl = true; } out.push(`<li>${inline(ulM[1])}</li>`); continue; }

    const olM = rawLine.match(/^\s*\d+[.)]\s+(.*)/);
    if (olM) { if (inUl) { out.push('</ul>'); inUl = false; } if (!inOl) { out.push('<ol class="md-ol">'); inOl = true; } out.push(`<li>${inline(olM[1])}</li>`); continue; }

    closeList();
    if (rawLine.trim() === '') {
      out.push('<div class="md-spacer"></div>');
      continue;
    }

    // Lines that lead with a block-level HTML tag (e.g. `<details>`, `<p>…</p>`,
    // `<blockquote>`) should not be wrapped in `<p class="md-p">` — the wrap
    // would produce invalid nesting that browsers auto-close, breaking the
    // visual rhythm. Pass the line through `inline()` so markdown inside the
    // tags (bold, inline code, emoji) still works.
    const leadM = rawLine.match(/^\s*\x00HT(\d+)\x00/);
    if (leadM && BLOCK_HTML_TAGS.has(tagName(htmlTags[parseInt(leadM[1])]))) {
      out.push(inline(rawLine));
    } else {
      out.push(`<p class="md-p">${inline(rawLine)}</p>`);
    }
  }

  closeList();
  let result = out.join('');
  result = result.replace(/\x00HT(\d+)\x00/g, (_m, i) => htmlTags[parseInt(i)]);
  return prepareImagesForPreview(result);
}
