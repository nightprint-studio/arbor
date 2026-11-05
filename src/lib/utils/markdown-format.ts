/**
 * Markdown formatting for the shared editor — what the caret is in, and the commands that
 * apply or remove it.
 *
 * **One table, many surfaces.** {@link MARKDOWN_ACTIONS} is the single list of formatting
 * actions: the toolbar draws buttons from it, the keymap binds its `key`s, and a product's
 * command palette lists it. Each action carries its command *and* a predicate over
 * {@link FormatState}, so a button's pressed look and what a second press does come from the
 * same place and cannot disagree.
 *
 * **Toggles are real toggles.** Bold on bold text removes the bold; a list command on a list
 * line takes the list off; a code block toggles its fences. What is "on" is read from the text
 * of the line(s) the selection touches (and from the Lezer tree for fenced code), never from
 * the whole document, so the state can follow every selection change at the cost of scanning
 * one line.
 *
 * The text work is pure ({@link inlineSpans}, {@link blockFormatOf}, {@link toggleListKind}, …)
 * and is what the tests cover; the commands only translate a CodeMirror selection into them.
 * Opt-in: a host mounts {@link markdownFormatKeymap}, because the `Alt` family is not
 * something every markdown surface wants.
 */

import { syntaxTree } from '@codemirror/language';
import {
  EditorSelection,
  Prec,
  type EditorState,
  type Extension,
  type SelectionRange,
} from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import type { SyntaxNode } from '@lezer/common';

// ─── Inline: spans on one line ───────────────────────────────────────────────

export type InlineKind = 'bold' | 'italic' | 'strike' | 'highlight' | 'code';

export interface InlineSpan {
  kind: InlineKind;
  /** Start of the opening marker, and end of the closing one — offsets into the line. */
  from: number;
  to: number;
  /** Length of one marker (the opening and the closing are the same length). */
  markLen: number;
}

const MARK_LEN: Record<Exclude<InlineKind, 'code'>, number> = {
  bold: 2,
  italic: 1,
  strike: 2,
  highlight: 2,
};

/** The marker text that creates each kind. */
export const INLINE_MARKER: Record<InlineKind, string> = {
  bold: '**',
  italic: '*',
  strike: '~~',
  highlight: '==',
  code: '`',
};

/**
 * Every emphasis span on one line.
 *
 * Deliberately line-local and forgiving rather than a CommonMark implementation: it has to run
 * on each selection change, and the editor's own parser already draws the real thing. What it
 * needs to get right is "is the caret inside bold here", and for that the flanking rule is
 * enough — an opening marker must be followed by a non-space and a closing one preceded by one,
 * which is also what keeps `2 * 3` and a `* ` bullet from opening anything. Code spans are
 * skipped whole, so `` `a*b` `` has no italics; `\*` is an escape.
 */
export function inlineSpans(text: string): InlineSpan[] {
  const spans: InlineSpan[] = [];
  const open: Partial<Record<Exclude<InlineKind, 'code'>, number>> = {};

  const toggle = (kind: Exclude<InlineKind, 'code'>, pos: number): void => {
    const len = MARK_LEN[kind];
    const start = open[kind];
    if (start == null) {
      const next = text[pos + len];
      if (next !== undefined && !/\s/.test(next)) open[kind] = pos;
      return;
    }
    const prev = text[pos - 1];
    if (prev !== undefined && !/\s/.test(prev) && pos > start + len) {
      spans.push({ kind, from: start, to: pos + len, markLen: len });
      delete open[kind];
    }
  };

  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '\\') {
      i += 2;
      continue;
    }
    if (c === '`') {
      let run = 1;
      while (text[i + run] === '`') run += 1;
      const fence = '`'.repeat(run);
      const close = text.indexOf(fence, i + run);
      if (close === -1) {
        i += run;
      } else {
        spans.push({ kind: 'code', from: i, to: close + run, markLen: run });
        i = close + run;
      }
      continue;
    }
    if (c === '*') {
      let run = 1;
      while (text[i + run] === '*') run += 1;
      if (run === 1) toggle('italic', i);
      else if (run === 2) toggle('bold', i);
      else if (run === 3) {
        // `***x***` is <em><strong>x</strong></em>: italic outermost, so it opens first and
        // closes last.
        if (open.bold != null) {
          toggle('bold', i);
          toggle('italic', i + 2);
        } else if (open.italic != null) {
          toggle('italic', i);
          toggle('bold', i + 1);
        } else {
          toggle('italic', i);
          toggle('bold', i + 1);
        }
      }
      i += run;
      continue;
    }
    if ((c === '~' || c === '=') && text[i + 1] === c) {
      toggle(c === '~' ? 'strike' : 'highlight', i);
      i += 2;
      continue;
    }
    i += 1;
  }
  return spans;
}

