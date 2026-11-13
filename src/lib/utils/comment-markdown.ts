/**
 * Markdown in line comments — the markdown editor's live preview, drawn on a run of `//` lines.
 *
 * ## What a run is
 *
 * Consecutive full-line comments with the same indentation and the same marker (`//`, `///`, `//!`) are
 * one markdown text: the marker and one space are taken off every line, and what is left is parsed
 * as a document. So a paragraph wraps across lines, a list keeps going, a table is a table and a
 * ```` ``` ```` fence is a code block — exactly as in a `.md`. A trailing comment (`x = 1; // why`)
 * is not part of any run: half a line of prose next to code is not a document.
 *
 * ## One walk, two surfaces
 *
 * Nothing here decides how `**bold**` or a callout looks. The walk is the markdown editor's own
 * ({@link decorateMarkdown} / {@link markdownBlocks}), run on a {@link MarkdownSurface} that maps the
 * comment text's positions back into the code file. A fix to the preview lands in both places.
 *
 * ## The reveal rule
 *
 * The same as the markdown editor's, at the same granularity: the line the caret is on shows its
 * `//`, every other line of the run reads as prose. Markers inside a line (`**`, `` ` ``, `#`) reveal
 * per element, as they do in a `.md`.
 *
 * ## The one assumption
 *
 * A line that starts with the comment token is a comment. True for a language whose strings do not
 * span lines (Patina). Rust's can, so a line of a multi-line string literal that begins with `//`
 * would be read as a comment — rare enough to accept rather than wait on the syntax tree, which a
 * stream mode only parses as far as the viewport.
 */

import { markdownLanguage } from '@codemirror/lang-markdown';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';
import { type EditorState, type Extension, type Range, StateField, type Text } from '@codemirror/state';
import type { Tree } from '@lezer/common';
import {
  decorateMarkdown,
  markdownBlocks,
  markdownElementsTheme,
  selectionTouches,
  type MarkdownEmbedding,
  type MarkdownSurface,
} from './markdown-editor';

/** One host line of a run. Every field but `vFrom` is a document position. */
interface CommentLine {
  from: number;
  to: number;
  /** Where the comment marker starts — the end of the indentation. */
  marker: number;
  /** Where the markdown starts — after the marker and its one space. */
  md: number;
  /** Where this line starts in the run's markdown text. */
  vFrom: number;
}

