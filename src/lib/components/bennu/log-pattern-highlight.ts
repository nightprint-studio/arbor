/**
 * Colours for the **conversion patterns** of a `log4j2.xml` / `logback.xml`.
 *
 * A logging layout is a small language written inside a string — `%d{HH:mm:ss.SSS} [%t] %-5level
 * %logger{36} - %msg%n` — and the XML mode paints all of it as one flat attribute value. The pattern
 * is the one thing in these files that is read character by character (which column is the logger,
 * what is the `{36}`, where did `%n` go), so it gets what a string with structure deserves: every
 * `%`-conversion takes the hue of what it prints, its `{options}` are quieter, and a `${property}` is
 * marked as the substitution it is.
 *
 * The hues follow the **console's own columns** (the date, the level, the logger, the thread), so a
 * pattern and the line it produces are read in the same colours.
 *
 * ## Where a pattern is
 *
 * Four places, all found by one pass over the text: a `pattern="…"` (Log4j2's `PatternLayout`), a
 * `filePattern="…"` (its rolling archives), a `<pattern>…</pattern>` or `<fileNamePattern>…</…>`
 * (Logback) and Log4j 1.x's `<param name="ConversionPattern" value="…"/>`. Like the pom's section
 * colours this is a scan and not a parse — the XML mode is a stream parser with no tree — and it is
 * bounded by {@link MAX_SCANNED}: a generated multi-megabyte file is coloured the ordinary way.
 *
 * Applied with `!important` for the reason the pom's are: the string token the mark sits inside
 * paints its own colour.
 */

import { RangeSetBuilder, type Extension } from '@codemirror/state';
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { NAMESPACE_COLORS } from '$lib/components/shared/ui/code-editor';
import { patternMarks, type PatternKind } from './log-pattern-marks';

const MARKS: Record<PatternKind, Decoration> = Object.fromEntries(
  (['date', 'level', 'logger', 'thread', 'msg', 'mdc', 'dim', 'other', 'opt', 'prop'] as PatternKind[]).map((kind) => [
    kind,
    Decoration.mark({ class: `cm-lp-${kind}` }),
  ]),
) as Record<PatternKind, Decoration>;

function decorate(text: string): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const mark of patternMarks(text)) builder.add(mark.from, mark.to, MARKS[mark.kind]);
  return builder.finish();
}

const patternPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = decorate(view.state.doc.toString());
    }
    update(update: ViewUpdate) {
      if (update.docChanged) this.decorations = decorate(update.state.doc.toString());
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

/** The hue of each kind, from the namespace palette the pom's sections draw on. */
const HUE: Record<PatternKind, string> = {
  date: NAMESPACE_COLORS[4],
  level: NAMESPACE_COLORS[3],
  logger: NAMESPACE_COLORS[0],
  thread: NAMESPACE_COLORS[1],
  msg: `color-mix(in srgb, ${NAMESPACE_COLORS[2]} 74%, var(--bg-base, #0d1117))`,
  mdc: NAMESPACE_COLORS[5],
  dim: 'var(--text-muted, #8b949e)',
  other: 'var(--text-secondary, #9da5b0)',
  // Quieter than the conversion it belongs to: the options say how, not what.
  opt: 'var(--text-secondary, #9da5b0)',
  prop: 'var(--warning, #d29922)',
};

const patternTheme = EditorView.baseTheme(
  Object.fromEntries(
    (Object.keys(HUE) as PatternKind[]).map((kind) => [`.cm-lp-${kind}, .cm-lp-${kind} *`, { color: `${HUE[kind]} !important` }]),
  ),
);

/** The extension a logging configuration's descriptor installs as its `extraHighlight`. */
export const logPatternHighlight: Extension = [patternPlugin, patternTheme];