export interface LinkSpan {
  from: number;
  to: number;
  /** The visible text, and where it sits in the line. */
  label: string;
  labelFrom: number;
}

/** `[label](url)` on one line; an image (`![…](…)`) is not a link here. */
export function linkSpans(text: string): LinkSpan[] {
  const out: LinkSpan[] = [];
  for (const m of text.matchAll(/\[([^\]]*)\]\(([^)]*)\)/g)) {
    const from = m.index ?? 0;
    if (from > 0 && text[from - 1] === '!') continue;
    out.push({ from, to: from + m[0].length, label: m[1], labelFrom: from + 1 });
  }
  return out;
}

/** Is `[from, to]` inside `[spanFrom, spanTo]`? A caret has to be strictly inside — just
 *  before an opening marker or just after a closing one is outside the formatting. */
function within(spanFrom: number, spanTo: number, from: number, to: number): boolean {
  return from === to ? from > spanFrom && from < spanTo : from >= spanFrom && to <= spanTo;
}

/** The span of `kind` containing the selection, if any. */
export function spanAt(
  text: string,
  kind: InlineKind,
  from: number,
  to: number,
): InlineSpan | undefined {
  return inlineSpans(text).find((s) => s.kind === kind && within(s.from, s.to, from, to));
}

export function linkAt(text: string, from: number, to: number): LinkSpan | undefined {
  return linkSpans(text).find((s) => within(s.from, s.to, from, to));
}

// ─── Inline: wrapping when nothing is there to remove ────────────────────────

export interface Wrapped {
  text: string;
  /** Where the selection starts and ends inside `text`. */
  from: number;
  to: number;
}

/**
 * Wrap `selected` in `marker` — or unwrap it when it is itself wrapped (the user selected
 * `**word**` whole). Removing a marker that surrounds the selection is {@link spanAt}'s job.
 * An empty selection yields an empty pair with the caret between the markers.
 */
export function wrapInline(selected: string, marker: string): Wrapped {
  const m = marker.length;
  if (selected.length >= 2 * m && selected.startsWith(marker) && selected.endsWith(marker)) {
    const inner = selected.slice(m, selected.length - m);
    return { text: inner, from: 0, to: inner.length };
  }
  return { text: `${marker}${selected}${marker}`, from: m, to: m + selected.length };
}

// ─── Block: lines ────────────────────────────────────────────────────────────

export type ListKind = 'bullet' | 'numbered' | 'task';

export interface BlockFormat {
  /** 0 when the (first) line is not a heading, else 1–6. */
  heading: number;
  /** The list kind every non-empty line shares, else `null`. */
  list: ListKind | null;
  quote: boolean;
  callout: boolean;
}

const TASK_RE = /^\s*[-*+]\s+\[[ xX]\](?:\s|$)/;
const BULLET_RE = /^\s*[-*+]\s/;
const NUMBERED_RE = /^\s*\d+[.)]\s/;
const LIST_MARKER_RE = /^(\s*)(?:[-*+]\s+\[[ xX]\]\s+|[-*+]\s+|\d+[.)]\s+)/;

