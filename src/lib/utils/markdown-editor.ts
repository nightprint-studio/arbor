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
  keymap,
} from '@codemirror/view';
import {
  EditorState,
  Compartment,
  type Extension,
  type Range,
} from '@codemirror/state';
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
const listMarker  = Decoration.mark({ class: 'cm-md-list-marker' });
const taskUnchecked = Decoration.mark({ class: 'cm-md-task-unchecked' });
const taskChecked   = Decoration.mark({ class: 'cm-md-task-checked' });

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
          // No special widget rendering yet — the alt text just shows
          // through. Markup conceal happens via the per-marker scoping
          // below (the parent's range is the Image node's range).
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
          // Inside a Link: dim when cursor is inside the link, fully
          // conceal otherwise. Inside an Image: always conceal off-
          // component so only the alt text shows.
          const parent = node.node.parent;
          const scope  = parent ? { from: parent.from, to: parent.to } : lineSpan(view, node.from);
          if (selectionTouchesRange(view, scope.from, scope.to)) {
            push(node.from, node.to, linkUrlDim);
          } else {
            push(node.from, node.to, concealMark);
          }
          return;
        }

        // ── List markers + task list checkboxes ─────────────────────────
        if (name === 'ListMark') { push(node.from, node.to, listMarker); return; }
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
  const { readOnly = false } = opts;
  return [
    markdownTheme,
    markdown({ base: markdownLanguage, codeLanguages: [] }),
    syntaxHighlighting(markdownHighlight),
    livePreview,
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
