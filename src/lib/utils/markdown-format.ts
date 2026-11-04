/**
 * Markdown formatting commands for the shared editor — bold, lists, links, tables.
 *
 * **Commands, not buttons.** Each is a `(view) => boolean` that edits the document, so the
 * keymap, the toolbar and a palette verb all run the same function and cannot disagree about
 * what "bold" does. Opt-in: the editor mounts them only when a host passes
 * {@link markdownFormatKeymap}, because Corvus's modal and Bennu's `.md` tabs never asked for
 * the `Alt` family.
 *
 * The text transforms are pure functions over strings/lines ({@link wrapInline},
 * {@link toggleLinePrefixes}, {@link cycleQuote}) and are what the unit tests cover; the
 * commands only translate a CodeMirror selection into them.
 */

import { EditorSelection, Prec, type Extension } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';

// ─── Pure transforms ─────────────────────────────────────────────────────────

export interface Wrapped {
  text: string;
  /** Where the selection starts and ends inside `text`. */
  from: number;
  to: number;
}

/**
 * Toggle `marker` around `selected`.
 *
 * `before`/`after` are the characters already surrounding the selection in the document, so a
 * selection that sits *inside* `**…**` unwraps instead of nesting a second pair. An empty
 * selection yields an empty pair with the caret between the markers.
 */
export function wrapInline(selected: string, marker: string, before = '', after = ''): Wrapped & {
  /** How many characters of surrounding context the edit consumed on each side. */
  eatBefore: number;
  eatAfter: number;
} {
  const m = marker.length;
  // `**bold**` selected as a whole.
  if (selected.length >= 2 * m && selected.startsWith(marker) && selected.endsWith(marker)) {
    const inner = selected.slice(m, selected.length - m);
    return { text: inner, from: 0, to: inner.length, eatBefore: 0, eatAfter: 0 };
  }
  // The markers sit just outside the selection.
  if (before.endsWith(marker) && after.startsWith(marker)) {
    return { text: selected, from: 0, to: selected.length, eatBefore: m, eatAfter: m };
  }
  const text = `${marker}${selected}${marker}`;
  return { text, from: m, to: m + selected.length, eatBefore: 0, eatAfter: 0 };
}

/**
 * Toggle a line prefix (`- [ ] `, `1. `, `- `) over a block of lines.
 *
 * If *every* non-empty line already has it, it is removed from all; otherwise it is added to
 * the ones that lack it. Blank lines are left alone — a prefix on an empty line is a stray
 * bullet.
 */
export function toggleLinePrefixes(lines: string[], prefix: string): string[] {
  const body = lines.filter((l) => l.trim().length > 0);
  const allHave = body.length > 0 && body.every((l) => l.startsWith(prefix));
  return lines.map((l) => {
    if (!l.trim()) return l;
    if (allHave) return l.slice(prefix.length);
    return l.startsWith(prefix) ? l : `${prefix}${l}`;
  });
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

// ─── CodeMirror commands ─────────────────────────────────────────────────────

export type EditorCommand = (view: EditorView) => boolean;

/** Bold, italic, code… around the selection(s). */
export function toggleInline(marker: string): EditorCommand {
  return (view) => {
    view.dispatch(
      view.state.changeByRange((range) => {
        const doc = view.state.doc;
        const m = marker.length;
        const selected = doc.sliceString(range.from, range.to);
        const before = doc.sliceString(Math.max(0, range.from - m), range.from);
        const after = doc.sliceString(range.to, Math.min(doc.length, range.to + m));
        const w = wrapInline(selected, marker, before, after);
        const from = range.from - w.eatBefore;
        const to = range.to + w.eatAfter;
        return {
          changes: { from, to, insert: w.text },
          range: EditorSelection.range(from + w.from, from + w.to),
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
  const next = fn(lines);
  view.dispatch({
    changes: { from: first.from, to: last.to, insert: next.join('\n') },
    selection: EditorSelection.range(first.from, first.from + next.join('\n').length),
  });
  view.focus();
  return true;
}

export const toggleLinePrefix = (prefix: string): EditorCommand => (view) =>
  transformLines(view, (lines) => toggleLinePrefixes(lines, prefix));

export const toggleTask: EditorCommand = toggleLinePrefix('- [ ] ');
export const toggleBullet: EditorCommand = toggleLinePrefix('- ');
export const toggleNumbered: EditorCommand = toggleLinePrefix('1. ');
export const quoteOrCallout: EditorCommand = (view) => transformLines(view, cycleQuote);

/** Set the heading level of the current line; pressing the same level again clears it. */
export const setHeading = (level: number): EditorCommand => (view) =>
  transformLines(view, ([line, ...rest]) => {
    const bare = line.replace(/^#{1,6}\s+/, '');
    const had = /^(#{1,6})\s/.exec(line)?.[1].length ?? 0;
    return [had === level ? bare : `${'#'.repeat(level)} ${bare}`, ...rest];
  });

/** `[selection](url)` with the url selected, ready to paste over. */
export const insertLink: EditorCommand = (view) => {
  view.dispatch(
    view.state.changeByRange((range) => {
      const label = view.state.doc.sliceString(range.from, range.to);
      const insert = `[${label}](url)`;
      const urlFrom = range.from + label.length + 3;
      return {
        changes: { from: range.from, to: range.to, insert },
        range: EditorSelection.range(urlFrom, urlFrom + 3),
      };
    }),
  );
  view.focus();
  return true;
};

/** A fenced code block around the selection (or an empty one with the caret inside). */
export const insertCodeBlock: EditorCommand = (view) => {
  view.dispatch(
    view.state.changeByRange((range) => {
      const body = view.state.doc.sliceString(range.from, range.to);
      const insert = `\`\`\`\n${body}\n\`\`\``;
      return {
        changes: { from: range.from, to: range.to, insert },
        range: EditorSelection.cursor(range.from + 4 + body.length),
      };
    }),
  );
  view.focus();
  return true;
};

/** A table skeleton on its own lines at the caret. */
export const insertTable: EditorCommand = (view) => {
  const head = view.state.selection.main.head;
  const line = view.state.doc.lineAt(head);
  const lead = line.text.trim() ? '\n\n' : '';
  const insert = `${lead}${tableSkeleton()}\n`;
  view.dispatch({
    changes: { from: line.to, insert },
    selection: EditorSelection.cursor(line.to + lead.length + 2),
  });
  view.focus();
  return true;
};

/** The `Alt` family decided in `docs/garrulus-design.md` §9, plus the table chord. */
export const markdownFormatKeymap: Extension = Prec.high(
  keymap.of([
    { key: 'Alt-b', run: toggleInline('**') },
    { key: 'Alt-i', run: toggleInline('*') },
    { key: 'Alt-c', run: toggleInline('`') },
    { key: 'Alt-s', run: toggleInline('~~') },
    { key: 'Alt-h', run: toggleInline('==') },
    { key: 'Alt-l', run: insertLink },
    { key: 'Alt-q', run: quoteOrCallout },
    { key: 'Alt-t', run: toggleTask },
    { key: 'Mod-Shift-t', run: insertTable },
  ]),
);