function listKindOf(line: string): ListKind | null {
  if (line.startsWith('>')) return null;
  if (TASK_RE.test(line)) return 'task';
  if (BULLET_RE.test(line)) return 'bullet';
  if (NUMBERED_RE.test(line)) return 'numbered';
  return null;
}

/** What a block of lines is, as far as the toolbar cares. */
export function blockFormatOf(lines: string[]): BlockFormat {
  const body = lines.filter((l) => l.trim().length > 0);
  if (body.length === 0) return { heading: 0, list: null, quote: false, callout: false };
  const first = body[0];
  const kinds = body.map(listKindOf);
  return {
    heading: /^(#{1,6})\s/.exec(first)?.[1].length ?? 0,
    list: kinds.every((k) => k === kinds[0]) ? kinds[0] : null,
    quote: body.every((l) => l.startsWith('>')),
    callout: /^>\s*\[![^\]]+\][+-]?/.test(first),
  };
}

const LIST_PREFIX: Record<Exclude<ListKind, 'numbered'>, string> = {
  bullet: '- ',
  task: '- [ ] ',
};

/**
 * Apply `kind` to a block of lines, or take it off when every line already has it.
 *
 * Switching kinds converts rather than stacking: a bullet line made a task becomes `- [ ] x`,
 * not `- [ ] - x`. Numbered lists are numbered 1…n so the source reads the way it renders.
 * Blank lines and quoted lines are left alone.
 */
export function toggleListKind(lines: string[], kind: ListKind): string[] {
  const body = lines.filter((l) => l.trim().length > 0 && !l.startsWith('>'));
  const allHave = body.length > 0 && body.every((l) => listKindOf(l) === kind);
  let n = 0;
  return lines.map((l) => {
    if (!l.trim() || l.startsWith('>')) return l;
    const bare = l.replace(LIST_MARKER_RE, '$1');
    if (allHave) return bare;
    n += 1;
    const indent = /^\s*/.exec(bare)?.[0] ?? '';
    const rest = bare.slice(indent.length);
    return `${indent}${kind === 'numbered' ? `${n}. ` : LIST_PREFIX[kind]}${rest}`;
  });
}

/**
 * Set the heading level of the first line. `level` 0 removes the heading; asking for the level
 * the line already has also removes it when `toggle` is set (the keyboard's meaning), and keeps
 * it when not (a picker that says "Heading 2" twice means Heading 2).
 */
