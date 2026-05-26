/**
 * Markdown editor — CodeMirror 6 glue + Obsidian-style live preview.
 *
 * Builds on top of `@codemirror/lang-markdown` (Lezer Markdown parser). The
 * live preview is a decoration ViewPlugin that walks the syntax tree of the
 * visible viewport and:
 *
 *   • Sizes ATX headings (h1..h6) and dims their leading `#` marks
 *   • Renders strong / emphasis / strikethrough / inline-code with proper
 *     styling and conceals the surrounding markup characters **per
 *     inline component**: only the element under the selection reveals
 *     its raw markdown; siblings on the same line stay rendered.
 *   • Adds a left border + muted colour to blockquote lines
 *   • Paints fenced code blocks with a contrasting background and tokenises
 *     their content through Prism (same grammar set as DiffViewer / blame)
 *     so syntax highlighting matches the rest of the app.
 *   • Renders links as the visible label + dims the URL when the cursor
 *     is outside the link
 *   • Bumps list-marker contrast and horizontal-rule rendering
 *
 * The plugin only inspects the *visible* viewport and is rebuilt when the
 * doc changes, the viewport scrolls, or the selection moves — so it stays
 * cheap even on long README files.
 */

import Prism from 'prismjs';
import './prism-shared';        // side-effect: registers every grammar
import { syntaxTree } from '@codemirror/language';
import type { SyntaxNodeRef } from '@lezer/common';
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
  keymap,
} from '@codemirror/view';
import {
  EditorState,
  StateField,
  Facet,
  Compartment,
  type Extension,
  type Range,
} from '@codemirror/state';
import { convertFileSrc } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  history,
  defaultKeymap,
  historyKeymap,
  indentWithTab,
} from '@codemirror/commands';
import {
  syntaxHighlighting,
  HighlightStyle,
  bracketMatching,
  indentOnInput,
} from '@codemirror/language';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { tags as t } from '@lezer/highlight';

// ─── Facets ─────────────────────────────────────────────────────────────

/**
 * Absolute filesystem path of the markdown document currently in the
 * editor. Used to resolve relative URLs in `![…](…)` image/video/audio
 * references — without it the WebView can't load assets next to the
 * .md file. Provide it via `createMarkdownExtensions({ docPath })`.
 */
export const markdownDocPath = Facet.define<string | null, string | null>({
  combine: (values) => (values.length ? values[values.length - 1] : null),
  static: false,
});

// ─── Decorations ────────────────────────────────────────────────────────

const headingLine = (level: number) =>
  Decoration.line({ attributes: { class: `cm-md-h${level}` } });

const blockquoteLine = Decoration.line({ attributes: { class: 'cm-md-blockquote' } });
const codeBlockLine  = Decoration.line({ attributes: { class: 'cm-md-codeblock-line' } });
const hrLine         = Decoration.line({ attributes: { class: 'cm-md-hr' } });

const concealMark = Decoration.mark({ class: 'cm-md-conceal' });
const boldMark    = Decoration.mark({ class: 'cm-md-bold' });
const italicMark  = Decoration.mark({ class: 'cm-md-italic' });
const strikeMark  = Decoration.mark({ class: 'cm-md-strike' });
const inlineCode  = Decoration.mark({ class: 'cm-md-inline-code' });
const linkLabel   = Decoration.mark({ class: 'cm-md-link-label' });
const linkUrlDim  = Decoration.mark({ class: 'cm-md-link-url' });
const bulletMarker  = Decoration.mark({ class: 'cm-md-list-marker cm-md-list-marker-bullet' });
const orderedMarker = Decoration.mark({ class: 'cm-md-list-marker cm-md-list-marker-ordered' });
const taskUnchecked = Decoration.mark({ class: 'cm-md-task-unchecked' });
const taskChecked   = Decoration.mark({ class: 'cm-md-task-checked' });
const tableDelimMark = Decoration.mark({ class: 'cm-md-table-delim' });

// Bullet replacement widget — renders `•` in place of `-`/`*`/`+` when the
// cursor is off the line. Same Obsidian-style per-line reveal as inline
// marks: editing a list item shows the raw character, siblings render as a
// proper bullet glyph.
class BulletWidget extends WidgetType {
  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-md-bullet-glyph';
    el.textContent = '•';
    return el;
  }
  eq(_other: WidgetType): boolean { return _other instanceof BulletWidget; }
  ignoreEvent(): boolean { return true; }
}
const bulletReplace = Decoration.replace({ widget: new BulletWidget() });

// ── GFM table — render as real <table> ────────────────────────────────
//
// Obsidian-style: while the cursor sits *outside* the table block we replace
// the entire source range with a block widget that builds a real HTML
// `<table>` (so the user sees a properly framed grid with padded cells,
// alignment, and inline markdown rendered inside each cell). The moment the
// selection moves *into* the range, we fall back to source-mode line
// styling so the user can edit. The default click behaviour of block
// widgets is to position the caret at the widget boundary — once the caret
// lands at `node.from` it's "inside" by our overlap check and source mode
// kicks in on the next viewport rebuild.

type Align = 'left' | 'center' | 'right' | null;

function splitTableRow(s: string): string[] {
  const cells: string[] = [];
  let buf = '';
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === '\\' && i + 1 < s.length && s[i + 1] === '|') { buf += '|'; i++; continue; }
    if (c === '|') { cells.push(buf); buf = ''; continue; }
    buf += c;
  }
  cells.push(buf);
  if (cells.length && cells[0].trim() === '')              cells.shift();
  if (cells.length && cells[cells.length - 1].trim() === '') cells.pop();
  return cells.map(c => c.trim());
}

function parseGfmTable(text: string): { header: string[]; aligns: Align[]; rows: string[][] } | null {
  const lines = text.split(/\r?\n/).filter(l => l.trim().length > 0);
  if (lines.length < 2) return null;
  const header = splitTableRow(lines[0]);
  const sep    = splitTableRow(lines[1]);
  const aligns: Align[] = sep.map(s => {
    const left  = s.startsWith(':');
    const right = s.endsWith(':');
    if (left && right) return 'center';
    if (right)         return 'right';
    if (left)          return 'left';
    return null;
  });
  const rows = lines.slice(2).map(splitTableRow);
  return { header, aligns, rows };
}

