/**
 * Per-line annotations — what `git blame` looks like once it stops being a table.
 *
 * Two surfaces, one data set:
 *
 * - a **slim coloured bar** in the left margin, one per annotated line, so the eye can see where
 *   one change ends and the next begins without any column of text costing width;
 * - the annotation's **inline text**, drawn after the code at the end of a line, the way GitLens
 *   does. The host marks which lines carry it (`inline` — the first line of each run) so a whole
 *   run reads once, not forty times; the line the caret is on also shows its own text
 *   (`detail`) when it is not one of those, so the line you are reading is never anonymous.
 *
 * The host owns what a line says (`inline`, `detail`, `tooltip`) and how it is tinted (`accent`); this only
 * draws. Markers are anchored to their line, so typing above them moves them with the text; a
 * line inserted after the set was computed simply has none until the host sends a fresh one.
 */
import { RangeSet, StateEffect, StateField } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';
import { Decoration, EditorView, gutter, GutterMarker, WidgetType } from '@codemirror/view';
import type { DecorationSet } from '@codemirror/view';
import { tooltip } from '$lib/actions/tooltip';

/** One annotated line. */
export interface LineAnnotation {
  /** 1-based line. */
  line: number;
  /** Shown after the code on this line, always. Set it on the first line of a run. */
  inline?: string;
  /** Shown after the code only while the caret is on this line and it has no `inline`. */
  detail?: string;
  /** Hover text for the bar and the inline text. */
  tooltip?: string;
  /** Any CSS colour: the bar's fill. */
  accent?: string;
}

class BarMarker extends GutterMarker {
  a: LineAnnotation;

  constructor(a: LineAnnotation) {
    super();
    this.a = a;
  }

  eq(other: GutterMarker): boolean {
    return (
      other instanceof BarMarker &&
      other.a.tooltip === this.a.tooltip &&
      other.a.accent === this.a.accent &&
      other.a.inline === this.a.inline &&
      other.a.detail === this.a.detail
    );
  }

  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-line-annotation-bar';
    if (this.a.accent) el.style.background = this.a.accent;
    if (this.a.tooltip) tooltip(el, this.a.tooltip);
    return el;
  }
}

class InlineWidget extends WidgetType {
  text: string;
  tooltip: string;

  constructor(text: string, tooltip: string) {
    super();
    this.text = text;
    this.tooltip = tooltip;
  }

  eq(other: InlineWidget): boolean {
    return other.text === this.text && other.tooltip === this.tooltip;
  }

  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-line-annotation-inline';
    el.textContent = this.text;
    if (this.tooltip) tooltip(el, this.tooltip);
    return el;
  }

  // Never swallow a click meant for the code beside it.
  ignoreEvent() {
    return true;
  }
}

const setAnnotations = StateEffect.define<RangeSet<GutterMarker>>();

const annotationField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(current, tr) {
    for (const effect of tr.effects) if (effect.is(setAnnotations)) return effect.value;
    return current.map(tr.changes);
  },
});

/** One inline widget at the end of document line `from`. */
function inlineWidget(doc: EditorState['doc'], at: number, a: LineAnnotation, text: string) {
  const line = doc.lineAt(at);
  const widget = new InlineWidget(text, a.tooltip ?? '');
  return Decoration.widget({ widget, side: 1 }).range(line.to);
}

/** The run labels: an inline widget on every line the host gave `inline` text. Rebuilt only when
 *  a new annotation set arrives, and otherwise just carried along through edits — a label stays on
 *  its line while you type above it, instead of being recomputed per keystroke. */
const blockField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(current, tr) {
    for (const effect of tr.effects) {
      if (!effect.is(setAnnotations)) continue;
      const decos: ReturnType<typeof inlineWidget>[] = [];
      const iter = effect.value.iter();
      while (iter.value) {
        const marker = iter.value;
        if (marker instanceof BarMarker && marker.a.inline && iter.from <= tr.state.doc.length) {
          decos.push(inlineWidget(tr.state.doc, iter.from, marker.a, marker.a.inline));
        }
        iter.next();
      }
      return Decoration.set(decos, true);
    }
    return current.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** The caret line's own text, for a line that is not the start of a run. Declared after
 *  `annotationField`, so by the time it updates, that field already reflects the transaction. */
function caretDetail(state: EditorState): DecorationSet {
  const markers = state.field(annotationField, false);
  if (!markers) return Decoration.none;
  const line = state.doc.lineAt(state.selection.main.head);
  let found: LineAnnotation | undefined;
  markers.between(line.from, line.from, (from, _to, value) => {
    if (from === line.from && value instanceof BarMarker && !value.a.inline && value.a.detail) {
      found = value.a;
      return false;
    }
    return undefined;
  });
  if (!found) return Decoration.none;
  return Decoration.set([inlineWidget(state.doc, line.from, found, found.detail ?? '')]);
}

const caretField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (_current, tr) => caretDetail(tr.state),
  provide: (f) => EditorView.decorations.from(f),
});

const annotationTheme = EditorView.baseTheme({
  '.cm-line-annotation-gutter': { minWidth: '0' },
  '.cm-line-annotation-gutter .cm-gutterElement': { padding: '0 2px 0 3px' },
  '.cm-line-annotation-bar': {
    display: 'block',
    width: '3px',
    height: '100%',
    borderRadius: '1px',
    opacity: '0.85',
  },
  '.cm-line-annotation-inline': {
    marginLeft: '2.5em',
    padding: '0 7px',
    borderRadius: '3px',
    fontSize: '0.88em',
    // Readable on any theme: the secondary text colour at most of its strength, on a faint wash of
    // itself, instead of a tertiary grey that vanished against dark backgrounds.
    color: 'color-mix(in srgb, var(--text-secondary) 80%, transparent)',
    background: 'color-mix(in srgb, var(--text-secondary) 10%, transparent)',
    userSelect: 'none',
    whiteSpace: 'pre',
  },
});

/** The extension. Always installed: with no annotations the field is empty, the bar column has
 *  no markers and collapses, and no inline text is drawn — an editor that never annotates pays
 *  nothing. */
export const lineAnnotationExtension = [
  annotationField,
  blockField,
  caretField,
  gutter({
    class: 'cm-line-annotation-gutter',
    markers: (v) => v.state.field(annotationField, false) ?? RangeSet.empty,
  }),
  annotationTheme,
];

/** Replace the editor's annotations. */
export function pushLineAnnotations(view: EditorView, annotations: readonly LineAnnotation[]) {
  const lines = view.state.doc.lines;
  const ranges = annotations
    .filter((a) => a.line >= 1 && a.line <= lines)
    .filter((a, i, all) => all.findIndex((o) => o.line === a.line) === i)
    .sort((a, b) => a.line - b.line)
    .map((a) => new BarMarker(a).range(view.state.doc.line(a.line).from));
  view.dispatch({ effects: setAnnotations.of(RangeSet.of(ranges, true)) });
}