export function setHeadingLines(lines: string[], level: number, toggle = false): string[] {
  const [line = '', ...rest] = lines;
  const had = /^(#{1,6})\s/.exec(line)?.[1].length ?? 0;
  const bare = line.replace(/^#{1,6}\s+/, '');
  const clear = level === 0 || (toggle && had === level);
  return [clear ? bare : `${'#'.repeat(level)} ${bare}`, ...rest];
}

/**
 * One press: plain → quote. Second press: quote → callout (a header line is added). Third:
 * callout → plain. The state is read off the lines, so it survives a reload.
 */
export function cycleQuote(lines: string[]): string[] {
  const nonEmpty = lines.filter((l) => l.trim().length > 0);
  const quoted = nonEmpty.length > 0 && nonEmpty.every((l) => l.startsWith('>'));
  if (!quoted) return lines.map((l) => (l.trim() ? `> ${l}` : l));
  if (/^>\s*\[![^\]]+\][+-]?/.test(lines[0])) {
    return lines.slice(1).map((l) => l.replace(/^>\s?/, ''));
  }
  return ['> [!note]', ...lines];
}

/** `| Column | Column |` skeleton with `rows` empty body rows. */
export function tableSkeleton(columns = 2, rows = 1): string {
  const row = (cell: string) => `| ${Array.from({ length: columns }, () => cell).join(' | ')} |`;
  return [row('Column'), row('---'), ...Array.from({ length: rows }, () => row(' '))].join('\n');
}

// ─── The state a toolbar shows ───────────────────────────────────────────────

export interface FormatState {
  bold: boolean;
  italic: boolean;
  strike: boolean;
  highlight: boolean;
  code: boolean;
  link: boolean;
  heading: number;
  list: ListKind | null;
  quote: boolean;
  callout: boolean;
  codeBlock: boolean;
}

export const NO_FORMAT: FormatState = {
  bold: false,
  italic: false,
  strike: false,
  highlight: false,
  code: false,
  link: false,
  heading: 0,
  list: null,
  quote: false,
  callout: false,
  codeBlock: false,
};

export function sameFormat(a: FormatState, b: FormatState): boolean {
  return (
    a.bold === b.bold && a.italic === b.italic && a.strike === b.strike &&
    a.highlight === b.highlight && a.code === b.code && a.link === b.link &&
    a.heading === b.heading && a.list === b.list && a.quote === b.quote &&
    a.callout === b.callout && a.codeBlock === b.codeBlock
  );
}

/** Lines scanned for a block state; past this a selection is "a lot of text", not a format. */
const MAX_BLOCK_LINES = 200;

/** The fenced code block containing `pos`, if any. */
function fencedBlockAt(state: EditorState, pos: number): { from: number; to: number } | null {
  for (let n: SyntaxNode | null = syntaxTree(state).resolveInner(pos, 1); n; n = n.parent) {
    if (n.name === 'FencedCode') return { from: n.from, to: n.to };
  }
  return null;
}

/**
 * What the selection is in. Reads the line(s) the selection touches and the syntax tree at its
 * head — nothing proportional to the document, so it is safe to call on every selection change.
 */
export function markdownFormatState(state: EditorState): FormatState {
  const sel = state.selection.main;
  const first = state.doc.lineAt(sel.from);
  const last = state.doc.lineAt(sel.to);
  const lines: string[] = [];
  for (let n = first.number; n <= Math.min(last.number, first.number + MAX_BLOCK_LINES); n += 1) {
    lines.push(state.doc.line(n).text);
  }
  const block = blockFormatOf(lines);
  const codeBlock = fencedBlockAt(state, sel.head) != null;

  // A fenced line is code, not prose: nothing inline applies inside it.
  const inline =
    first.number === last.number && !codeBlock
      ? {
          bold: spanAt(first.text, 'bold', sel.from - first.from, sel.to - first.from) != null,
          italic: spanAt(first.text, 'italic', sel.from - first.from, sel.to - first.from) != null,
          strike: spanAt(first.text, 'strike', sel.from - first.from, sel.to - first.from) != null,
          highlight: spanAt(first.text, 'highlight', sel.from - first.from, sel.to - first.from) != null,
          code: spanAt(first.text, 'code', sel.from - first.from, sel.to - first.from) != null,
          link: linkAt(first.text, sel.from - first.from, sel.to - first.from) != null,
        }
      : { bold: false, italic: false, strike: false, highlight: false, code: false, link: false };

  return { ...inline, ...block, codeBlock };
}

// ─── CodeMirror commands ─────────────────────────────────────────────────────

export type EditorCommand = (view: EditorView) => boolean;

/** Remove `[open, open+len)` and `[close, close+len)`, mapping a selection endpoint across. */
function unwrapChange(
  open: number,
  close: number,
  len: number,
  range: SelectionRange,
): { changes: { from: number; to: number }[]; range: SelectionRange } {
  const map = (p: number) =>
    p - Math.min(Math.max(p - open, 0), len) - Math.min(Math.max(p - close, 0), len);
  return {
    changes: [
      { from: open, to: open + len },
      { from: close, to: close + len },
    ],
    range: EditorSelection.range(map(range.anchor), map(range.head)),
  };
}

/**
 * Toggle an inline format. Inside a span of that kind the span's markers come off (wherever in
 * it the caret is); otherwise the selection is wrapped — or, with nothing selected, an empty
 * pair is opened with the caret between.
 */
export function toggleInline(kind: InlineKind): EditorCommand {
  const marker = INLINE_MARKER[kind];
  return (view) => {
    view.dispatch(
      view.state.changeByRange((range) => {
        const line = view.state.doc.lineAt(range.from);
        const oneLine = range.to <= line.to;
        const span = oneLine
          ? spanAt(line.text, kind, range.from - line.from, range.to - line.from)
          : undefined;
        if (span) {
          return unwrapChange(
            line.from + span.from,
            line.from + span.to - span.markLen,
            span.markLen,
            range,
          );
        }
        const selected = view.state.doc.sliceString(range.from, range.to);
        const w = wrapInline(selected, marker);
        return {
          changes: { from: range.from, to: range.to, insert: w.text },
          range: EditorSelection.range(range.from + w.from, range.from + w.to),
        };
      }),
    );
    view.focus();
    return true;
  };
}

/** Run a line-block transform over the lines the selection touches. */
function transformLines(view: EditorView, fn: (lines: string[]) => string[]): boolean {
  const { state } = view;
  const main = state.selection.main;
  const first = state.doc.lineAt(main.from);
  const last = state.doc.lineAt(main.to);
  const lines: string[] = [];
  for (let n = first.number; n <= last.number; n += 1) lines.push(state.doc.line(n).text);
  const next = fn(lines).join('\n');
  view.dispatch({
    changes: { from: first.from, to: last.to, insert: next },
    selection: EditorSelection.range(first.from, first.from + next.length),
  });
  view.focus();
  return true;
}

export const toggleList = (kind: ListKind): EditorCommand => (view) =>
  transformLines(view, (lines) => toggleListKind(lines, kind));

export const toggleTask: EditorCommand = toggleList('task');
export const toggleBullet: EditorCommand = toggleList('bullet');
export const toggleNumbered: EditorCommand = toggleList('numbered');
export const quoteOrCallout: EditorCommand = (view) => transformLines(view, cycleQuote);

/** Set the heading level of the current line (0 clears it). */
export const setHeading = (level: number, toggle = false): EditorCommand => (view) =>
  transformLines(view, (lines) => setHeadingLines(lines, level, toggle));

/** Make `[selection](url)` with the url selected — or, inside a link, unwrap it to its label. */
export const toggleLink: EditorCommand = (view) => {
  view.dispatch(
    view.state.changeByRange((range) => {
      const line = view.state.doc.lineAt(range.from);
      const link =
        range.to <= line.to ? linkAt(line.text, range.from - line.from, range.to - line.from) : undefined;
      if (link) {
        const from = line.from + link.from;
        return {
          changes: { from, to: line.from + link.to, insert: link.label },
          range: EditorSelection.cursor(from + link.label.length),
        };
      }
      const label = view.state.doc.sliceString(range.from, range.to);
      const urlFrom = range.from + label.length + 3;
      return {
        changes: { from: range.from, to: range.to, insert: `[${label}](url)` },
        range: EditorSelection.range(urlFrom, urlFrom + 3),
      };
    }),
  );
  view.focus();
  return true;
};

const FENCE_LINE = /^\s*(`{3,}|~{3,})\s*$/;

/** Fence the selection as a code block — or, inside one, remove its fences and keep the code. */
export const toggleCodeBlock: EditorCommand = (view) => {
  const { state } = view;
  const block = fencedBlockAt(state, state.selection.main.head);
  if (block) {
    const open = state.doc.lineAt(block.from);
    const close = state.doc.lineAt(block.to);
    const changes: { from: number; to: number }[] = [
      { from: open.from, to: Math.min(open.to + 1, state.doc.length) },
    ];
    if (close.number !== open.number && FENCE_LINE.test(close.text)) {
      changes.push({ from: Math.max(close.from - 1, open.to + 1), to: close.to });
    }
    view.dispatch({ changes });
    view.focus();
    return true;
  }
  view.dispatch(
    state.changeByRange((range) => {
      const body = state.doc.sliceString(range.from, range.to);
      return {
        changes: { from: range.from, to: range.to, insert: `\`\`\`\n${body}\n\`\`\`` },
        range: EditorSelection.cursor(range.from + 4 + body.length),
      };
    }),
  );
  view.focus();
  return true;
};