// Very small inline markdown renderer used inside table cells. It handles
// code spans, links, bold, italic, strikethrough — enough for typical
// table content. Anything fancier (images, footnotes, nested formatting
// edge cases) falls through as plain text. We build with DOM nodes (no
// innerHTML) so cell content can never be parsed as HTML, even if the
// markdown source contains `<script>`.
function isSafeHref(url: string): boolean {
  return /^(https?:|mailto:|tel:|#|\/|\.)/i.test(url);
}

// ── URL resolution ────────────────────────────────────────────────────
//
// Markdown references in a `.md` file commonly use paths relative to the
// file itself (`./img.png`, `assets/diagram.svg`). To turn those into
// something the WebView can fetch we:
//   • leave already-absolute web URLs (`http(s):`, `data:`, `mailto:`,
//     `blob:`) alone,
//   • join everything else against `dirname(docPath)` and run it through
//     Tauri's `convertFileSrc()` — that gives us a custom-protocol URL
//     (`http://asset.localhost/<encoded path>` on Windows,
//     `asset://localhost/<encoded path>` elsewhere) that the WebView is
//     allowed to load because tauri.conf.json enables the asset protocol.

function dirOf(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i >= 0 ? path.slice(0, i) : '';
}

function isAbsoluteFsPath(p: string): boolean {
  return p.startsWith('/') || /^[a-z]:[\\/]/i.test(p);
}

function isWebUrl(url: string): boolean {
  return /^(https?:|data:|blob:|mailto:|tel:)/i.test(url);
}

/** Returns a URL the WebView can load, or `null` if the reference can't
 *  be resolved (relative path without a known docPath, unsupported
 *  scheme, etc.). */
function resolveAssetUrl(url: string, docPath: string | null): string | null {
  const raw = url.trim();
  if (!raw) return null;
  if (isWebUrl(raw)) return raw;
  // Filesystem path — absolute or relative.
  let fsPath: string;
  if (isAbsoluteFsPath(raw)) {
    fsPath = raw;
  } else {
    if (!docPath) return null;
    const sep = docPath.includes('\\') ? '\\' : '/';
    fsPath = `${dirOf(docPath)}${sep}${raw.replace(/^\.[\\/]/, '')}`;
  }
  try {
    return convertFileSrc(fsPath);
  } catch {
    return null;
  }
}

// ── Media kind detection ──────────────────────────────────────────────

const VIDEO_EXT = /\.(mp4|webm|ogg|ogv|mov|m4v|mkv)(\?|#|$)/i;
const AUDIO_EXT = /\.(mp3|wav|ogg|oga|m4a|flac|aac|opus)(\?|#|$)/i;
const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp|ico|avif)(\?|#|$)/i;

// Bare URLs that lack a file extension but are known to serve media via
// the server's Content-Type. GitHub's user-attachments CDN is the big one
// — pasting a video into a GitHub README produces a bare URL of the form
// `https://github.com/user-attachments/assets/<uuid>`, which the github.com
// renderer turns into an inline <video>. We mimic that behaviour so the
// same README renders the same way inside arbor.
const VIDEO_CDN_PATTERNS = [
  /^https?:\/\/github\.com\/user-attachments\/assets\//i,
];

// CDNs that almost always serve images. Used to render bare autolinks to
// these domains as `<img>` rather than nothing.
const IMAGE_CDN_PATTERNS = [
  /^https?:\/\/user-images\.githubusercontent\.com\//i,
  /^https?:\/\/private-user-images\.githubusercontent\.com\//i,
];

function matchesVideoCdn(url: string): boolean {
  return VIDEO_CDN_PATTERNS.some((re) => re.test(url));
}

function matchesImageCdn(url: string): boolean {
  return IMAGE_CDN_PATTERNS.some((re) => re.test(url));
}

type MediaKind = 'video' | 'audio' | 'image';

function classifyMedia(url: string): MediaKind {
  if (VIDEO_EXT.test(url))    return 'video';
  if (AUDIO_EXT.test(url))    return 'audio';
  if (matchesVideoCdn(url))   return 'video';
  return 'image';
}

/** Used by the autolink handler to decide whether a bare URL on its own
 *  should be replaced with a media widget. Anything not in here stays a
 *  plain underlined link. */
function isProbablyMediaUrl(url: string): boolean {
  return (
    VIDEO_EXT.test(url) ||
    AUDIO_EXT.test(url) ||
    IMAGE_EXT.test(url) ||
    matchesVideoCdn(url) ||
    matchesImageCdn(url)
  );
}

// Build a clickable "open externally" card used as the fallback for media
// that the WebView refuses to play inline (typically signed-redirect CDNs
// like GitHub's user-attachments — they validate against headers the
// embedded WebView can't replicate). The original URL is always opened
// via the system browser, where the user's existing browser session can
// satisfy whatever auth the CDN wants.
function renderExternalMediaCard(url: string, kind: MediaKind): HTMLAnchorElement {
  const a = document.createElement('a');
  a.className = 'cm-md-external-media-card';
  a.href = url;
  a.setAttribute('target', '_blank');
  a.setAttribute('rel', 'noopener noreferrer');
  a.onclick = (e) => {
    e.preventDefault();
    void openUrl(url);
  };

  const icon = document.createElement('span');
  icon.className = 'cm-md-external-media-icon';
  icon.textContent = kind === 'audio' ? '♪' : '▶';
  a.appendChild(icon);

  const labels = document.createElement('span');
  labels.className = 'cm-md-external-media-labels';

  const title = document.createElement('span');
  title.className = 'cm-md-external-media-title';
  title.textContent =
    kind === 'audio' ? 'Play audio in browser' : 'Play video in browser';
  labels.appendChild(title);

  const sub = document.createElement('span');
  sub.className = 'cm-md-external-media-sub';
  sub.textContent = url;
  labels.appendChild(sub);

  a.appendChild(labels);

  const arrow = document.createElement('span');
  arrow.className = 'cm-md-external-media-arrow';
  arrow.textContent = '↗';
  a.appendChild(arrow);

  return a;
}

function renderMediaElement(alt: string, url: string, docPath: string | null): HTMLElement {
  const resolved = resolveAssetUrl(url, docPath);
  const kind     = classifyMedia(url);

  if (!resolved) {
    const fb = document.createElement('span');
    fb.className = 'cm-md-broken-image';
    fb.textContent = alt ? `![${alt}]` : `![](${url})`;
    return fb;
  }

  if (kind === 'video') {
    // Signed-redirect CDNs (GitHub user-attachments and friends) refuse
    // to play inside an isolated WebView: the redirect target needs
    // session cookies that arbor's `tauri://localhost` origin can't
    // store (SameSite=Lax on the Set-Cookie blocks them as cross-site),
    // and the first hop's referrer check returns 404 to anyone who
    // isn't github.com. Replicating any of that from inside the
    // embedded WebView would require a Rust-side proxy with its own
    // cookie jar — out of scope here. Skip the inline player entirely
    // for these URLs and offer an "Open in browser" card instead, so
    // the system browser (where the user already has a GitHub session)
    // can play the video the way it does on github.com.
    if (matchesVideoCdn(url)) {
      return renderExternalMediaCard(url, 'video');
    }

    // Wrap the <video> so the error handler can swap the entire block for
    // the external-open card without leaving dangling siblings.
    const wrap = document.createElement('div');
    wrap.className = 'cm-md-rendered-video-wrap';

    const v = document.createElement('video');
    v.className = 'cm-md-rendered-video';
    v.controls = true;
    v.preload = 'metadata';
    v.muted = true;          // matches GitHub's README rendering — lets the
                             // browser load the first frame without an
                             // explicit user gesture, then the user clicks
                             // play and unmutes if they want sound
    v.setAttribute('playsinline', '');
    // Strip the Referer header on the media request. Some CDNs reject
    // requests whose Referer doesn't match a whitelisted origin (the
    // WebView's `tauri://localhost` rarely is) — sending none usually
    // mirrors the "typed into the address bar" path, which is allowed.
    // Harmless for local / asset-protocol URLs since they don't cross
    // origins.
    v.setAttribute('referrerpolicy', 'no-referrer');
    v.src = resolved;
    if (alt) v.setAttribute('aria-label', alt);

    // If the source still fails (anything else the WebView can't load
    // inline), swap for the external-open card.
    const onFail = () => {
      v.removeEventListener('error', onFail);
      const card = renderExternalMediaCard(url, 'video');
      wrap.replaceWith(card);
    };
    v.addEventListener('error', onFail);

    wrap.appendChild(v);
    return wrap;
  }
  if (kind === 'audio') {
    const wrap = document.createElement('div');
    wrap.className = 'cm-md-rendered-audio-wrap';

    const a = document.createElement('audio');
    a.className = 'cm-md-rendered-audio';
    a.controls = true;
    a.preload = 'metadata';
    a.setAttribute('referrerpolicy', 'no-referrer');
    a.src = resolved;
    if (alt) a.setAttribute('aria-label', alt);

    const onFail = () => {
      a.removeEventListener('error', onFail);
      const card = renderExternalMediaCard(url, 'audio');
      wrap.replaceWith(card);
    };
    a.addEventListener('error', onFail);

    wrap.appendChild(a);
    return wrap;
  }
  // image
  const img = document.createElement('img');
  img.className = 'cm-md-rendered-image';
  img.alt = alt;
  img.src = resolved;
  img.loading = 'lazy';
  img.onerror = () => {
    const fb = document.createElement('span');
    fb.className = 'cm-md-broken-image';
    fb.textContent = alt ? `![${alt}]` : '🖼';
    img.replaceWith(fb);
  };
  return img;
}

function renderInlineMdInto(text: string, parent: Node, docPath: string | null): void {
  let i = 0;
  const len = text.length;
  const rest = (): string => text.slice(i);

  while (i < len) {
    let m: RegExpExecArray | null;
    const tail = rest();

    // Inline code: `…`
    if ((m = /^`([^`\n]+)`/.exec(tail))) {
      const code = document.createElement('code');
      code.textContent = m[1];
      parent.appendChild(code);
      i += m[0].length;
      continue;
    }
    // Image / video / audio: ![alt](url "title") — must come BEFORE the
    // link branch so the leading `!` isn't eaten as plain text and the
    // bracketed group mis-parsed as a link. The media kind is decided
    // from the URL extension (mp4 → <video>, mp3 → <audio>, otherwise
    // <img>).
    if ((m = /^!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/.exec(tail))) {
      parent.appendChild(renderMediaElement(m[1], m[2], docPath));
      i += m[0].length;
      continue;
    }
    // Link: [label](url)
    if ((m = /^\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/.exec(tail))) {
      const a = document.createElement('a');
      renderInlineMdInto(m[1], a, docPath);
      const url = m[2].trim();
      if (isSafeHref(url)) {
        a.setAttribute('href', url);
        a.setAttribute('target', '_blank');
        a.setAttribute('rel', 'noopener noreferrer');
      }
      parent.appendChild(a);
      i += m[0].length;
      continue;
    }
    // Bold: **…** or __…__
    if ((m = /^\*\*([^*\n]+)\*\*/.exec(tail)) || (m = /^__([^_\n]+)__/.exec(tail))) {
      const s = document.createElement('strong');
      renderInlineMdInto(m[1], s, docPath);
      parent.appendChild(s);
      i += m[0].length;
      continue;
    }
    // Italic: *…* or _…_
    if ((m = /^\*([^*\n]+)\*/.exec(tail)) || (m = /^_([^_\n]+)_/.exec(tail))) {
      const e = document.createElement('em');
      renderInlineMdInto(m[1], e, docPath);
      parent.appendChild(e);
      i += m[0].length;
      continue;
    }
    // Strikethrough: ~~…~~
    if ((m = /^~~([^~\n]+)~~/.exec(tail))) {
      const s = document.createElement('s');
      renderInlineMdInto(m[1], s, docPath);
      parent.appendChild(s);
      i += m[0].length;
      continue;
    }
    // Hard break or plain char
    parent.appendChild(document.createTextNode(text[i]));
    i++;
  }
}

// Inline media widget used for top-level `![alt](url)` references in the
// document (outside tables). When the caret is on the image's source the
// existing per-marker conceal logic keeps the raw `![alt](url)` editable;
// off-cursor the replace decoration swaps it for a real <img>/<video>/
// <audio>. Block-level layout is left to the inner element (e.g. video
// is `display: block`).
class MediaWidget extends WidgetType {
  constructor(
    private readonly alt: string,
    private readonly url: string,
    private readonly docPath: string | null,
  ) { super(); }

  eq(other: WidgetType): boolean {
    return other instanceof MediaWidget
        && other.alt     === this.alt
        && other.url     === this.url
        && other.docPath === this.docPath;
  }

  toDOM(): HTMLElement {
    return renderMediaElement(this.alt, this.url, this.docPath);
  }

  ignoreEvent(): boolean { return false; }
}

class TableWidget extends WidgetType {
  constructor(
    private readonly text: string,
    private readonly docPath: string | null,
  ) { super(); }

  eq(other: WidgetType): boolean {
    return other instanceof TableWidget
        && other.text === this.text
        && other.docPath === this.docPath;
  }

  toDOM(): HTMLElement {
    const wrap = document.createElement('div');
    wrap.className = 'cm-md-rendered-table-wrap';

    const parsed = parseGfmTable(this.text);
    if (!parsed) {
      wrap.textContent = this.text;
      return wrap;
    }

    const { header, aligns, rows } = parsed;
    const nCols = header.length;
    const table = document.createElement('table');
    table.className = 'cm-md-rendered-table';

    // GFM lets you omit header content with `| | |` — render that as a
    // headerless grid (no empty grey bar at the top) instead of forcing
    // a blank `<thead>` that looks broken.
    const hasHeader = header.some(c => c.length > 0);
    if (hasHeader) {
      const thead = document.createElement('thead');
      const trh = document.createElement('tr');
      for (let c = 0; c < nCols; c++) {
        const th = document.createElement('th');
        if (aligns[c]) th.style.textAlign = aligns[c]!;
        renderInlineMdInto(header[c] ?? '', th, this.docPath);
        trh.appendChild(th);
      }
      thead.appendChild(trh);
      table.appendChild(thead);
    } else {
      table.classList.add('cm-md-rendered-table-headerless');
    }

    const tbody = document.createElement('tbody');
    for (const row of rows) {
      const tr = document.createElement('tr');
      for (let c = 0; c < nCols; c++) {
        const td = document.createElement('td');
        if (aligns[c]) td.style.textAlign = aligns[c]!;
        renderInlineMdInto(row[c] ?? '', td, this.docPath);
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);

    wrap.appendChild(table);
    return wrap;
  }

  // Let clicks bubble — CodeMirror will position the caret at the widget
  // boundary, and our viewport-rebuild logic flips the same table into
  // source mode on the next frame.
  ignoreEvent(): boolean { return false; }
}

// Cache line decorations by class string — many table lines share the same
// class combination so we want to reuse the Decoration instance.
const lineDecoCache = new Map<string, Decoration>();
function lineDeco(cls: string): Decoration {
  let d = lineDecoCache.get(cls);
  if (!d) {
    d = Decoration.line({ attributes: { class: cls } });
    lineDecoCache.set(cls, d);
  }
  return d;
}

// ─── Live preview decoration plugin ─────────────────────────────────────

function selectionTouchesRange(view: EditorView, from: number, to: number): boolean {
  for (const r of view.state.selection.ranges) {
    if (r.to < from || r.from > to) continue;
    return true;
  }
  return false;
}

function lineSpan(view: EditorView, pos: number): { from: number; to: number } {
  const line = view.state.doc.lineAt(pos);
  return { from: line.from, to: line.to };
}

type PushFn = (from: number, to: number, deco: Decoration) => void;

// Conceal a markdown-marker node **only if the selection is outside the
// component it belongs to** (Obsidian-style per-element reveal). The
// "component" is the marker's parent inline node — Strong/Emphasis/
// Strikethrough/InlineCode/Link/Image/FencedCode. Block-level markers
// (HeaderMark / QuoteMark on a multi-line blockquote) fall back to
// line-level scope so the whole line reveals together.
function concealIfOff(
  view: EditorView,
  push: PushFn,
  from: number,
  to: number,
  scope: { from: number; to: number },
) {
  if (!selectionTouchesRange(view, scope.from, scope.to)) {
    push(from, to, concealMark);
  }
}

function parentRange(node: SyntaxNodeRef): { from: number; to: number } | null {
  const p = node.node.parent;
  return p ? { from: p.from, to: p.to } : null;
}

// ─── Prism token highlighting for fenced code ──────────────────────────

const tokenMarkCache = new Map<string, Decoration>();
function getTokenMark(classes: string): Decoration {
  let m = tokenMarkCache.get(classes);
  if (!m) {
    m = Decoration.mark({ class: classes });
    tokenMarkCache.set(classes, m);
  }
  return m;
}

// Common short aliases that map to Prism grammar names. Anything not listed
// falls through to the exact `lang` string — Prism handles e.g. `bash`,
// `rust`, `yaml`, `kotlin` etc. directly.
const PRISM_LANG_ALIAS: Record<string, string> = {
  js: 'javascript', ts: 'typescript', py: 'python',
  sh: 'bash', shell: 'bash', zsh: 'bash',
  yml: 'yaml', md: 'markdown', rs: 'rust',
  cpp: 'cpp', 'c++': 'cpp', 'c#': 'csharp', cs: 'csharp',
  html: 'markup', xml: 'markup', svg: 'markup',
  ps1: 'powershell', ps: 'powershell',
};

interface PrismTokenLike {
  type:     string;
  content:  string | (string | PrismTokenLike)[];
  length:   number;
  alias?:   string | string[];
}

function isToken(x: unknown): x is PrismTokenLike {
  return typeof x === 'object' && x !== null && 'type' in x && 'length' in x;
}

function tokenClasses(tok: PrismTokenLike): string {
  let classes = `token ${tok.type}`;
  if (tok.alias) {
    if (Array.isArray(tok.alias)) classes += ` ${tok.alias.join(' ')}`;
    else                          classes += ` ${tok.alias}`;
  }
  return classes;
}

/** Walk Prism's token stream and emit a mark decoration per token. Returns
 *  the new offset after consuming all tokens. */
function walkPrismTokens(
  tokens: (string | PrismTokenLike)[],
  offset: number,
  ranges: Array<{ from: number; to: number }>,
  segmentStarts: number[],
  push: PushFn,
): number {
  for (const tok of tokens) {
    if (typeof tok === 'string') {
      offset += tok.length;
      continue;
    }
    if (!isToken(tok)) continue;
    const start = offset;
    if (Array.isArray(tok.content)) {
      offset = walkPrismTokens(tok.content, offset, ranges, segmentStarts, push);
    } else {
      offset += tok.length;
    }
    const absFrom = concatOffsetToAbs(start,  ranges, segmentStarts);
    const absTo   = concatOffsetToAbs(offset, ranges, segmentStarts);
    if (absTo > absFrom) {
      push(absFrom, absTo, getTokenMark(tokenClasses(tok)));
    }
  }
  return offset;
}

function concatOffsetToAbs(
  off: number,
  ranges: Array<{ from: number; to: number }>,
  segmentStarts: number[],
): number {
  // Find the segment that contains `off`. Segments are contiguous in the
  // concatenated string, so a backwards linear scan is O(segments) and
  // segments == number of CodeText nodes (usually 1).
  for (let i = ranges.length - 1; i >= 0; i--) {
    if (off >= segmentStarts[i]) return ranges[i].from + (off - segmentStarts[i]);
  }
  return ranges[0]?.from ?? 0;
}

function highlightFencedCode(view: EditorView, node: SyntaxNodeRef, push: PushFn) {
  let lang: string | null = null;
  const codeRanges: Array<{ from: number; to: number }> = [];

  // Walk children — Lezer markdown emits CodeMark (open), optional
  // CodeInfo (lang), one or more CodeText (body), CodeMark (close).
  const cur = node.node.cursor();
  if (!cur.firstChild()) return;
  do {
    if (cur.name === 'CodeInfo') {
      lang = view.state.sliceDoc(cur.from, cur.to).trim().toLowerCase();
    } else if (cur.name === 'CodeText') {
      codeRanges.push({ from: cur.from, to: cur.to });
    }
  } while (cur.nextSibling());

  if (!lang || codeRanges.length === 0) return;
  const grammarName = PRISM_LANG_ALIAS[lang] ?? lang;
  const grammar = Prism.languages[grammarName];
  if (!grammar) return;

  // Concatenate the body slices so Prism sees the whole block at once —
  // multiline tokens (strings, comments) need that continuity. The
  // segmentStarts array lets us map Prism's flat offsets back to
  // absolute document positions.
  let code = '';
  const segmentStarts: number[] = [];
  for (const r of codeRanges) {
    segmentStarts.push(code.length);
    code += view.state.sliceDoc(r.from, r.to);
  }

  try {
    const tokens = Prism.tokenize(code, grammar);
    walkPrismTokens(tokens as (string | PrismTokenLike)[], 0, codeRanges, segmentStarts, push);
  } catch { /* malformed input or grammar — fall back to plain rendering */ }
}

// ─── Main decoration builder ───────────────────────────────────────────

function buildDecorations(view: EditorView): DecorationSet {
  const entries: Range<Decoration>[] = [];
  const push: PushFn = (from, to, deco) => entries.push(deco.range(from, to));

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from, to,
      enter: (node) => {
        const name = node.name;

        // ── ATX headings ────────────────────────────────────────────────
        if (name.startsWith('ATXHeading')) {
          const level = parseInt(name.slice('ATXHeading'.length), 10) || 1;
          const firstLine = view.state.doc.lineAt(node.from);
          push(firstLine.from, firstLine.from, headingLine(level));
          return;
        }

        // ── Setext headings ────────────────────────────────────────────
        if (name === 'SetextHeading1' || name === 'SetextHeading2') {
          const level = name === 'SetextHeading1' ? 1 : 2;
          let pos = node.from;
          while (pos <= node.to) {
            const line = view.state.doc.lineAt(pos);
            push(line.from, line.from, headingLine(level));
            if (line.to >= node.to) break;
            pos = line.to + 1;
          }
          return;
        }

        // ── Block-level markers — line-scoped conceal ─────────────────
        if (name === 'HeaderMark') {
          const scope = lineSpan(view, node.from);
          const end = Math.min(node.to + 1, view.state.doc.length);
          concealIfOff(view, push, node.from, end, scope);
          return;
        }
        if (name === 'QuoteMark') {
          const scope = lineSpan(view, node.from);
          const end = Math.min(node.to + 1, view.state.doc.length);
          concealIfOff(view, push, node.from, end, scope);
          return;
        }

        // ── Blockquote line decoration ──────────────────────────────────
        if (name === 'Blockquote') {
          let pos = node.from;
          while (pos <= node.to) {
            const line = view.state.doc.lineAt(pos);
            push(line.from, line.from, blockquoteLine);
            if (line.to >= node.to) break;
            pos = line.to + 1;
          }
          return;
        }

        // ── Fenced + indented code blocks ───────────────────────────────
        if (name === 'FencedCode') {
          let pos = node.from;
          while (pos <= node.to) {
            const line = view.state.doc.lineAt(pos);
            push(line.from, line.from, codeBlockLine);
            if (line.to >= node.to) break;
            pos = line.to + 1;
          }
          highlightFencedCode(view, node, push);
          return;
        }
        if (name === 'CodeBlock') {
          let pos = node.from;
          while (pos <= node.to) {
            const line = view.state.doc.lineAt(pos);
            push(line.from, line.from, codeBlockLine);
            if (line.to >= node.to) break;
            pos = line.to + 1;
          }
          return;
        }

        // ── Horizontal rule ─────────────────────────────────────────────
        if (name === 'HorizontalRule') {
          const line = view.state.doc.lineAt(node.from);
          push(line.from, line.from, hrLine);
          if (!selectionTouchesRange(view, line.from, line.to)) {
            push(node.from, node.to, concealMark);
          }
          return;
        }

        // ── Inline composites ──────────────────────────────────────────
        // The composite gets its rendering decoration (bold/italic/etc.).
        // The marker children (EmphasisMark, CodeMark, LinkMark, URL, …)
        // are concealed below only when the selection sits OUTSIDE the
        // composite — that's the per-component reveal: editing
        // `**bold**` shows its `**` without revealing the sibling
        // `*italic*` markers on the same line.
        if (name === 'StrongEmphasis') { push(node.from, node.to, boldMark);   return; }
        if (name === 'Emphasis')       { push(node.from, node.to, italicMark); return; }
        if (name === 'Strikethrough')  { push(node.from, node.to, strikeMark); return; }
        if (name === 'InlineCode')     { push(node.from, node.to, inlineCode); return; }
        if (name === 'Link') {
          push(node.from, node.to, linkLabel);
          return;
        }
        if (name === 'Image') {
          // Obsidian-style reveal: caret on the source → fall through to
          // the per-marker conceal handlers so the user sees the editable
          // `![alt](url)` form. Off-cursor → replace with a rendered
          // <img>/<video>/<audio> (the renderer picks the kind from the
          // URL's file extension).
          if (selectionTouchesRange(view, node.from, node.to)) {
            return;
          }
          const text = view.state.sliceDoc(node.from, node.to);
          const m = /^!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/.exec(text);
          if (m) {
            const docPath = view.state.facet(markdownDocPath);
            push(
              node.from,
              node.to,
              Decoration.replace({ widget: new MediaWidget(m[1], m[2], docPath) }),
            );
            return false;
          }
          return;
        }

        // ── Inline markers — scope = parent component ──────────────────
        if (
          name === 'EmphasisMark'      ||
          name === 'StrikethroughMark' ||
          name === 'LinkMark'          ||
          name === 'LinkTitle'
        ) {
          const scope = parentRange(node) ?? lineSpan(view, node.from);
          concealIfOff(view, push, node.from, node.to, scope);
          return;
        }
        if (name === 'CodeMark') {
          // Two parents possible: InlineCode (one-char ticks) and
          // FencedCode (the ```fences```). Either way, scope = parent
          // node so the entire code component reveals together.
          const scope = parentRange(node) ?? lineSpan(view, node.from);
          concealIfOff(view, push, node.from, node.to, scope);
          return;
        }
        if (name === 'URL') {
          const parent = node.node.parent;
          const parentName = parent?.name;
          const wrappedInLink =
            parentName === 'Link' ||
            parentName === 'Image' ||
            parentName === 'Autolink';

          if (!wrappedInLink) {
            // GFM bare autolink — the parser emits a top-level URL node
            // whose parent is the containing block (typically Paragraph).
            // When the URL stands alone in its block and points to media
            // (extension or known video CDN like
            // github.com/user-attachments) we replace it with a real
            // <img>/<video>/<audio>, matching GitHub README rendering.
            // Otherwise we just paint it as an accent-coloured link.
            const urlText = view.state.sliceDoc(node.from, node.to);
            const aloneInBlock =
              parent != null &&
              view.state.sliceDoc(parent.from, parent.to).trim() === urlText;
            if (
              aloneInBlock &&
              isProbablyMediaUrl(urlText) &&
              !selectionTouchesRange(view, node.from, node.to)
            ) {
              const docPath = view.state.facet(markdownDocPath);
              push(
                node.from,
                node.to,
                Decoration.replace({ widget: new MediaWidget('', urlText, docPath) }),
              );
              return false;
            }
            push(node.from, node.to, linkLabel);
            return;
          }

          // Wrapped link/image: dim when cursor is inside the wrapper,
          // fully conceal otherwise (off-component only the label shows).
          const scope = { from: parent!.from, to: parent!.to };
          if (selectionTouchesRange(view, scope.from, scope.to)) {
            push(node.from, node.to, linkUrlDim);
          } else {
            push(node.from, node.to, concealMark);
          }
          return;
        }

        // ── Tables (GFM) ────────────────────────────────────────────────
        // Obsidian-style hybrid rendering:
        //   • selection outside the table  → replace the whole block with
        //     a real <table> widget (rendered cells, header, alignment,
        //     inline markdown inside each cell).
        //   • selection inside the table   → fall back to source-mode
        //     line styling so the user can edit the raw pipes.
        if (name === 'Table') {
          // The block-replace `<table>` widget lives in a StateField
          // (CodeMirror forbids block decorations from plugins). Here we
          // only paint source-mode line styling for when the caret sits
          // inside the table; when it's outside we skip the children so
          // no inline marks get emitted under the now-hidden range.
          const inside = selectionTouchesRange(view, node.from, node.to);
          if (!inside) return false;
          const lines: { from: number; to: number }[] = [];
          let scan = node.from;
          while (scan <= node.to) {
            const ln = view.state.doc.lineAt(scan);
            lines.push({ from: ln.from, to: ln.to });
            if (ln.to >= node.to) break;
            scan = ln.to + 1;
          }
          for (let i = 0; i < lines.length; i++) {
            const ln = lines[i];
            const classes = ['cm-md-table-line'];
            if (i === 0)                classes.push('cm-md-table-first', 'cm-md-table-header-line');
            if (i === 1)                classes.push('cm-md-table-sep-line');
            if (i === lines.length - 1) classes.push('cm-md-table-last');
            push(ln.from, ln.from, lineDeco(classes.join(' ')));
          }
          // Children (TableDelimiter pipes) iterate below in source mode.
          return;
        }
        if (name === 'TableDelimiter') {
          // Only style the `|` pipes when the user is editing in source
          // mode. When the table is rendered as a widget the replace
          // decoration already hides them.
          if (selectionTouchesRange(view, node.from, node.to)) {
            push(node.from, node.to, tableDelimMark);
          } else {
            // Check if we're inside a Table that currently has a caret.
            // The parent chain is TableHeader/TableRow → Table.
            let p = node.node.parent;
            while (p && p.name !== 'Table') p = p.parent;
            if (p && selectionTouchesRange(view, p.from, p.to)) {
              push(node.from, node.to, tableDelimMark);
            }
          }
          return;
        }

        // ── List markers + task list checkboxes ─────────────────────────
        if (name === 'ListMark') {
          const parent = node.node.parent;
          const grandparent = parent ? parent.parent : null;
          const isOrdered = grandparent?.name === 'OrderedList';
          if (isOrdered) {
            push(node.from, node.to, orderedMarker);
          } else {
            // Reveal the raw `-` / `*` / `+` while the cursor is on the
            // item's line; otherwise replace with a real bullet glyph.
            const line = view.state.doc.lineAt(node.from);
            if (selectionTouchesRange(view, line.from, line.to)) {
              push(node.from, node.to, bulletMarker);
            } else {
              push(node.from, node.to, bulletReplace);
            }
          }
          return;
        }
        if (name === 'TaskMarker') {
          const text = view.state.sliceDoc(node.from, node.to);
          const checked = /x/i.test(text);
          push(node.from, node.to, checked ? taskChecked : taskUnchecked);
          return;
        }
      },
    });
  }

  // `Decoration.set` with `sort=true` orders line vs mark decorations by
  // their `startSide` automatically — safer than a hand-rolled comparator
  // that has to cast through private fields.
  return Decoration.set(entries, true);
}

const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || u.selectionSet) {
        this.decorations = buildDecorations(u.view);
      }
    }
  },
  { decorations: v => v.decorations },
);

// ─── Block decorations (tables) — StateField ────────────────────────────
//
// CodeMirror forbids block decorations from coming through a ViewPlugin
// (they'd violate the document-position invariants the editor relies on
// during scroll measurement). The table widget therefore lives in a
// StateField that recomputes on every doc/selection change and renders
// each `Table` node as a real `<table>` block whenever the caret sits
// outside its range.

function selectionTouches(state: EditorState, from: number, to: number): boolean {
  for (const r of state.selection.ranges) {
    if (r.to < from || r.from > to) continue;
    return true;
  }
  return false;
}

function buildTableBlocks(state: EditorState): DecorationSet {
  const entries: Range<Decoration>[] = [];
  const docPath = state.facet(markdownDocPath);
  syntaxTree(state).iterate({
    enter: (node) => {
      if (node.name === 'Table') {
        if (!selectionTouches(state, node.from, node.to)) {
          const text = state.sliceDoc(node.from, node.to);
          entries.push(
            Decoration.replace({
              widget: new TableWidget(text, docPath),
              block: true,
            }).range(node.from, node.to),
          );
        }
        return false; // never descend — children are either hidden under
                      // the block widget or already styled by livePreview
      }
    },
  });
  return Decoration.set(entries, true);
}

const tableBlockField = StateField.define<DecorationSet>({
  create(state) {
    return buildTableBlocks(state);
  },
  update(value, tr) {
    // Rebuild on doc/selection change AND on parser progress: the Lezer
    // markdown parser advances incrementally and emits transactions that
    // touch neither the doc nor the selection. Without the tree-reference
    // check the very first render of a file with tables would stay empty
    // until the user typed or moved the caret.
    if (
      tr.docChanged ||
      tr.selection  ||
      syntaxTree(tr.startState) !== syntaxTree(tr.state)
    ) {
      return buildTableBlocks(tr.state);
    }
    return value;
  },
  provide: f => EditorView.decorations.from(f),
});

// ─── Theme ──────────────────────────────────────────────────────────────

export const markdownTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-primary)',
      fontFamily: 'var(--font-ui-sans)',
      fontSize: '14px',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': {
      fontFamily: 'var(--font-ui-sans)',
      lineHeight: '1.65',
      overflow: 'auto',
    },
    // No `max-width` / `margin: 0 auto` here on purpose — that would create
    // two empty side gutters inside the scroller that wouldn't map back to
    // any `.cm-line`, so clicks in them go to dead space (you'd have to
    // aim exactly at the text). Padding gives breathing room while keeping
    // the whole content area click-active.
    '.cm-content': {
      padding: '24px 48px 48px 48px',
      caretColor: 'var(--text-primary)',
    },
    // Keep each line a clean box: no per-line padding (line decorations
    // add their own padding where needed). This makes CodeMirror's
    // click-to-position hit-testing predictable, especially when line
    // wrapping is on.
    '.cm-line': { padding: '0' },
    '.cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'var(--accent-subtle) !important',
    },
    '&.cm-focused .cm-selectionBackground': {
      backgroundColor: 'var(--accent-subtle) !important',
    },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--text-primary)' },

    // ── Headings ──────────────────────────────────────────────────────
    // IMPORTANT: keep all spacing as padding (not margin) on line
    // decorations. CodeMirror's hit-testing maps clicks to the `.cm-line`
    // bounding box; `margin` shifts the rendered text outside that box
    // and the gap becomes a dead zone (click lands "above the line" and
    // the cursor goes to the wrong row). Padding keeps the spacing
    // inside the line's own box so hit-testing stays accurate, especially
    // with line wrapping where any vertical misalignment compounds.
    '.cm-md-h1': {
      fontSize: '1.85em', fontWeight: '700', lineHeight: '1.35',
      paddingTop: '0.4em', paddingBottom: '0.25em',
      color: 'var(--text-primary)',
      borderBottom: '1px solid var(--border-subtle)',
    },
    '.cm-md-h2': {
      fontSize: '1.5em', fontWeight: '700', lineHeight: '1.4',
      paddingTop: '0.5em', paddingBottom: '0.2em',
      color: 'var(--text-primary)',
      borderBottom: '1px solid var(--border-subtle)',
    },
    '.cm-md-h3': {
      fontSize: '1.25em', fontWeight: '700', lineHeight: '1.45',
      paddingTop: '0.4em',
      color: 'var(--text-primary)',
    },
    '.cm-md-h4': { fontSize: '1.1em',  fontWeight: '700', lineHeight: '1.5',  color: 'var(--text-primary)' },
    '.cm-md-h5': { fontSize: '1em',    fontWeight: '700', lineHeight: '1.55', color: 'var(--text-secondary)' },
    '.cm-md-h6': { fontSize: '0.95em', fontWeight: '700', lineHeight: '1.55', color: 'var(--text-muted)' },

    // ── Marks ─────────────────────────────────────────────────────────
    '.cm-md-conceal': { display: 'none' },
    '.cm-md-bold':    { fontWeight: '700', color: 'var(--text-primary)' },
    '.cm-md-italic':  { fontStyle: 'italic' },
    '.cm-md-strike':  { textDecoration: 'line-through', color: 'var(--text-muted)' },
    '.cm-md-inline-code': {
      fontFamily: 'var(--font-code)',
      fontSize: '0.92em',
      background: 'var(--bg-overlay)',
      border: '1px solid var(--border-subtle)',
      borderRadius: '4px',
      padding: '0 4px',
      color: 'var(--syntax-string, var(--text-primary))',
    },

    // ── Block decorations ─────────────────────────────────────────────
    '.cm-md-blockquote': {
      borderLeft: '3px solid var(--accent)',
      paddingLeft: '12px',
      color: 'var(--text-secondary)',
      fontStyle: 'italic',
      background: 'rgba(77,120,204,0.04)',
    },
    '.cm-md-codeblock-line': {
      fontFamily: 'var(--font-code)',
      fontSize: '0.92em',
      background: 'var(--bg-overlay)',
      paddingLeft: '14px !important',
      paddingRight: '14px !important',
    },
    '.cm-md-hr': {
      borderBottom: '1px solid var(--border-subtle)',
      paddingTop: '0.4em',
      paddingBottom: '0.4em',
    },

    // ── Links ─────────────────────────────────────────────────────────
    '.cm-md-link-label': { color: 'var(--accent)', textDecoration: 'underline', textUnderlineOffset: '2px' },
    '.cm-md-link-url':   { color: 'var(--text-muted)', fontFamily: 'var(--font-code)', fontSize: '0.85em' },

    // ── Lists ─────────────────────────────────────────────────────────
    '.cm-md-list-marker': { color: 'var(--accent)', fontWeight: '600' },
    '.cm-md-list-marker-bullet': {
      // When the raw `-`/`*`/`+` is shown (cursor on that line) we still
      // want it muted enough not to compete with the text — Obsidian dims
      // the source marker on the active line so the typography stays
      // calm.
      color: 'var(--text-muted)',
      fontWeight: '500',
    },
    '.cm-md-list-marker-ordered': {
      // Slight monospace tabular feel so multi-digit numerators (10., 11.)
      // align with single-digit ones.
      fontVariantNumeric: 'tabular-nums',
      color: 'var(--accent)',
      fontWeight: '600',
    },
    '.cm-md-bullet-glyph': {
      color: 'var(--accent)',
      fontWeight: '700',
      fontSize: '1.15em',
      lineHeight: '1',
      display: 'inline-block',
      // The replaced range is exactly the marker char (1 col). Keep the
      // glyph the same advance so caret positioning around it stays
      // predictable.
      width: '1ch',
      textAlign: 'center',
      transform: 'translateY(-0.05em)',
    },

    // ── Tables (GFM) ──────────────────────────────────────────────────
    // Each table line gets `cm-md-table-line`. Role-specific classes layer
    // on top: header (bold + tinted bg), separator (thin divider, dimmed
    // dashes), first/last (rounded corners + outer border).
    '.cm-md-table-line': {
      fontFamily: 'var(--font-code)',
      fontSize: '0.92em',
      background: 'var(--bg-overlay)',
      paddingLeft: '14px !important',
      paddingRight: '14px !important',
      lineHeight: '1.7',
      borderLeft: '1px solid var(--border-subtle)',
      borderRight: '1px solid var(--border-subtle)',
    },
    '.cm-md-table-first': {
      paddingTop: '6px !important',
      borderTop: '1px solid var(--border-subtle)',
      borderTopLeftRadius: 'var(--radius-md, 6px)',
      borderTopRightRadius: 'var(--radius-md, 6px)',
    },
    '.cm-md-table-last': {
      paddingBottom: '6px !important',
      borderBottom: '1px solid var(--border-subtle)',
      borderBottomLeftRadius: 'var(--radius-md, 6px)',
      borderBottomRightRadius: 'var(--radius-md, 6px)',
    },
    '.cm-md-table-header-line': {
      fontWeight: '700',
      color: 'var(--text-primary)',
      background: 'var(--bg-overlay-strong, var(--bg-overlay))',
      borderBottom: '1px solid var(--border-default, var(--border-subtle))',
    },
    '.cm-md-table-sep-line': {
      color: 'var(--text-muted)',
      fontSize: '0.78em',
      lineHeight: '1',
      paddingTop: '2px !important',
      paddingBottom: '2px !important',
      borderBottom: '1px solid var(--border-default, var(--border-subtle))',
      letterSpacing: '0.05em',
    },
    '.cm-md-table-delim': {
      color: 'var(--accent)',
      opacity: '0.55',
      fontWeight: '600',
    },

    // ── Rendered table widget (cursor outside) ────────────────────────
    '.cm-md-rendered-table-wrap': {
      margin: '10px 0',
      border: '1px solid var(--border-default, var(--border-subtle))',
      borderRadius: 'var(--radius-md, 6px)',
      overflow: 'hidden',
      background: 'var(--bg-base)',
    },
    '.cm-md-rendered-table': {
      borderCollapse: 'collapse',
      width: '100%',
      fontFamily: 'var(--font-ui-sans)',
      fontSize: '0.95em',
      lineHeight: '1.55',
    },
    '.cm-md-rendered-table th, .cm-md-rendered-table td': {
      padding: '7px 12px',
      borderRight: '1px solid var(--border-subtle)',
      borderBottom: '1px solid var(--border-subtle)',
      textAlign: 'left',
      verticalAlign: 'top',
      color: 'var(--text-primary)',
    },
    '.cm-md-rendered-table th:last-child, .cm-md-rendered-table td:last-child': {
      borderRight: 'none',
    },
    '.cm-md-rendered-table tr:last-child td': {
      borderBottom: 'none',
    },
    '.cm-md-rendered-table thead th': {
      background: 'var(--bg-overlay)',
      fontWeight: '700',
      color: 'var(--text-primary)',
      borderBottom: '1px solid var(--border-default, var(--border-subtle))',
    },
    '.cm-md-rendered-table tbody tr:nth-child(even) td': {
      background: 'rgba(255,255,255,0.02)',
    },
    '.cm-md-rendered-table code': {
      fontFamily: 'var(--font-code)',
      fontSize: '0.9em',
      background: 'var(--bg-overlay)',
      border: '1px solid var(--border-subtle)',
      borderRadius: '4px',
      padding: '0 4px',
      color: 'var(--syntax-string, var(--text-primary))',
    },
    '.cm-md-rendered-table a': {
      color: 'var(--accent)',
      textDecoration: 'underline',
      textUnderlineOffset: '2px',
    },
    '.cm-md-rendered-table strong': { fontWeight: '700', color: 'var(--text-primary)' },
    '.cm-md-rendered-table em':     { fontStyle: 'italic' },
    '.cm-md-rendered-table s':      { textDecoration: 'line-through', color: 'var(--text-muted)' },

    // Inline images (rendered tables + future inline image support).
    // `max-height` keeps oversized assets from blowing up a single cell;
    // `object-fit: contain` preserves aspect ratio. Falls back to a muted
    // `![alt]` chip when the URL can't be loaded.
    '.cm-md-rendered-image': {
      maxWidth: '100%',
      maxHeight: '220px',
      objectFit: 'contain',
      display: 'inline-block',
      verticalAlign: 'middle',
      borderRadius: '4px',
      background: 'var(--bg-overlay)',
    },
    '.cm-md-rendered-video': {
      maxWidth: '100%',
      maxHeight: '360px',
      display: 'block',
      borderRadius: '4px',
      background: '#000',
      outline: 'none',
    },
    '.cm-md-rendered-audio': {
      width: '100%',
      maxWidth: '420px',
      display: 'block',
    },
    '.cm-md-broken-image': {
      fontStyle: 'italic',
      color: 'var(--text-muted)',
      fontFamily: 'var(--font-code)',
      fontSize: '0.9em',
    },

    // ── External-media card (fallback when inline playback fails) ────
    '.cm-md-external-media-card': {
      display: 'inline-flex',
      alignItems: 'center',
      gap: '12px',
      padding: '10px 14px',
      margin: '6px 0',
      maxWidth: '100%',
      background: 'var(--bg-overlay)',
      border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-md, 6px)',
      color: 'var(--text-primary)',
      textDecoration: 'none',
      cursor: 'pointer',
      transition: 'background 120ms, border-color 120ms',
    },
    '.cm-md-external-media-card:hover': {
      background: 'var(--bg-overlay-strong, var(--bg-overlay))',
      borderColor: 'var(--accent)',
      textDecoration: 'none',
    },
    '.cm-md-external-media-icon': {
      flex: '0 0 auto',
      width: '32px',
      height: '32px',
      borderRadius: '50%',
      background: 'var(--accent)',
      color: 'var(--accent-fg, #fff)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      fontSize: '14px',
      paddingLeft: '2px', // optical centering for the play triangle
    },
    '.cm-md-external-media-labels': {
      flex: '1 1 auto',
      display: 'flex',
      flexDirection: 'column',
      minWidth: '0',
    },
    '.cm-md-external-media-title': {
      fontWeight: '600',
      color: 'var(--text-primary)',
      fontSize: '0.95em',
    },
    '.cm-md-external-media-sub': {
      color: 'var(--text-muted)',
      fontSize: '0.8em',
      fontFamily: 'var(--font-code)',
      overflow: 'hidden',
      textOverflow: 'ellipsis',
      whiteSpace: 'nowrap',
    },
    '.cm-md-external-media-arrow': {
      flex: '0 0 auto',
      color: 'var(--text-muted)',
      fontSize: '1.1em',
    },

    // ── Task list checkboxes ──────────────────────────────────────────
    '.cm-md-task-unchecked': {
      color: 'var(--text-muted)',
      fontFamily: 'var(--font-code)',
    },
    '.cm-md-task-checked': {
      color: 'var(--success, #6a9956)',
      fontFamily: 'var(--font-code)',
    },
  },
  { dark: true },
);

// Syntax highlighting fallback for fenced-code inner content (the inner
// language parser is loaded lazily by lang-markdown when available; without
// a registered language we still want the tokens to render in monospace
// muted text instead of plain body text).
const markdownHighlight = HighlightStyle.define([
  { tag: t.heading1, color: 'var(--text-primary)', fontWeight: '700' },
  { tag: t.heading2, color: 'var(--text-primary)', fontWeight: '700' },
  { tag: t.heading3, color: 'var(--text-primary)', fontWeight: '700' },
  { tag: t.heading4, color: 'var(--text-primary)', fontWeight: '700' },
  { tag: t.heading5, color: 'var(--text-secondary)', fontWeight: '700' },
  { tag: t.heading6, color: 'var(--text-muted)', fontWeight: '700' },
  { tag: t.strong,   fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: t.link,     color: 'var(--accent)' },
  { tag: t.url,      color: 'var(--text-muted)' },
  { tag: t.monospace, fontFamily: 'var(--font-code)' },
  { tag: t.comment,  color: 'var(--syntax-comment, #7a7d85)', fontStyle: 'italic' },
  { tag: t.keyword,  color: 'var(--syntax-keyword, #cc7832)' },
  { tag: t.string,   color: 'var(--syntax-string, #6a9956)' },
  { tag: t.number,   color: 'var(--syntax-number, #9876aa)' },
]);

// ─── Public API ─────────────────────────────────────────────────────────

export interface MarkdownEditorOptions {
  readOnly?: boolean;
  /** Absolute path of the file being edited — needed to resolve relative
   *  `![…](…)` URLs in images/videos/audio. Pass `null` (or omit) for
   *  buffers not backed by a file. */
  docPath?: string | null;
}

export interface MarkdownCompartments {
  readOnly: Compartment;
}

export function makeMarkdownCompartments(): MarkdownCompartments {
  return { readOnly: new Compartment() };
}

export function createMarkdownExtensions(
  opts: MarkdownEditorOptions,
  compartments: MarkdownCompartments,
): Extension {
  const { readOnly = false, docPath = null } = opts;
  return [
    markdownTheme,
    markdown({ base: markdownLanguage, codeLanguages: [] }),
    syntaxHighlighting(markdownHighlight),
    livePreview,
    tableBlockField,
    markdownDocPath.of(docPath),
    history(),
    indentOnInput(),
    bracketMatching(),
    highlightSelectionMatches(),
    EditorView.lineWrapping,
    keymap.of([
      ...defaultKeymap,
      ...historyKeymap,
      ...searchKeymap,
      indentWithTab,
    ]),
    compartments.readOnly.of(EditorState.readOnly.of(readOnly)),
  ];
}
