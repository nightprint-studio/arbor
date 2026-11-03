/**
 * Colours for a **log file** opened in the editor: the line takes the hue of its level, and its
 * timestamp, thread and logger are drawn the way the console draws them.
 *
 * A stack trace under an error is part of the error, so its frames are painted with the line that
 * owns them — which means the first line on screen may belong to one above the viewport, and that line
 * is looked for (a bounded way up) before the visible ones are decorated.
 *
 * Only what is on screen is read: a log is megabytes, and nobody scrolls through all of it at once.
 */

import { RangeSetBuilder, type Extension } from '@codemirror/state';
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { continues, readLogLine, type FileLogLevel, type LogMarkKind } from './log-file-marks';

/** How far above the viewport the line that owns a trace is looked for. */
const LOOK_BACK = 300;

const LINE: Record<FileLogLevel, Decoration> = {
  trace: Decoration.line({ class: 'cm-lf-trace' }),
  debug: Decoration.line({ class: 'cm-lf-debug' }),
  info: Decoration.line({ class: 'cm-lf-info' }),
  warn: Decoration.line({ class: 'cm-lf-warn' }),
  error: Decoration.line({ class: 'cm-lf-error' }),
  fatal: Decoration.line({ class: 'cm-lf-fatal' }),
};
const MARK: Record<LogMarkKind, Decoration> = {
  ts: Decoration.mark({ class: 'cm-lf-ts' }),
  level: Decoration.mark({ class: 'cm-lf-levelword' }),
  thread: Decoration.mark({ class: 'cm-lf-thread' }),
  logger: Decoration.mark({ class: 'cm-lf-logger' }),
};

function decorate(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc;
  const first = view.visibleRanges.length ? doc.lineAt(view.visibleRanges[0].from).number : 1;

  // The level the first visible line inherits, if it is the tail of something above the viewport.
  let inherited: FileLogLevel | null = null;
  if (continues(doc.line(first).text)) {
    for (let n = first - 1; n >= 1 && n >= first - LOOK_BACK; n--) {
      const text = doc.line(n).text;
      const level = readLogLine(text).level;
      if (level) { inherited = level; break; }
      if (!continues(text)) break;
    }
  }

  let done = 0;
  for (const range of view.visibleRanges) {
    const last = doc.lineAt(range.to).number;
    for (let n = Math.max(doc.lineAt(range.from).number, done + 1); n <= last; n++) {
      const line = doc.line(n);
      const read = readLogLine(line.text);
      if (read.level) inherited = read.level;
      else if (!continues(line.text)) inherited = null;
      done = n;
      if (inherited) builder.add(line.from, line.from, LINE[inherited]);
      for (const m of read.marks) builder.add(line.from + m.from, line.from + m.to, MARK[m.kind]);
    }
  }
  return builder.finish();
}

const plugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view);
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) this.decorations = decorate(update.view);
    }
  },
  { decorations: (p) => p.decorations },
);

const theme = EditorView.baseTheme({
  '.cm-lf-fatal': { color: 'var(--error)', fontWeight: '600' },
  '.cm-lf-error': { color: 'var(--error)' },
  '.cm-lf-warn': { color: 'var(--warning)' },
  '.cm-lf-debug, .cm-lf-trace': { color: 'var(--text-muted)' },
  // Chrome of the line — the parts that are the same on every one, so they recede behind the message.
  '.cm-lf-ts': { color: 'var(--text-disabled)' },
  '.cm-lf-thread': { color: 'var(--text-muted)' },
  '.cm-lf-logger': { color: 'var(--accent)' },
  '.cm-lf-levelword': { fontWeight: '700' },
  '.cm-lf-info .cm-lf-levelword': { color: 'var(--info)' },
});

/** The extension the `.log` language descriptor installs. */
export const logFileHighlight: Extension = [plugin, theme];