/**
 * A `columns` × `rows` table skeleton on its own lines under the caret's line.
 *
 * A table glued to the line above it is not a table to the parser, hence the blank line when
 * that line has text. The caret lands in the first header cell, ready to be typed over.
 */
export function insertTableOfSize(columns: number, rows: number): EditorCommand {
  return (view) => {
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    const lead = line.text.trim() ? '\n\n' : '';
    const insert = `${lead}${tableSkeleton(columns, rows)}\n`;
    view.dispatch({
      changes: { from: line.to, insert },
      selection: EditorSelection.cursor(line.to + lead.length + 2),
    });
    view.focus();
    return true;
  };
}

/** The default table (2 × 1) — what the `Ctrl+Shift+T` chord writes without asking a size. */
export const insertTable: EditorCommand = insertTableOfSize(2, 1);

// ─── The action table ────────────────────────────────────────────────────────

export interface MarkdownAction {
  id: string;
  /** What it does, in a few words — the tooltip and the palette title. */
  label: string;
  /** CodeMirror key spec, when the action has a chord. */
  key?: string;
  /** The chord as a person reads it. */
  shortcut?: string;
  command: EditorCommand;
  /** Is the format "on" under this selection? Absent for an action that is not a toggle. */
  active?: (s: FormatState) => boolean;
}

