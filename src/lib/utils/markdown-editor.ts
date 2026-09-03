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
 *   • Adds a left border + muted colour to blockquote lines, and turns the ones
 *     that open with `> [!WARNING]` (GitHub alerts) into a coloured callout with
 *     an icon and a title
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
  type Text,
} from '@codemirror/state';
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { convertFileSrc } from '@tauri-apps/api/core';
import { openPath, openUrl } from '@tauri-apps/plugin-opener';
import { isMac } from '$lib/utils/platform';
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
/**
 * The files a link may point at — the host's list, because the editor has no idea what a project
 * is. Absolute paths; the completion writes them relative to the document being edited.
 *
 * A function rather than an array so the host can keep it lazy and current: it is called when the
 * completion list is built, never on every keystroke.
 */
export const markdownFileIndex = Facet.define<
  (() => string[]) | null,
  (() => string[]) | null
>({ combine: (values) => values.find((v) => v != null) ?? null });

/**
 * What "open this file" means, supplied by whoever mounts the editor.
 *
 * The editor knows a link points at a path; it cannot know whether that should become a tab, a
 * note, or a window — Bennu, Garrulus and the markdown modal answer differently. `null` (nobody
 * said) falls back to the operating system.
 */
export const markdownOpenLink = Facet.define<
  ((path: string, anchor: string | null) => void) | null,
  ((path: string, anchor: string | null) => void) | null
>({
  combine: (values) => values.find((v) => v) ?? null,
});

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

// ── GitHub alerts (`> [!WARNING]`) ────────────────────────────────────
//
// A blockquote whose first line is `[!KIND]` is not a quotation, it is a callout, and every
// markdown surface that matters renders it as one — GitHub, GitLab, Obsidian (as its own
// `> [!note]`), VS Code. Rendered here rather than left as a quoted `[!WARNING]` because that
// line is the *only* thing distinguishing an important warning from an offhand citation, and a
// reader who has to notice five raw characters to tell them apart will not.
//
// The kinds are GitHub's five plus the Obsidian words that mean the same things — a vault and a
// repository are the two places these files come from, and one of them writing `[!danger]` where
// the other writes `[!caution]` is not a reason to render one of them as a quotation. Nothing
// beyond that union: an alert nobody else renders is a broken box in every other viewer the file
// passes through.

/** One alert kind: what it is called, and the glyph that says so at a glance. */
interface AlertKind {
  /** Title shown when the head line has no title of its own. */
  label: string;
  /** Lucide icon geometry, as `<path>`/`<circle>` element specs. */
  paths: Array<[tag: string, attrs: Record<string, string>]>;
}

const ALERT_KINDS: Record<string, AlertKind> = {
  note: {
    label: 'Note',
    paths: [
      ['circle', { cx: '12', cy: '12', r: '10' }],
      ['path', { d: 'M12 16v-4' }],
      ['path', { d: 'M12 8h.01' }],
    ],
  },
  tip: {
    label: 'Tip',
    paths: [
      ['path', { d: 'M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5' }],
      ['path', { d: 'M9 18h6' }],
      ['path', { d: 'M10 22h4' }],
    ],
  },
  important: {
    label: 'Important',
    paths: [
      ['path', { d: 'M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z' }],
      ['path', { d: 'M12 7v2' }],
      ['path', { d: 'M12 13h.01' }],
    ],
  },
  warning: {
    label: 'Warning',
    paths: [
      ['path', { d: 'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3' }],
      ['path', { d: 'M12 9v4' }],
      ['path', { d: 'M12 17h.01' }],
    ],
  },
  caution: {
    label: 'Caution',
    paths: [
      ['path', { d: 'M12 16h.01' }],
      ['path', { d: 'M12 8v4' }],
      ['path', { d: 'M15.312 2a2 2 0 0 1 1.414.586l4.688 4.688A2 2 0 0 1 22 8.688v6.624a2 2 0 0 1-.586 1.414l-4.688 4.688a2 2 0 0 1-1.414.586H8.688a2 2 0 0 1-1.414-.586l-4.688-4.688A2 2 0 0 1 2 15.312V8.688a2 2 0 0 1 .586-1.414l4.688-4.688A2 2 0 0 1 8.688 2z' }],
    ],
  },
  question: {
    label: 'Question',
    paths: [
      ['circle', { cx: '12', cy: '12', r: '10' }],
      ['path', { d: 'M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3' }],
      ['path', { d: 'M12 17h.01' }],
    ],
  },
  example: {
    label: 'Example',
    paths: [
      ['path', { d: 'm3 17 2 2 4-4' }],
      ['path', { d: 'm3 7 2 2 4-4' }],
      ['path', { d: 'M13 6h8' }],
      ['path', { d: 'M13 12h8' }],
      ['path', { d: 'M13 18h8' }],
    ],
  },
  quote: {
    label: 'Quote',
    paths: [
      ['path', { d: 'M16 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z' }],
      ['path', { d: 'M5 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z' }],
    ],
  },
};

/**
 * The words that mean one of the kinds above.
 *
 * A vault written in Obsidian says `[!danger]` and a repository says `[!caution]`, and they are
 * the same box. Mapping them here rather than giving each its own entry keeps one icon, one
 * colour and one label per *meaning* — the alternative is two boxes that look different and
 * read the same.
 */
const ALERT_ALIASES: Record<string, string> = {
  info: 'note',
  hint: 'tip',
  success: 'tip',
  check: 'tip',
  done: 'tip',
  attention: 'warning',
  danger: 'caution',
  error: 'caution',
  bug: 'caution',
  help: 'question',
  faq: 'question',
  cite: 'quote',
};

/**
 * The marker that opens an alert, and whatever the author wrote after it as a title.
 *
 * The `[-+]?` is Obsidian's fold state (`[!warning]-` opens collapsed). Matched so that it does
 * not end up rendered as the callout's title; acting on it is a separate feature, and a stray
 * dash where a title belongs is the kind of small wrongness that makes a renderer look broken.
 */
const ALERT_HEAD = /^(\s*)\[!([a-z]+)\]([-+]?)(\s*)(.*)$/i;

/** The kind an alert word names, or `null` when it names none of them. */
function alertKindOf(word: string): string | null {
  const key = word.toLowerCase();
  const resolved = ALERT_ALIASES[key] ?? key;
  return ALERT_KINDS[resolved] ? resolved : null;
}

/** The badge that replaces `[!WARNING]` while the caret is elsewhere: the kind's icon and its
 *  name, in the kind's colour. A widget rather than a `::before`, because the icon has to be
 *  real geometry to stay crisp and the label has to be the kind's own word. */
class AlertHeadWidget extends WidgetType {
  constructor(
    private readonly kind: string,
    private readonly title: string,
  ) {
    super();
  }

  eq(other: AlertHeadWidget): boolean {
    return other.kind === this.kind && other.title === this.title;
  }