export interface CommentRun {
  lines: CommentLine[];
  /** The markdown: every line's text after its marker, joined by newlines. */
  text: string;
  embedding: MarkdownEmbedding;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * The full-line comment of a language whose line comment is `token`: indentation, the marker — the
 * token, or the token plus one more of its last character (`///`, the outer doc comment) or a `!`
 * (`//!`, the inner one) — and one space. Four or more (`////`) is a ruler, not a comment with
 * text, and ends a run.
 */
export function lineCommentPattern(token: string): RegExp {
  const last = escapeRegExp(token[token.length - 1]);
  return new RegExp(`^([ \\t]*)(${escapeRegExp(token)}(?:${last}|!)?)(?!${last})( ?)`);
}

/** The columns `indent` occupies, tabs expanded. */
function columns(indent: string, tabSize: number): number {
  let col = 0;
  for (const ch of indent) col = ch === '\t' ? col + tabSize - (col % tabSize) : col + 1;
  return col;
}

/** Every run of full-line comments in `doc`. */
export function commentRuns(doc: Text, pattern: RegExp, tabSize = 4): CommentRun[] {
  const runs: CommentRun[] = [];
  let key: string | null = null;
  let lines: CommentLine[] = [];
  let parts: string[] = [];
  let vPos = 0;

  const close = () => {
    if (key != null && lines.length) {
      const indent = /^[ \t]*/.exec(key)![0];
      const head = key;
      runs.push({
        lines,
        text: parts.join('\n'),
        embedding: {
          linePrefix: `${head} `,
          indent: columns(indent, tabSize),
          strip: (line) => {
            const m = pattern.exec(line);
            return m && m[1] + m[2] === head ? line.slice(m[0].length) : null;
          },
        },
      });
    }
    key = null;
    lines = [];
    parts = [];
    vPos = 0;
  };

  for (let n = 1; n <= doc.lines; n++) {
    const line = doc.line(n);
    const m = pattern.exec(line.text);
    if (!m) {
      close();
      continue;
    }
    // A change of indentation or of marker is a different comment, even with no blank line between.
    const lineKey = m[1] + m[2];
    if (lineKey !== key) close();
    key = lineKey;
    const md = line.from + m[0].length;
    lines.push({ from: line.from, to: line.to, marker: line.from + m[1].length, md, vFrom: vPos });
    const text = line.text.slice(m[0].length);
    parts.push(text);
    vPos += text.length + 1;
  }
  close();
  return runs;
}

// ── Parsing, cached ─────────────────────────────────────────────────────────

/** Trees by text. A keystroke re-parses only the run it touched; the others hit the cache. */
const trees = new Map<string, Tree>();
const MAX_TREES = 256;

function parseMarkdown(text: string): Tree {
  let tree = trees.get(text);
  if (!tree) {
    // A cache, not a memory: it starts over rather than growing with every edit of a session.
    if (trees.size >= MAX_TREES) trees.clear();
    tree = markdownLanguage.parser.parse(text);
    trees.set(text, tree);
  }
  return tree;
}

// ── The surface of a run ────────────────────────────────────────────────────

/** The run's line holding markdown position `v`. */
function lineIndexAt(run: CommentRun, v: number): number {
  let lo = 0;
  let hi = run.lines.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (run.lines[mid].vFrom <= v) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** Markdown position → document position. The newline between two lines maps to the first one's end. */
function toDoc(run: CommentRun, v: number): number {
  const line = run.lines[lineIndexAt(run, v)];
  return line.md + Math.min(v - line.vFrom, line.to - line.md);
}

function runSurface(
  state: EditorState,
  run: CommentRun,
  inline: Range<Decoration>[],
  blocks: Range<Decoration>[],
): MarkdownSurface {
  return {
    state,
    length: run.text.length,
    embedding: run.embedding,
    slice: (from, to) => run.text.slice(from, to),
    lineAt: (pos) => {
      const line = run.lines[lineIndexAt(run, pos)];
      return { from: line.vFrom, to: line.vFrom + (line.to - line.md) };
    },
    touches: (from, to) => selectionTouches(state, toDoc(run, from), toDoc(run, to)),
    inline: (from, to, deco) => {
      const a = toDoc(run, from);
      const b = toDoc(run, to);
      // An empty mark is nothing to draw; an empty replace is only something when it carries a widget.
      if (b < a || (b === a && !deco.spec.widget)) return;
      inline.push(deco.range(a, b));
    },
    line: (pos, deco) => inline.push(deco.range(run.lines[lineIndexAt(run, pos)].from)),
    // Whole host lines, indentation and markers included: a table cannot sit next to a `//`.
    block: (from, to, widget) =>
      blocks.push(
        Decoration.replace({ widget, block: true }).range(
          run.lines[lineIndexAt(run, from)].from,
          run.lines[lineIndexAt(run, to)].to,
        ),
      ),
  };
}

// ── Decorations ─────────────────────────────────────────────────────────────

/** The `//` and its space, off the line the caret is on. */
const markerConceal = Decoration.replace({});
/** The indentation keeps the code's font, so a comment lines up with the code around it. */
const indentMark = Decoration.mark({ class: 'cm-cmd-indent' });

/** Every line of a run: the prose font, and the indentation a band (quote, callout, code) starts at. */
const lineDecos = new Map<number, Decoration>();
function commentLine(indent: number): Decoration {
  let deco = lineDecos.get(indent);
  if (!deco) {
    deco = Decoration.line({ class: 'cm-cmd-line', attributes: { style: `--cmd-indent: ${indent}ch` } });
    lineDecos.set(indent, deco);
  }
  return deco;
}

function buildCommentDecorations(state: EditorState, pattern: RegExp): DecorationSet {
  const inline: Range<Decoration>[] = [];
  const blocks: Range<Decoration>[] = [];
  for (const run of commentRuns(state.doc, pattern, state.tabSize)) {
    const tree = parseMarkdown(run.text);
    const surface = runSurface(state, run, inline, blocks);
    markdownBlocks(tree, surface);
    decorateMarkdown(tree, surface);
    const line = commentLine(run.embedding.indent);
    for (const l of run.lines) {
      inline.push(line.range(l.from));
      if (l.marker > l.from) inline.push(indentMark.range(l.from, l.marker));
      if (!selectionTouches(state, l.from, l.to)) inline.push(markerConceal.range(l.marker, l.md));
    }
  }
  // A block widget stands in for its lines: nothing else may decorate them, or CodeMirror would be
  // asked to draw a mark inside a range it has already replaced.
  const covered = (r: Range<Decoration>) => blocks.some((b) => r.from >= b.from && r.to <= b.to);
  return Decoration.set([...blocks, ...inline.filter((r) => !covered(r))], true);
}

// ── Theme ───────────────────────────────────────────────────────────────────

/**
 * What changes when markdown lives in a code editor instead of on its own page.
 *
 * - **Prose font, upright.** The comment colour stays — a comment must still read as one next to
 *   the code — but the italic every comment carries in the code theme is undone, or every
 *   paragraph would be slanted and `*emphasis*` would have nothing to stand out from.
 * - **Bands start at the comment's indentation.** A quote, a callout and a code block are drawn
 *   with a rule and a wash across their line; in a `.md` the line starts at the margin, here it
 *   starts at column 0 of an indented function. So the band is drawn by a `::before` placed at
 *   `--cmd-indent` instead — absolutely positioned and click-through, which keeps the line's own
 *   box (the thing CodeMirror hit-tests) exactly as it was.
 */
const commentTheme = EditorView.theme({
  '.cm-cmd-line': {
    position: 'relative',
    // A stacking context of its own, so the band's `z-index: -1` lands behind the text and not
    // behind the editor's background.
    zIndex: '0',
    fontFamily: 'var(--font-ui-sans)',
    fontStyle: 'normal',
  },
  '.cm-cmd-line span': { fontStyle: 'inherit' },
  '.cm-cmd-line .cm-md-italic': { fontStyle: 'italic' },
  '.cm-cmd-indent': { fontFamily: 'var(--font-code)' },
  '.cm-cmd-line.cm-md-blockquote, .cm-cmd-line.cm-md-callout, .cm-cmd-line.cm-md-codeblock-line': {
    borderLeft: 'none',
    background: 'none',
  },
  '.cm-cmd-line.cm-md-blockquote::before, .cm-cmd-line.cm-md-callout::before, .cm-cmd-line.cm-md-codeblock-line::before': {
    content: '""',
    position: 'absolute',
    top: '0',
    bottom: '0',
    right: '0',
    // `ch` of the code font: the indentation is drawn in it (see `.cm-cmd-indent`).
    fontFamily: 'var(--font-code)',
    left: 'var(--cmd-indent, 0)',
    pointerEvents: 'none',
    zIndex: '-1',
  },
  '.cm-cmd-line.cm-md-blockquote::before': {
    borderLeft: '3px solid var(--accent)',
    background: 'rgba(77,120,204,0.04)',
  },
  '.cm-cmd-line.cm-md-callout::before': {
    borderLeft: '3px solid var(--md-callout)',
    background: 'color-mix(in srgb, var(--md-callout) 9%, transparent)',
  },
  '.cm-cmd-line.cm-md-codeblock-line::before': {
    background: 'var(--bg-overlay)',
  },
});

/**
 * Render the full-line comments of a language whose line comment is `lineComment` as live markdown.
 *
 * A state field rather than a view plugin: tables and diagrams are block widgets, and CodeMirror
 * takes those only from state. The whole document is scanned — fine for the size of a script, and
 * the parse of every run that did not change comes from the cache.
 */
export function commentMarkdown(lineComment: string): Extension {
  const pattern = lineCommentPattern(lineComment);
  const field = StateField.define<DecorationSet>({
    create: (state) => buildCommentDecorations(state, pattern),
    update: (value, tr) => (tr.docChanged || tr.selection ? buildCommentDecorations(tr.state, pattern) : value),
    provide: (f) => EditorView.decorations.from(f),
  });
  return [field, markdownElementsTheme, commentTheme];
}