export const MARKDOWN_ACTIONS: MarkdownAction[] = [
  { id: 'bold', label: 'Bold', key: 'Alt-b', shortcut: 'Alt+B', command: toggleInline('bold'), active: (s) => s.bold },
  { id: 'italic', label: 'Italic', key: 'Alt-i', shortcut: 'Alt+I', command: toggleInline('italic'), active: (s) => s.italic },
  { id: 'strike', label: 'Strikethrough', key: 'Alt-s', shortcut: 'Alt+S', command: toggleInline('strike'), active: (s) => s.strike },
  { id: 'highlight', label: 'Highlight', key: 'Alt-h', shortcut: 'Alt+H', command: toggleInline('highlight'), active: (s) => s.highlight },
  { id: 'code', label: 'Inline code', key: 'Alt-c', shortcut: 'Alt+C', command: toggleInline('code'), active: (s) => s.code },
  { id: 'h1', label: 'Heading 1', command: setHeading(1, true), active: (s) => s.heading === 1 },
  { id: 'h2', label: 'Heading 2', command: setHeading(2, true), active: (s) => s.heading === 2 },
  { id: 'h3', label: 'Heading 3', command: setHeading(3, true), active: (s) => s.heading === 3 },
  { id: 'quote', label: 'Quote — press again for a callout', key: 'Alt-q', shortcut: 'Alt+Q', command: quoteOrCallout, active: (s) => s.quote },
  { id: 'bullet', label: 'Bulleted list', command: toggleBullet, active: (s) => s.list === 'bullet' },
  { id: 'numbered', label: 'Numbered list', command: toggleNumbered, active: (s) => s.list === 'numbered' },
  { id: 'task', label: 'Task', key: 'Alt-t', shortcut: 'Alt+T', command: toggleTask, active: (s) => s.list === 'task' },
  { id: 'link', label: 'Link', key: 'Alt-l', shortcut: 'Alt+L', command: toggleLink, active: (s) => s.link },
  { id: 'codeblock', label: 'Code block', command: toggleCodeBlock, active: (s) => s.codeBlock },
  { id: 'table', label: 'Table', key: 'Mod-Shift-t', shortcut: 'Ctrl+Shift+T', command: insertTable },
];

/** An action by id. */
export function markdownAction(id: string): MarkdownAction {
  const action = MARKDOWN_ACTIONS.find((a) => a.id === id);
  if (!action) throw new Error(`unknown markdown action: ${id}`);
  return action;
}

/** The `Alt` family decided in `docs/garrulus-design.md` §9, plus the table chord. */
export const markdownFormatKeymap: Extension = Prec.high(
  keymap.of(
    MARKDOWN_ACTIONS.flatMap((a) => (a.key ? [{ key: a.key, run: a.command }] : [])),
  ),
);