  toDOM(): HTMLElement {
    const spec = ALERT_KINDS[this.kind];
    const wrap = document.createElement('span');
    wrap.className = `cm-md-callout-badge cm-md-callout-badge-${this.kind}`;
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '2');
    svg.setAttribute('stroke-linecap', 'round');
    svg.setAttribute('stroke-linejoin', 'round');
    for (const [tag, attrs] of spec.paths) {
      const el = document.createElementNS('http://www.w3.org/2000/svg', tag);
      for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
      svg.appendChild(el);
    }
    wrap.appendChild(svg);
    const label = document.createElement('span');
    // The author's own title wins: `> [!NOTE] Read this first` is a heading they wrote, and
    // replacing it with the word "Note" would delete it from the rendering.
    label.textContent = this.title || spec.label;
    wrap.appendChild(label);
    return wrap;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

const concealMark = Decoration.mark({ class: 'cm-md-conceal' });
const boldMark    = Decoration.mark({ class: 'cm-md-bold' });
const italicMark  = Decoration.mark({ class: 'cm-md-italic' });
const strikeMark  = Decoration.mark({ class: 'cm-md-strike' });
const inlineCode  = Decoration.mark({ class: 'cm-md-inline-code' });
const codeLangMark = Decoration.mark({ class: 'cm-md-code-lang' });
const linkLabel   = Decoration.mark({ class: 'cm-md-link-label' });

/**
 * A **real `<a href>`** around a rendered link — not a coloured span.
 *
 * ⚠️⚠️ This is the whole reason links in ordinary text did nothing. The delegated click handler
 * looks for `a[href]`, which is right, but body-text links were painted with a bare
 * `Decoration.mark({ class })` — a `<span>` with no href, so `closest('a[href]')` found nothing
 * and the click fell through to "place the caret". Links *inside widgets* (a table cell, a
 * callout) worked, because those are built with `document.createElement('a')` — which is exactly
 * the kind of half-working that makes the feature look implemented.
 *
 * A mark decoration takes a `tagName`, so the fix costs nothing: same range, same class, real
 * element. Cached by URL so a redraw reuses the decoration instead of rebuilding the DOM.
 */
const linkAnchors = new Map<string, Decoration>();
function linkAnchor(url: string): Decoration {
  let deco = linkAnchors.get(url);
  if (!deco) {
    // A document with thousands of distinct links would grow this forever; the cache exists to
    // keep redraws stable, not to remember a session, so it starts over rather than leaking.
    if (linkAnchors.size > 500) linkAnchors.clear();
    deco = Decoration.mark({
      tagName: 'a',
      class: 'cm-md-link-label',
      attributes: { href: url, title: url },
    });
    linkAnchors.set(url, deco);
  }
  return deco;
}
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
/**
 * Whether a link's target may be written into the `href`.
 *
 * Everything except a scheme this editor will not follow. A **bare relative path** (`notes/api.md`,
 * with no leading `./`) counts: it is the way most people write an intra-project link, and
 * excluding it meant those anchors got no `href` at all — so the click handler, which looks for
 * `a[href]`, could not see them and they were dead in a way nothing explained.
 */
function isSafeHref(url: string): boolean {
  if (/^[a-z][a-z0-9+.-]*:/i.test(url)) return /^(https?:|mailto:|tel:)/i.test(url);
  return true;
}

/** The target a `Link`/`Autolink` node points at, taken from its `URL` child — `null` for a
 *  reference link (`[label][ref]`), which has none, and for a scheme this editor won't follow. */
function urlChildOf(view: EditorView, node: SyntaxNodeRef): string | null {
  const url = node.node.getChild('URL');
  if (!url) return null;
  const raw = view.state.sliceDoc(url.from, url.to).trim();
  return raw && isSafeHref(raw) ? raw : null;
}

// ─── Following a link ───────────────────────────────────────────────────
//
// ⚠️ A rendered `<a>` inside a `contenteditable` **does nothing when clicked** — the browser
// treats it as text you are editing, not as a link — and `target="_blank"` has nowhere to go in a
// WebView. So every link in this editor was decoration: the external ones, the `#anchor`s and the
// relative paths alike. This is the handler that makes them links.
//
// One delegated listener rather than a handler per anchor: links are created in three places (the
// inline renderer, a table cell, the external-media card) and a fourth will be added by whoever
// adds the next widget. Delegation is what stops that fourth one from being dead again.

/** Where a link wants to go, once it is known what kind it is. */
type LinkTarget =
  | { kind: 'web'; url: string }
  /** A heading in THIS document — the GitHub-style `#slug`. */
  | { kind: 'anchor'; slug: string }
  /** A file, resolved against the document's own directory. */
  | { kind: 'file'; path: string; anchor: string | null };

/**
 * The id a heading has without declaring one — GitHub's slug, which is also Obsidian's rule:
 * lower-case the text, drop the punctuation, spaces become dashes.
 *
 * ⚠️ **Letters are `\p{L}`, not `\w`.** JavaScript's `\w` is `[A-Za-z0-9_]` and nothing else, so a
 * punctuation-stripping pass built on it eats every accent: `Perché il CST` became `perch-il-cst`
 * and `Città` became `citt`. In an Italian document that is most of the headings, and it made
 * every hand-written `#anchor` miss — which reads, from the outside, as "anchors are not
 * supported".
 *
 * The markup inside a heading is not part of its name: `## Il **CST**` is `il-cst`, and a heading
 * that is a link is slugged from the label a reader sees rather than from the URL they do not.
 */
function headingSlug(text: string): string {
  return text
    .trim()
    // A closed ATX heading (`## Titolo ##`) — the trailing hashes are syntax, not name.
    .replace(/\s+#+\s*$/, '')
    // `[label](url)` → `label`, and `![alt](url)` → nothing: an image in a heading is not text.
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    // ⚠️⚠️ **One dash per space, and runs are NOT collapsed.** This is where a slug that looked
    // right stopped matching: `### B.1 — Merula standalone` loses its em dash and keeps the two
    // spaces around it, so GitHub's id is `b1--merula-standalone` with a double dash. Collapsing
    // them produced `b1-merula-standalone`, and every anchor copied from a GitHub table of
    // contents missed — which is most headings in a document that uses em dashes at all.
    .replace(/\s/g, '-');
}

/**
 * The forgiving form of a slug, used only as a fallback after every exact match has been tried.
 *
 * Two things a link written by hand gets differently from a link generated by a tool, and
 * neither is a mistake worth refusing:
 *
 *  • **accents** — `perche` for `perché`, because that is what one types;
 *  • **dash runs** — `#pipelines-ci-cd` for a heading whose GitHub id is `pipelines--ci--cd`.
 *    The doubling comes from punctuation that stood between two spaces, which is invisible in
 *    the rendered title, so nobody writing the anchor by hand would ever reproduce it.
 */
function foldSlug(slug: string): string {
  return slug
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .replace(/-{2,}/g, '-')
    .replace(/^-+|-+$/g, '');
}

function classifyLink(href: string, docPath: string | null): LinkTarget | null {
  const url = href.trim();
  if (!url) return null;
  if (/^(https?:|mailto:|tel:)/i.test(url)) return { kind: 'web', url };
  if (url.startsWith('#')) return { kind: 'anchor', slug: url.slice(1).toLowerCase() };
  // Everything else is a path. A `#fragment` on it names a heading in THAT file.
  const hash = url.indexOf('#');
  const rawPath = hash >= 0 ? url.slice(0, hash) : url;
  const anchor = hash >= 0 ? url.slice(hash + 1).toLowerCase() : null;
  const path = isAbsoluteFsPath(rawPath) ? rawPath : resolveRelativePath(rawPath, docPath);
  return path ? { kind: 'file', path, anchor } : null;
}

/** `./notes/x.md` against the open document's directory. `null` for a buffer with no path —
 *  there is nothing to resolve against, and guessing would open a file somewhere else. */
function resolveRelativePath(rel: string, docPath: string | null): string | null {
  if (!docPath) return null;
  const base = dirOf(docPath).replace(/\\/g, '/').replace(/\/+$/, '');
  const parts = `${base}/${decodeURIComponent(rel)}`.split('/');
  const out: string[] = [];
  for (const part of parts) {
    if (part === '.' || part === '') { if (out.length === 0) out.push(part); continue; }
    if (part === '..') { out.pop(); continue; }
    out.push(part);
  }
  return out.join('/');
}

/** Move the caret to the heading whose slug is `slug`, and show it. `false` when there is none —
 *  the caller says so rather than scrolling to nowhere.
 *
 *  Public because a `file.md#heading` link crosses documents: the editor that opens the file is
 *  not the one that was clicked, so the host has to ask the new one to make the jump. */
/** A heading in the document, with the id a link can reach it by. */
export interface DocumentHeading {
  /** GitHub's id — the heading's text, with repeats suffixed `-1`, `-2`. */
  id: string;
  /** The text as written, markup and all. What a reader recognises in a list. */
  text: string;
  /** `#` count — 1..6, or 2 for an underlined heading, so a list can show depth. */
  level: number;
  /** Start of the heading's line. */
  from: number;
}

/**
 * Every heading in the document, in order, with its id.
 *
 * One scan behind both the jump and the completion list: a completion offering an id that the
 * jump then fails to find would be worse than no completion at all, and the only way to be sure
 * they agree is for them to be the same code.
 *
 * ⚠️ Line-based rather than tree-based on purpose: CodeMirror parses lazily, so on a long
 * document `syntaxTree` covers the viewport and not much more — headings past it simply would
 * not exist. Two things the scan therefore has to know itself:
 *
 *  • **fenced code is not text.** A `# install deps` in a shell block is a comment, and it sits
 *    *above* the heading it would shadow, so a top-down search returns it and the reader lands
 *    in a code block.
 *  • **an underlined heading is a heading** (`Titolo` over `===` or `---`), which needs the
 *    previous line to recognise.
 */
export function documentHeadings(doc: Text): DocumentHeading[] {
  const out: DocumentHeading[] = [];
  const seen = new Map<string, number>();
  let fence: string | null = null;
  let prev: { from: number; text: string } | null = null;

  for (let n = 1; n <= doc.lines; n++) {
    const line = doc.line(n);
    const text = line.text;

    const fenceAt = /^\s{0,3}(```+|~~~+)/.exec(text);
    if (fence !== null) {
      if (fenceAt && text.trim().startsWith(fence)) fence = null;
      prev = null;
      continue;
    }
    if (fenceAt) {
      fence = fenceAt[1].slice(0, 3);
      prev = null;
      continue;
    }

    let headText: string | null = null;
    let level = 1;
    let at = line.from;
    const atx = /^\s{0,3}(#{1,6})\s+(.*)$/.exec(text);
    if (atx) {
      headText = atx[2];
      level = atx[1].length;
    } else if (prev && prev.text.trim() !== '' && /^\s{0,3}(=+|-+)\s*$/.test(text)) {
      headText = prev.text;
      level = text.trim().startsWith('=') ? 1 : 2;
      at = prev.from;
    }
    prev = { from: line.from, text };
    if (headText === null) continue;

    const base = headingSlug(headText);
    const repeats = seen.get(base) ?? 0;
    seen.set(base, repeats + 1);
    out.push({
      id: repeats === 0 ? base : `${base}-${repeats}`,
      text: headText.trim(),
      level,
      from: at,
    });
  }
  return out;
}

/** Move the caret to the heading whose slug is `slug`, and show it. `false` when there is none —
 *  the caller says so rather than scrolling to nowhere.
 *
 *  Public because a `file.md#heading` link crosses documents: the editor that opens the file is
 *  not the one that was clicked, so the host has to ask the new one to make the jump. */
export function goToHeading(view: EditorView, slug: string): boolean {
  const want = decodeAnchor(slug);
  const wantFolded = foldSlug(want);
  /** The first exact match wins; a fold-only match is kept as the fallback, so an exact one
   *  further down the document still beats it — two headings differing only by an accent are
   *  rare, and when they exist the one written exactly is the one meant. */
  let folded: number | null = null;

  for (const heading of documentHeadings(view.state.doc)) {
    if (heading.id === want) {
      jumpTo(view, heading.from);
      return true;
    }
    if (folded === null && foldSlug(heading.id) === wantFolded) folded = heading.from;
  }
  if (folded !== null) {
    jumpTo(view, folded);
    return true;
  }
  return false;
}

/** The anchor as written in the link. A table of contents generated by a tool percent-encodes
 *  anything non-ASCII, so `#perch%C3%A9-il-cst` and `#perché-il-cst` are the same heading — and
 *  a lone `%` in a hand-written anchor is a literal, not a broken escape. */
function decodeAnchor(raw: string): string {
  let text = raw;
  try {
    text = decodeURIComponent(raw);
  } catch {
    text = raw;
  }
  return text.toLowerCase();
}

/** Put the caret on a line and bring it to the top of the view — where a jump should land. */
function jumpTo(view: EditorView, pos: number) {
  const scroll = () => EditorView.scrollIntoView(pos, { y: 'start', yMargin: 24 });
  view.dispatch({ selection: { anchor: pos }, effects: scroll() });
  view.focus();

  // ⚠️ The heights above the target are not final at this point. A mermaid diagram renders
  // asynchronously, a table widget measures after its first paint, an image arrives when it
  // arrives — and every one of them that grows or shrinks *above* the target slides it out of
  // view after the scroll has already happened. From the reader's side that is indistinguishable
  // from "the link went to the wrong section", which is why it is re-asserted rather than fired
  // once: whenever the document's total height moves, the scroll is redone, until it stops
  // moving (three quiet frames) or ~40 frames have passed, whichever comes first.
  let lastHeight = view.contentHeight;
  let quiet = 0;
  let frames = 40;
  const settle = () => {
    if (frames-- <= 0) return;
    const height = view.contentHeight;
    if (height === lastHeight) {
      if (++quiet >= 3) return;
    } else {
      quiet = 0;
      lastHeight = height;
      view.dispatch({ effects: scroll() });
    }
    requestAnimationFrame(settle);
  };
  requestAnimationFrame(settle);
}

// ─── Completing a link ──────────────────────────────────────────────────
//
// Typing `](` is the moment a link's target has to be remembered exactly — a heading's id, or a
// path relative to *this* file rather than to the project root. Both are things the editor
// already knows and the writer has to look up, which is the definition of a completion worth
// having. It is also what keeps hand-written anchors honest: the list is built by the same scan
// that the jump uses, so an offered id is one that resolves.

const HEADING_SECTION = { name: 'Headings in this file', rank: 1 };
const FILE_SECTION = { name: 'Files', rank: 2 };

/** How many files are offered before the list stops being a list. Typing narrows it long before
 *  this matters; the cap is there so a monorepo does not build 40 000 objects on one keystroke. */
const MAX_FILE_COMPLETIONS = 2000;

function linkCompletions(context: CompletionContext): CompletionResult | null {
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);
  // Inside the target half of `[label](…)`, and not past a space — a URL with one is not a URL
  // this editor's own renderer would accept.
  const open = /\]\(([^()\s]*)$/.exec(before);
  if (!open) return null;
  const typed = open[1];
  const from = context.pos - typed.length;
  const hash = typed.indexOf('#');
  // `altro.md#…` — those headings live in a file this editor has not read. Saying nothing is
  // better than offering this document's headings under another document's name.
  if (hash > 0) return null;

  const options: Completion[] = [];

  if (hash !== 0) {
    const docPath = context.state.facet(markdownDocPath);
    const files = context.state.facet(markdownFileIndex)?.() ?? [];
    for (const path of files) {
      if (options.length >= MAX_FILE_COMPLETIONS) break;
      if (docPath && normalizePath(path) === normalizePath(docPath)) continue;
      const label = relativeLink(docPath, path);
      if (label) options.push({ label, type: 'text', section: FILE_SECTION });
    }
  }
  // Headings whenever an anchor is being typed, and also on an empty target: `](` with nothing
  // after it is exactly when "which heading was that called?" is the question.
  if (hash === 0 || typed === '') {
    for (const heading of documentHeadings(context.state.doc)) {
      options.push({
        label: `#${heading.id}`,
        // The heading as written reads far better than its slug, and it is what the writer is
        // actually looking for in the list.
        detail: heading.text,
        type: 'property',
        section: HEADING_SECTION,
      });
    }
  }

  if (options.length === 0) return null;
  return { from, options, validFor: /^[^()\s]*$/ };
}

/** Case- and separator-insensitive enough to tell "the same file" from "another one". */
function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase();
}

/**
 * `path` written the way it has to appear in the link: relative to the document's own folder,
 * with the characters that would end the target escaped.
 *
 * A space in a path closes the `(…)` as far as the renderer's own regex is concerned, so it is
 * percent-encoded — which is what `resolveRelativePath` decodes on the way back.
 */
function relativeLink(docPath: string | null, target: string): string {
  const to = target.replace(/\\/g, '/');
  if (!docPath) return encodeLinkPath(to);
  const fromDir = dirOf(docPath).replace(/\\/g, '/').replace(/\/+$/, '').split('/');
  const parts = to.split('/');
  let shared = 0;
  while (
    shared < fromDir.length &&
    shared < parts.length - 1 &&
    fromDir[shared].toLowerCase() === parts[shared].toLowerCase()
  ) {
    shared++;
  }
  // Nothing in common at all (a different drive, or a path from somewhere else entirely): the
  // absolute one is the only form that means anything.
  if (shared === 0) return encodeLinkPath(to);
  const up = fromDir.length - shared;
  return encodeLinkPath([...Array<string>(up).fill('..'), ...parts.slice(shared)].join('/'));
}

function encodeLinkPath(path: string): string {
  return path.replace(/[ ()]/g, (c) => encodeURIComponent(c));
}

/**
 * Following a link: **Cmd+click** (Ctrl elsewhere), the way an IDE does it.
 *
 * A plain click puts the caret where you clicked, because inside a document you are editing that
 * is what a click has to mean — a link is text like the rest of it, and one that swallowed the
 * click would leave no way to type in the middle of a title.
 *
 * ⚠️⚠️ **The listener is on `mousedown`, not on `click`.** Cancelling a `click` is far too late:
 * the browser has already put the caret at the point pressed, CodeMirror's DOM observer reads
 * that back and dispatches its own selection, and it lands *after* the jump — so the caret
 * returns to the link and the document appears not to have moved at all. Nothing looks broken,
 * nothing throws, and the link simply does nothing. Preventing the default on `mousedown` is
 * what stops the caret from being placed in the first place.
 *
 * ⚠️ And it is a **capture listener on the editor's own DOM**, not `EditorView.domEventHandlers`.
 * Those are CodeMirror's, and CodeMirror skips them for events that come from inside a widget
 * whose `ignoreEvent()` says so — which the table widget's does, since a click in a cell belongs
 * to the cell. A link inside a table cell would have been dead again, and only there.
 */
function linkFollowHandler(): Extension {
  return ViewPlugin.fromClass(
    class {
      private readonly onDown: (e: MouseEvent) => void;
      private readonly onClick: (e: MouseEvent) => void;

      constructor(private readonly view: EditorView) {
        this.onDown = (e: MouseEvent) => this.handle(e);
        // The `click` that follows a handled `mousedown` still has to be swallowed, or a
        // consumer further out (a table cell, the host's own handler) acts on it.
        this.onClick = (e: MouseEvent) => {
          if (followModifier(e) && (e.target as HTMLElement | null)?.closest?.('a[href]')) {
            e.preventDefault();
            e.stopPropagation();
          }
        };
        view.dom.addEventListener('mousedown', this.onDown, true);
        view.dom.addEventListener('click', this.onClick, true);
      }

      destroy() {
        this.view.dom.removeEventListener('mousedown', this.onDown, true);
        this.view.dom.removeEventListener('click', this.onClick, true);
      }

      private handle(event: MouseEvent) {
        if (event.button !== 0 || !followModifier(event)) return;
        const el = event.target as HTMLElement | null;
        const anchor = el?.closest?.('a[href]') as HTMLAnchorElement | null;
        if (!anchor) return;
        const href = anchor.getAttribute('href') ?? '';
        const view = this.view;
        const target = classifyLink(href, view.state.facet(markdownDocPath));
        if (!target) return;
        event.preventDefault();
        event.stopPropagation();
        if (target.kind === 'web') {
          void openUrl(target.url);
          return;
        }
        if (target.kind === 'anchor') {
          // A heading that is not there is worth saying nothing about: the link is in the
          // document the reader is looking at, and the missing heading is visible from here.
          goToHeading(view, target.slug);
          return;
        }
        // A file. The host decides what opening one means — a tab in Bennu, a note in a vault —
        // and when nobody said, the operating system does (which is the honest fallback for a
        // `.pdf` or a `.png` beside the document).
        const open = view.state.facet(markdownOpenLink);
        // ⚠️ `openPath`, not `openUrl`: the opener plugin's default permission scopes `open_url`
        // to `http/https/mailto/tel`, so handing it a filesystem path is rejected — and rejected
        // in a promise nobody awaits, which is a dead link with no error anywhere.
        if (open) open(target.path, target.anchor);
        else void openPath(target.path).catch(() => {});
      }
    },
  );
}

/** The key that turns a click into a jump — Cmd on macOS, Ctrl elsewhere, the same one
 *  that follows a symbol in the code editor. */
function followModifier(event: MouseEvent | KeyboardEvent): boolean {
  return isMac ? event.metaKey : event.ctrlKey;
}

/**
 * The hand cursor, only while the key that would follow the link is down.
 *
 * A pointer over text you can type in is a lie the rest of the time: it promises a click will
 * navigate, and a plain click doesn't. The class rides on the editor's root so the styling
 * stays in the theme, and it is dropped on blur — a window that loses focus never sees the
 * keyup, and the editor would keep the hand up forever.
 */
function followCursorHandler(): Extension {
  return ViewPlugin.fromClass(
    class {
      private readonly sync: (e: KeyboardEvent) => void;
      private readonly clear: () => void;

      constructor(private readonly view: EditorView) {
        this.sync = (e: KeyboardEvent) => this.set(followModifier(e));
        this.clear = () => this.set(false);
        window.addEventListener('keydown', this.sync, true);
        window.addEventListener('keyup', this.sync, true);
        window.addEventListener('blur', this.clear);
      }

      destroy() {
        window.removeEventListener('keydown', this.sync, true);
        window.removeEventListener('keyup', this.sync, true);
        window.removeEventListener('blur', this.clear);
      }

      private set(on: boolean) {
        this.view.dom.classList.toggle('cm-md-follow', on);
      }
    },
  );
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
        // Where it goes, without following it — the one thing a rendered link cannot say on its
        // own, since the label is the label precisely because the URL is hidden.
        a.setAttribute('title', url);
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

/**
 * A GFM table, edited **as a table**.
 *
 * ## What changed, and why
 *
 * It used to render only while the caret was elsewhere: put it in a cell and the whole thing
 * turned back into pipes and dashes. That is the opposite of what a rendered table is for — you
 * are looking at a grid because the grid is how the content reads, and losing it to edit one word
 * means the editor's answer to "change this cell" is "here is the source, find it".
 *
 * So the table stays a table. Cells are edited in place, rows and columns are added and removed
 * from the toolbar under it, and the markdown is rewritten underneath.
 *
 * ## The one thing that still reveals its source: the focused cell
 *
 * A cell is displayed as **rendered inline markdown** — that is what makes the table readable —
 * and rendered markdown cannot be typed into: reading `textContent` back out of a cell showing
 * **bold** would write `bold` and lose the asterisks. So the cell you are IN shows its own
 * source, and the others stay rendered. It is the same rule the rest of this editor follows, at
 * the granularity the complaint was about: the cell, not the table.
 *
 * ## The file is rewritten, and the padding is not kept
 *
 * Editing a cell rewrites that table's markdown from the model — `| a | b |`, one space each
 * side. A hand-aligned table loses its alignment padding the first time it is edited here, which
 * is the price of editing it as a grid rather than as text; nothing outside the table is touched.
 */
class TableWidget extends WidgetType {
  constructor(
    private readonly text: string,
    private readonly docPath: string | null,
  ) { super(); }

  /**
   * Same table, same content, same everything.
   *
   * Identity by text — but see {@link updateDOM}: when the text differs the DOM is *patched*
   * rather than rebuilt, which is what keeps the caret in the cell being typed in.
   */
  eq(other: WidgetType): boolean {
    return other instanceof TableWidget
        && other.text === this.text
        && other.docPath === this.docPath;
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement('div');
    wrap.className = 'cm-md-rendered-table-wrap';
    this.paint(wrap, view);
    // Same reason as the mermaid widget's: this one changes height too — a focused cell swaps to
    // its monospace source and can rewrap, a row is added, the toolbar appears. A block widget
    // whose size drifts from what CodeMirror measured moves every line below it out from under
    // the pointer.
    if (typeof ResizeObserver !== 'undefined') {
      const ro = new ResizeObserver(() => view.requestMeasure());
      ro.observe(wrap);
      (wrap as HTMLElement & { _tblRo?: ResizeObserver })._tblRo = ro;
    }
    return wrap;
  }

  destroy(dom: HTMLElement): void {
    (dom as HTMLElement & { _tblRo?: ResizeObserver })._tblRo?.disconnect();
  }

  /**
   * Patch the existing DOM instead of replacing it.
   *
   * Load-bearing for the whole feature: without it every keystroke in a cell produces a new
   * widget, CodeMirror swaps the DOM, and the focus — and the caret inside the cell — goes with
   * it. Here the cell that has focus is left exactly as it is (its text is already what the
   * document now says) and everything around it is re-rendered.
   */
  updateDOM(dom: HTMLElement, view: EditorView): boolean {
    const active = document.activeElement as HTMLElement | null;
    const keep = active && dom.contains(active)
      ? { row: active.dataset.row, col: active.dataset.col }
      : null;
    this.paint(dom, view);
    if (keep) {
      const back = dom.querySelector<HTMLElement>(
        `[data-row="${keep.row}"][data-col="${keep.col}"]`,
      );
      if (back) {
        this.enterCell(back);
        placeCaretAtEnd(back);
      }
    }
    return true;
  }

  /** Build (or rebuild) the whole widget into `host`. */
  private paint(host: HTMLElement, view: EditorView) {
    host.textContent = '';
    const model = parseGfmTable(this.text);
    if (!model) {
      host.textContent = this.text;
      return;
    }
    const { header, aligns, rows } = model;
    const nCols = header.length;

    const table = document.createElement('table');
    table.className = 'cm-md-rendered-table';

    // GFM lets you omit header content with `| | |` — render that as a headerless grid (no empty
    // grey bar at the top) instead of forcing a blank `<thead>` that looks broken.
    const hasHeader = header.some((c) => c.length > 0);
    if (!hasHeader) table.classList.add('cm-md-rendered-table-headerless');

    const thead = document.createElement('thead');
    const trh = document.createElement('tr');
    for (let c = 0; c < nCols; c++) {
      const th = document.createElement('th');
      if (aligns[c]) th.style.textAlign = aligns[c]!;
      // The header row is row -1 in cell coordinates: it is a row you edit like any other, and
      // it is the one row that cannot be deleted.
      this.cell(th, header[c] ?? '', -1, c, view);
      trh.appendChild(th);
    }
    thead.appendChild(trh);
    table.appendChild(thead);

    const tbody = document.createElement('tbody');
    rows.forEach((row, r) => {
      const tr = document.createElement('tr');
      for (let c = 0; c < nCols; c++) {
        const td = document.createElement('td');
        if (aligns[c]) td.style.textAlign = aligns[c]!;
        this.cell(td, row[c] ?? '', r, c, view);
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    });
    table.appendChild(tbody);
    // The bordered box, inside the padded root — see the note on `.cm-md-rendered-table-wrap`.
    const box = document.createElement('div');
    box.className = 'cm-md-rendered-table-box';
    box.appendChild(table);
    host.appendChild(box);
    host.appendChild(this.toolbar(view));
  }

  /** One editable cell. Rendered while it is not focused, source while it is. */
  private cell(el: HTMLElement, source: string, row: number, col: number, view: EditorView) {
    el.dataset.row = String(row);
    el.dataset.col = String(col);
    el.dataset.src = source;
    el.contentEditable = 'true';
    el.spellcheck = false;
    renderInlineMdInto(source, el, this.docPath);

    el.addEventListener('focus', () => this.enterCell(el));
    el.addEventListener('blur', () => {
      // Back to rendered. Read from the dataset rather than from the DOM: the document is the
      // record, and it was already updated on every input.
      el.textContent = '';
      renderInlineMdInto(el.dataset.src ?? '', el, this.docPath);
    });
    el.addEventListener('input', () => {
      // A cell holds one line. A pasted newline would produce a row the parser cannot read, so
      // it is flattened rather than refused.
      const text = (el.textContent ?? '').replace(/\r?\n/g, ' ');
      el.dataset.src = text;
      this.write(view, (m) => {
        if (row < 0) m.header[col] = text;
        else if (m.rows[row]) m.rows[row][col] = text;
      });
    });
    el.addEventListener('keydown', (e) => this.onCellKey(e, el, view));
  }

  /** Show a cell's own markdown — what you type into. */
  private enterCell(el: HTMLElement) {
    const src = el.dataset.src ?? '';
    if (el.textContent !== src) el.textContent = src;
  }

  private onCellKey(e: KeyboardEvent, el: HTMLElement, view: EditorView) {
    // Every one of these is a key the editor under the table would otherwise take.
    if (e.key === 'Tab') {
      e.preventDefault();
      e.stopPropagation();
      this.moveFocus(el, e.shiftKey ? -1 : 1, 0);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      this.moveFocus(el, 0, e.shiftKey ? -1 : 1);
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      e.stopPropagation();
      this.moveFocus(el, 0, e.key === 'ArrowDown' ? 1 : -1);
      return;
    }
    if (e.key === 'Escape') {
      // Out of the table and back into the document, after it — the way out that does not
      // involve reaching for the mouse.
      e.preventDefault();
      e.stopPropagation();
      const range = this.range(view, el);
      el.blur();
      if (range) view.dispatch({ selection: { anchor: range.to } });
      view.focus();
    }
  }

  /** Move focus by `dc` columns / `dr` rows, wrapping across the end of a row. */
  private moveFocus(from: HTMLElement, dc: number, dr: number) {
    const host = from.closest('.cm-md-rendered-table-wrap');
    if (!host) return;
    const row = Number(from.dataset.row);
    const col = Number(from.dataset.col);
    const cols = host.querySelectorAll('thead th').length;
    let r = row + dr;
    let c = col + dc;
    if (c < 0) { c = cols - 1; r -= 1; }
    if (c >= cols) { c = 0; r += 1; }
    const next = host.querySelector<HTMLElement>(`[data-row="${r}"][data-col="${c}"]`);
    if (next) { next.focus(); placeCaretAtEnd(next); }
  }

  /** The toolbar: everything structural, acting on the cell that has focus. */
  private toolbar(view: EditorView): HTMLElement {
    const bar = document.createElement('div');
    bar.className = 'cm-md-table-tools';
    // `mousedown` is prevented on the whole bar so pressing a button does not blur the cell
    // first — the focused cell IS the argument to every one of these.
    bar.addEventListener('mousedown', (e) => e.preventDefault());

    const button = (label: string, title: string, run: () => void) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'cm-md-table-tool';
      b.textContent = label;
      b.title = title;
      b.addEventListener('click', (e) => { e.preventDefault(); run(); });
      bar.appendChild(b);
      return b;
    };
    const sep = () => {
      const s = document.createElement('span');
      s.className = 'cm-md-table-sep';
      bar.appendChild(s);
    };

    const focus = () => this.focusedCell();

    button('+ row', 'Insert a row below the one you are in (or at the end)', () => {
      this.write(view, (m) => {
        const at = focus() ? Math.max(0, focus()!.row + 1) : m.rows.length;
        m.rows.splice(at, 0, new Array(m.header.length).fill(''));
      });
    });
    button('+ col', 'Insert a column right of the one you are in (or at the end)', () => {
      this.write(view, (m) => {
        const at = focus() ? focus()!.col + 1 : m.header.length;
        m.header.splice(at, 0, '');
        m.aligns.splice(at, 0, null);
        for (const r of m.rows) r.splice(at, 0, '');
      });
    });
    sep();
    button('⌫ row', 'Delete the row you are in', () => {
      const f = focus();
      if (!f || f.row < 0) return;
      this.write(view, (m) => { m.rows.splice(f.row, 1); });
    });
    button('⌫ col', 'Delete the column you are in', () => {
      const f = focus();
      if (!f || m1(this.text) <= 1) return;
      this.write(view, (m) => {
        m.header.splice(f.col, 1);
        m.aligns.splice(f.col, 1);
        for (const r of m.rows) r.splice(f.col, 1);
      });
    });
    sep();
    for (const [label, align, title] of [
      ['⌐', 'left', 'Align this column left'],
      ['≡', 'center', 'Centre this column'],
      ['¬', 'right', 'Align this column right'],
    ] as const) {
      button(label, title, () => {
        const f = focus();
        if (!f) return;
        this.write(view, (m) => { m.aligns[f.col] = m.aligns[f.col] === align ? null : align; });
      });
    }
    return bar;
  }

  /** The cell that has focus, in model coordinates (`row: -1` is the header). */
  private focusedCell(): { row: number; col: number } | null {
    const el = document.activeElement as HTMLElement | null;
    if (!el || el.dataset.row === undefined) return null;
    return { row: Number(el.dataset.row), col: Number(el.dataset.col) };
  }

  /**
   * Apply `mutate` to the table's model and write the result back into the document.
   *
   * One dispatch per change, replacing exactly the table's own range — so the undo history gets
   * table-sized steps rather than a document-sized one, and nothing outside the table moves.
   */
  private write(view: EditorView, mutate: (m: GfmTable) => void) {
    const el = (document.activeElement as HTMLElement | null) ?? undefined;
    const range = this.range(view, el);
    if (!range) return;
    const model = parseGfmTable(this.text);
    if (!model) return;
    mutate(model);
    const next = serializeGfmTable(model);
    if (next === this.text) return;
    view.dispatch({ changes: { from: range.from, to: range.to, insert: next } });
  }

  /** Where this table currently sits in the document. Resolved from the DOM at call time — the
   *  widget outlives edits above it, and a position captured at mount would drift. */
  private range(view: EditorView, from?: HTMLElement): { from: number; to: number } | null {
    const host = (from?.closest('.cm-md-rendered-table-wrap') as HTMLElement | null)
      ?? view.dom.querySelector<HTMLElement>('.cm-md-rendered-table-wrap');
    if (!host) return null;
    let pos: number;
    try {
      pos = view.posAtDOM(host);
    } catch {
      return null;
    }
    return tableRangeAt(view.state, pos);
  }

  /** The widget owns its events: a click in a cell must place the caret in the CELL, not ask
   *  CodeMirror to put one beside the widget. */
  ignoreEvent(): boolean { return true; }
}

/** How many columns the table in `text` has — for the guard that refuses to delete the last. */
function m1(text: string): number {
  return parseGfmTable(text)?.header.length ?? 0;
}

/** Put the caret at the end of an editable cell, which is where typing should resume. */
function placeCaretAtEnd(el: HTMLElement) {
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

/**
 * The document range of the table that starts at `pos`.
 *
 * Read off the lines rather than from the syntax tree: this is called from a DOM event, the tree
 * may be mid-parse, and "the run of consecutive lines containing a pipe" is exactly what a GFM
 * table is on disk.
 */
function tableRangeAt(state: EditorState, pos: number): { from: number; to: number } | null {
  const first = state.doc.lineAt(pos);
  if (!first.text.includes('|')) return null;
  let last = first;
  while (last.number < state.doc.lines) {
    const next = state.doc.line(last.number + 1);
    if (!next.text.includes('|') || !next.text.trim()) break;
    last = next;
  }
  return { from: first.from, to: last.to };
}

/** The model a table is edited through. */
interface GfmTable {
  header: string[];
  aligns: Align[];
  rows: string[][];
}

/** Model → markdown. One space each side of every cell: a normalised table, because a grid
 *  edited as a grid has no padding to preserve. */
function serializeGfmTable(t: GfmTable): string {
  const escape = (cell: string) => cell.replace(/\|/g, '\\|').trim();
  const line = (cells: string[]) =>
    `| ${Array.from({ length: t.header.length }, (_, i) => escape(cells[i] ?? '')).join(' | ')} |`;
  const sep = t.aligns
    .slice(0, t.header.length)
    .map((a) => (a === 'center' ? ':---:' : a === 'right' ? '---:' : a === 'left' ? ':---' : '---'));
  while (sep.length < t.header.length) sep.push('---');
  return [line(t.header), `| ${sep.join(' | ')} |`, ...t.rows.map(line)].join('\n');
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
  // The languages this app is built around, under the names their files carry. `jsp` has no
  // grammar of its own anywhere — markup is the honest half of it (the tags), and the half a
  // reader is usually quoting.
  props: 'properties', jsp: 'markup', jspf: 'markup',
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

/**
 * A blockquote — a quotation, or an **alert** when its first line is `[!KIND]`.
 *
 * The two are the same construct in the parser and two different things on the page, so the
 * decision is made once here, from the head line's text: Lezer leaves `[!WARNING]` as ordinary
 * text (a shortcut reference link needs a matching definition to become a Link), which is why
 * reading the line is both sufficient and the only option.
 *
 * The marker itself is replaced with the badge, and only while the caret is on another line —
 * the same reveal rule as every other marker in this editor: the line you are editing shows
 * what it really says.
 */
function decorateBlockquote(view: EditorView, node: SyntaxNodeRef, push: PushFn) {
  const first = view.state.doc.lineAt(node.from);
  // `>` and the spaces after it are the quote mark; what follows is the head line's text.
  const afterMark = first.text.replace(/^\s*>\s?/, '');
  const markLength = first.text.length - afterMark.length;
  const head = ALERT_HEAD.exec(afterMark);
  const kind = head ? alertKindOf(head[2]) : null;

  let pos = node.from;
  while (pos <= node.to) {
    const line = view.state.doc.lineAt(pos);
    if (kind) {
      push(line.from, line.from, lineDeco(`cm-md-callout cm-md-callout-${kind}`));
      if (line.from === first.from) push(line.from, line.from, lineDeco('cm-md-callout-head'));
    } else {
      push(line.from, line.from, blockquoteLine);
    }
    if (line.to >= node.to) break;
    pos = line.to + 1;
  }

  if (!kind || !head) return;
  if (selectionTouchesRange(view, first.from, first.to)) return;
  // Replace `[!KIND]` — with its fold marker and the run of spaces after it, so the author's
  // title starts where the badge ends rather than a space further along.
  const from = first.from + markLength + head[1].length;
  const marker = `[!${head[2]}]${head[3]}${head[4]}`;
  const to = from + marker.length;
  push(from, to, Decoration.replace({ widget: new AlertHeadWidget(kind, head[5].trim()) }));
  // The title the author wrote is *in* the badge now, so the text that follows on the line
  // would be a second copy of it.
  if (head[5].length) push(to, first.to, Decoration.replace({}));
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

        // ── Blockquote line decoration (and GitHub alerts) ──────────────
        if (name === 'Blockquote') {
          decorateBlockquote(view, node, push);
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
          // An anchor whether or not the source is revealed: a plain click edits, so there is
          // nothing to protect, and Cmd+click follows the link you are in the middle of writing.
          const url = urlChildOf(view, node);
          push(node.from, node.to, url ? linkAnchor(url) : linkLabel);
          return;
        }
        if (name === 'Autolink') {
          // `<https://…>` — the URL *is* the label. It had no branch at all, so it fell to the
          // "wrapped link" rule below and its URL child got concealed along with the angle
          // brackets: the whole link disappeared from the rendered document.
          const url = urlChildOf(view, node);
          push(node.from, node.to, url ? linkAnchor(url) : linkLabel);
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
        // The language a fence declares. Left visible — it is what the block IS — but drawn as a
        // LABEL rather than as code: same size, same monospace and same colour as the lines
        // under it made `dig` read as the program's first statement.
        if (name === 'CodeInfo') {
          push(node.from, node.to, codeLangMark);
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
            push(node.from, node.to, isSafeHref(urlText) ? linkAnchor(urlText) : linkLabel);
            return;
          }

          // Inside an `Autolink` the URL is not a target hidden behind a label — it is the text
          // the reader sees. Concealing it leaves an empty line.
          if (parentName === 'Autolink') return;

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

// ─── Mermaid ────────────────────────────────────────────────────────────
//
// A ```mermaid fence is the one code block whose CONTENT is not the point — nobody reads a
// sequence diagram as text. So it renders in place, under the same rule as everything else in
// this editor: the caret inside it shows the source, the caret elsewhere shows the picture.
//
// **Loaded on first sight, never before.** Mermaid is by a distance the largest thing this app
// could depend on, and a document without a diagram must not pay for it: the `import()` below is
// a split point, so the bundle it produces is fetched from disk the first time a fence appears
// and never in a session that has none.

/** The library, initialised once. `null` until something asks for it. */
let mermaidLoad: Promise<typeof import('mermaid').default> | null = null;
/** The theme the library was initialised for — the value of one CSS variable is enough of a
 *  fingerprint, and it is what changes when a theme is switched. */
let mermaidTheme = '';
/** Rendered SVG by source, so scrolling a document past a diagram does not re-lay it out. */
const mermaidCache = new Map<string, string>();
let mermaidSeq = 0;

/** The Arbor colours mermaid should draw with, read from the live theme. */
function mermaidThemeVariables(): Record<string, string> {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  const accent = v('--accent', '#3d7fff');
  const bg = v('--bg-base', '#1e1f22');
  const text = v('--text-primary', '#dfe1e5');
  return {
    background: bg,
    primaryColor: v('--bg-elevated', '#2b2d31'),
    primaryTextColor: text,
    primaryBorderColor: accent,
    secondaryColor: v('--bg-overlay', '#33353a'),
    tertiaryColor: v('--bg-hover', '#3a3d42'),
    lineColor: v('--text-muted', '#7a7d85'),
    textColor: text,
    mainBkg: v('--bg-elevated', '#2b2d31'),
    nodeBorder: accent,
    clusterBkg: v('--bg-overlay', '#33353a'),
    clusterBorder: v('--border', '#404348'),
    titleColor: text,
    edgeLabelBackground: bg,
    fontFamily: v('--font-ui-sans', 'system-ui, sans-serif'),
  };
}

async function mermaidLib() {
  const theme = getComputedStyle(document.documentElement).getPropertyValue('--bg-base').trim();
  if (mermaidLoad && theme !== mermaidTheme) {
    // The theme moved under us: re-initialise and drop the pictures drawn in the old colours.
    mermaidLoad = null;
    mermaidCache.clear();
  }
  if (!mermaidLoad) {
    mermaidTheme = theme;
    mermaidLoad = import('mermaid').then((m) => {
      m.default.initialize({
        startOnLoad: false,
        // `strict` sanitises the HTML a diagram's labels may contain. A markdown file is
        // something you are given as often as something you wrote.
        securityLevel: 'strict',
        theme: 'base',
        themeVariables: mermaidThemeVariables(),
      });
      return m.default;
    });
  }
  return mermaidLoad;
}

/** The SVG for one diagram, or a thrown error carrying mermaid's own message — which names the
 *  line it could not parse, and is the only useful thing to show for a diagram that is wrong. */
async function renderMermaid(code: string): Promise<string> {
  const hit = mermaidCache.get(code);
  if (hit) return hit;
  const mermaid = await mermaidLib();
  mermaidSeq += 1;
  const { svg } = await mermaid.render(`arbor-mmd-${mermaidSeq}`, code);
  mermaidCache.set(code, svg);
  return svg;
}

class MermaidWidget extends WidgetType {
  constructor(private readonly code: string) {
    super();
  }

  eq(other: MermaidWidget): boolean {
    return other.code === this.code;
  }

  toDOM(view: EditorView): HTMLElement {
    const host = document.createElement('div');
    host.className = 'cm-md-mermaid';
    // ⚠️⚠️ A block widget that changes size after it is inserted **breaks clicking**, and not
    // only on itself: CodeMirror's height map still holds the size it measured, so every line
    // below the diagram is drawn at one offset and hit-tested at another. Reported as "to put
    // the caret on a line I have to click the line above it" — which names the symptom and says
    // nothing about diagrams, because from a reader's chair it is not about diagrams.
    //
    // A diagram changes size at least twice: when the SVG arrives (asynchronously, from a
    // library loaded on first sight) and again when the fonts it lays text out with land. So the
    // widget is *observed* rather than measured once — a one-shot `requestMeasure` after the
    // render covers the first change and quietly loses the second.
    const remeasure = () => view.requestMeasure();
    if (typeof ResizeObserver !== 'undefined') {
      const ro = new ResizeObserver(remeasure);
      ro.observe(host);
      // The observer belongs to this DOM node: CodeMirror drops the node when the widget goes,
      // and an observer left watching a detached element is a leak per diagram scrolled past.
      (host as HTMLElement & { _mmdRo?: ResizeObserver })._mmdRo = ro;
    }
    renderMermaid(this.code)
      .then((svg) => {
        host.innerHTML = svg;
        remeasure();
      })
      .catch((e: unknown) => {
        host.classList.add('cm-md-mermaid-error');
        // Mermaid's own message names the line and what it expected there. A generic "could not
        // render" would send the reader to look for a bug in the app instead of in the diagram.
        host.textContent = e instanceof Error ? e.message : String(e);
        remeasure();
      });
    return host;
  }

  destroy(dom: HTMLElement): void {
    (dom as HTMLElement & { _mmdRo?: ResizeObserver })._mmdRo?.disconnect();
  }

  ignoreEvent(): boolean {
    return false;
  }
}

/** The language a fence declares (`\`\`\`mermaid`), lower-cased, or `''`. */
function fenceInfo(state: EditorState, node: SyntaxNodeRef): string {
  const cur = node.node.cursor();
  if (!cur.firstChild()) return '';
  do {
    if (cur.name === 'CodeInfo') return state.sliceDoc(cur.from, cur.to).trim().toLowerCase();
  } while (cur.nextSibling());
  return '';
}

/** The diagram source inside a fence — its `CodeText` children, without the ``` lines. */
function mermaidBodyOf(state: EditorState, node: SyntaxNodeRef): string {
  const cur = node.node.cursor();
  if (!cur.firstChild()) return '';
  let out = '';
  do {
    if (cur.name === 'CodeText') out += state.sliceDoc(cur.from, cur.to);
  } while (cur.nextSibling());
  return out;
}

function buildTableBlocks(state: EditorState): DecorationSet {
  const entries: Range<Decoration>[] = [];
  const docPath = state.facet(markdownDocPath);
  syntaxTree(state).iterate({
    enter: (node) => {
      if (node.name === 'Table') {
        // No `selectionTouches` gate any more: a table stays a table with the caret in it, and
        // the cell you are in is the only thing that shows its markdown (see `TableWidget`).
        // The old behaviour — the whole grid collapsing into pipes because you clicked a word —
        // is what this replaced.
        const text = state.sliceDoc(node.from, node.to);
        entries.push(
          Decoration.replace({
            widget: new TableWidget(text, docPath),
            block: true,
          }).range(node.from, node.to),
        );
        return false; // never descend — children are hidden under the block widget
      }
      // A ```mermaid fence, drawn rather than printed. Same rule as the table above it: the
      // caret inside shows the source, and the block is never descended into (the syntax
      // highlighting under a picture would be work spent on nothing).
      if (node.name === 'FencedCode' && fenceInfo(state, node) === 'mermaid') {
        if (!selectionTouches(state, node.from, node.to)) {
          const body = mermaidBodyOf(state, node);
          if (body.trim()) {
            entries.push(
              Decoration.replace({
                widget: new MermaidWidget(body),
                block: true,
              }).range(node.from, node.to),
            );
          }
        }
        return false;
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
    // ── GitHub alerts ────────────────────────────────────────────────
    //
    // A band, not a quotation: the left rule is the kind's colour, the ground is a wash of it,
    // and the text keeps its ordinary weight and slant — an alert is a thing to READ, and a box
    // of italics is a box people skip. `--md-callout` is set per kind below, so every rule here
    // is written once.
    '.cm-md-callout': {
      borderLeft: '3px solid var(--md-callout)',
      paddingLeft: '12px',
      background: 'color-mix(in srgb, var(--md-callout) 9%, transparent)',
      color: 'var(--text-primary)',
    },
    '.cm-md-callout-head': {
      paddingTop: '2px',
      fontWeight: '600',
    },
    '.cm-md-callout-note':      { '--md-callout': 'var(--info, #4d9be6)' },
    '.cm-md-callout-tip':       { '--md-callout': 'var(--success, #6a9956)' },
    // Purple, as GitHub draws it — and as a *third* colour, which is the point: note is blue
    // and the accent is blue too in most themes, so borrowing it here would make the two kinds
    // that mean the least alike look identical.
    '.cm-md-callout-important': { '--md-callout': 'var(--syntax-constant, #9876aa)' },
    '.cm-md-callout-warning':   { '--md-callout': 'var(--warning, #ffc44d)' },
    '.cm-md-callout-caution':   { '--md-callout': 'var(--error, #e05252)' },
    '.cm-md-callout-question':  { '--md-callout': 'var(--syntax-type, #4d9be6)' },
    '.cm-md-callout-example':   { '--md-callout': 'var(--accent)' },
    // A quotation IS the plain blockquote, dressed as a callout because that is how it was
    // written: muted, and the only kind that keeps the italics.
    '.cm-md-callout-quote':     { '--md-callout': 'var(--text-muted)', fontStyle: 'italic' },
    // The badge that stands in for `[!WARNING]`.
    '.cm-md-callout-badge': {
      display: 'inline-flex',
      alignItems: 'center',
      gap: '6px',
      color: 'var(--md-callout)',
      fontWeight: '700',
      letterSpacing: '0.01em',
    },
    '.cm-md-callout-badge svg': {
      width: '15px',
      height: '15px',
      flex: 'none',
    },
    // A rendered diagram. Centred and given room, because it is a figure rather than a line of
    // text — and on its own ground, so a diagram whose own background is transparent does not
    // pick up the tint of the code block it replaced.
    '.cm-md-mermaid': {
      display: 'flex',
      justifyContent: 'center',
      padding: '12px 0',
      overflowX: 'auto',
    },
    '.cm-md-mermaid svg': { maxWidth: '100%', height: 'auto' },
    // A diagram that does not parse says why, in mermaid's own words, where the picture would
    // have been — the source is one click away (put the caret in it) and the message names the
    // line it stopped at.
    '.cm-md-mermaid-error': {
      display: 'block',
      whiteSpace: 'pre-wrap',
      fontFamily: 'var(--font-code)',
      fontSize: '0.9em',
      color: 'var(--error)',
      background: 'color-mix(in srgb, var(--error) 8%, transparent)',
      border: '1px solid color-mix(in srgb, var(--error) 35%, transparent)',
      borderRadius: 'var(--radius-sm)',
      padding: '10px 12px',
    },
    // The fence's language word: a caption on the block, not a line of it.
    // ── The editable table ───────────────────────────────────────────
    // A cell you can type in has to look like one: a focus ring that says where you are, and a
    // hover that says the whole grid is live rather than a picture of one.
    '.cm-md-rendered-table td, .cm-md-rendered-table th': { outline: 'none' },
    '.cm-md-rendered-table td:hover, .cm-md-rendered-table th:hover': {
      background: 'color-mix(in srgb, var(--accent) 7%, transparent)',
    },
    '.cm-md-rendered-table td:focus, .cm-md-rendered-table th:focus': {
      background: 'color-mix(in srgb, var(--accent) 12%, transparent)',
      boxShadow: 'inset 0 0 0 2px var(--accent)',
      // The focused cell shows its own source, which is code — and reads as code.
      fontFamily: 'var(--font-code)',
      fontSize: '0.92em',
    },
    // The structural controls. Out of the way until the table is used: a toolbar under every
    // table in a long document would be a row of buttons every few paragraphs.
    '.cm-md-table-tools': {
      display: 'flex',
      alignItems: 'center',
      gap: '2px',
      padding: '3px 0 0',
      opacity: '0',
      transition: 'opacity var(--transition-fast, 120ms)',
    },
    '.cm-md-rendered-table-wrap:hover .cm-md-table-tools, .cm-md-rendered-table-wrap:focus-within .cm-md-table-tools': {
      opacity: '1',
    },
    '.cm-md-table-tool': {
      font: 'inherit',
      fontSize: 'var(--font-size-2xs)',
      lineHeight: '1',
      padding: '3px 7px',
      color: 'var(--text-muted)',
      background: 'transparent',
      border: '1px solid transparent',
      borderRadius: 'var(--radius-sm)',
      cursor: 'pointer',
    },
    '.cm-md-table-tool:hover': {
      color: 'var(--text-primary)',
      background: 'var(--bg-hover)',
      borderColor: 'var(--border-subtle)',
    },
    '.cm-md-table-sep': {
      width: '1px',
      height: '13px',
      margin: '0 4px',
      background: 'var(--border-subtle)',
    },
    '.cm-md-code-lang': {
      fontFamily: 'var(--font-ui-sans)',
      fontSize: '0.78em',
      fontStyle: 'italic',
      letterSpacing: '0.03em',
      color: 'var(--text-faint)',
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
    '.cm-content a[href]': { color: 'var(--accent)' },
    // The hand and the underline appear only while the key that would follow the link is held —
    // a pointer over text you are editing promises a jump that a plain click does not make.
    '&.cm-md-follow .cm-content a[href]': { cursor: 'pointer' },
    '&.cm-md-follow .cm-content a[href]:hover': {
      textDecoration: 'underline',
      textUnderlineOffset: '2px',
      textDecorationThickness: '2px',
    },
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

    // ── The table widget ─────────────────────────────────────────────
    //
    // ⚠️⚠️ **PADDING, never MARGIN, on a block widget's root.** CodeMirror measures a widget by
    // its box, and a margin is outside the box: 10px top and bottom meant the table occupied 20
    // more pixels on screen than the height map believed, so every line BELOW it was drawn about
    // one line lower than CodeMirror thought it was. Reported as "to put the caret on a line I
    // have to click the one above" — a symptom that names neither tables nor margins, which is
    // why it survived two wrong guesses (a stale measure, then an async resize).
    //
    // The border and the background move onto an inner box so the padding does not draw as a
    // 10px-taller frame around the table.
    '.cm-md-rendered-table-wrap': {
      padding: '10px 0',
    },
    '.cm-md-rendered-table-box': {
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
  /** Open a file a link points at. Omitted → the operating system opens it. */
  onOpenLink?: ((path: string, anchor: string | null) => void) | null;
  /** The files `](` may complete to — absolute paths, written into the link relative to the
   *  document. Omitted → only this file's headings are offered. */
  fileIndex?: (() => string[]) | null;
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
    markdownOpenLink.of(opts.onOpenLink ?? null),
    markdownFileIndex.of(opts.fileIndex ?? null),
    linkFollowHandler(),
    followCursorHandler(),
    autocompletion({ override: [linkCompletions] }),
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
