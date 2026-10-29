/**
 * Line heat — a measurement drawn on the line it describes: a gutter bar whose intensity is the
 * value, and optionally a short label after the code ("38% CPU · 120 ms self").
 *
 * Generic on purpose: the host decides what a number means (profiler time, coverage, change
 * frequency) and hands over `heat` in 0..1; this only draws. Markers are anchored to their line
 * and mapped through edits, so hints stay put while you type above them; a line deleted takes its
 * marker with it. The host replaces the whole set when it has newer numbers.
 */
import { RangeSet, StateEffect, StateField } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';
import { Decoration, EditorView, gutter, GutterMarker, WidgetType } from '@codemirror/view';
import type { DecorationSet } from '@codemirror/view';
import { tooltip } from '$lib/actions/tooltip';

export interface LineHeat {
  /** 1-based line. */
  line: number;
  /** 0..1 — gutter bar intensity. */
  heat: number;
  /** Text after the code on this line. Omit for a bar-only line. */
  label?: string;
  tooltip?: string;
  /** The numbers describe an older version of the file: drawn muted. */
  stale?: boolean;
}

class HeatMarker extends GutterMarker {
  constructor(readonly h: LineHeat) {
    super();
  }

  eq(other: GutterMarker): boolean {
    return (
      other instanceof HeatMarker &&
      other.h.heat === this.h.heat &&
      other.h.stale === this.h.stale &&
      other.h.label === this.h.label &&
      other.h.tooltip === this.h.tooltip
    );
  }

  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-line-heat-bar';
    // Floor so the faintest line is still visible; the ramp is perceptual enough at this size.
    el.style.opacity = String(Math.max(0.18, Math.min(1, this.h.heat)) * (this.h.stale ? 0.5 : 1));
    if (this.h.tooltip) tooltip(el, this.h.tooltip);
    return el;
  }
}

class LabelWidget extends WidgetType {
  constructor(readonly h: LineHeat) {
    super();
  }

  eq(other: LabelWidget): boolean {
    return other.h.label === this.h.label && other.h.tooltip === this.h.tooltip && other.h.stale === this.h.stale && other.h.heat === this.h.heat;
  }

  toDOM() {
    const el = document.createElement('span');
    el.className = 'cm-line-heat-label' + (this.h.stale ? ' cm-line-heat-stale' : '') + (this.h.heat >= 0.5 ? ' cm-line-heat-hot' : '');
    el.textContent = this.h.label ?? '';
    if (this.h.tooltip) tooltip(el, this.h.tooltip);
    return el;
  }

  ignoreEvent() {
    return true;
  }
}

const setHeat = StateEffect.define<RangeSet<GutterMarker>>();

const heatField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(current, tr) {
    for (const effect of tr.effects) if (effect.is(setHeat)) return effect.value;
    return current.map(tr.changes);
  },
});

function labels(state: EditorState, set: RangeSet<GutterMarker>): DecorationSet {
  const decos = [];
  const iter = set.iter();
  while (iter.value) {
    const m = iter.value;
    if (m instanceof HeatMarker && m.h.label && iter.from <= state.doc.length) {
      const line = state.doc.lineAt(iter.from);
      decos.push(Decoration.widget({ widget: new LabelWidget(m.h), side: 1 }).range(line.to));
    }
    iter.next();
  }
  return Decoration.set(decos, true);
}

const labelField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(current, tr) {
    for (const effect of tr.effects) if (effect.is(setHeat)) return labels(tr.state, effect.value);
    return current.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

const theme = EditorView.baseTheme({
  '.cm-line-heat-gutter': { minWidth: '0' },
  '.cm-line-heat-gutter .cm-gutterElement': { padding: '0 1px 0 2px' },
  '.cm-line-heat-bar': {
    display: 'block',
    width: '3px',
    height: '100%',
    borderRadius: '1px',
    background: 'var(--warning, #e5a50a)',
  },
  '.cm-line-heat-label': {
    marginLeft: '2em',
    padding: '0 7px',
    borderRadius: '3px',
    fontSize: '0.86em',
    color: 'color-mix(in srgb, var(--text-secondary) 85%, transparent)',
    background: 'color-mix(in srgb, var(--warning, #e5a50a) 12%, transparent)',
    userSelect: 'none',
    whiteSpace: 'pre',
  },
  '.cm-line-heat-hot': {
    color: 'var(--text-primary)',
    background: 'color-mix(in srgb, var(--danger, #e5484d) 20%, transparent)',
  },
  '.cm-line-heat-stale': { opacity: '0.55', fontStyle: 'italic' },
});

/** Always installed: with nothing pushed the gutter column collapses and nothing is drawn. */
export const lineHeatExtension = [
  heatField,
  labelField,
  gutter({
    class: 'cm-line-heat-gutter',
    markers: (v) => v.state.field(heatField, false) ?? RangeSet.empty,
  }),
  theme,
];

/** Replace the editor's heat marks. */
export function pushLineHeat(view: EditorView, marks: readonly LineHeat[]) {
  const lines = view.state.doc.lines;
  const ranges = marks
    .filter((m) => m.line >= 1 && m.line <= lines)
    // One mark per line; the labelled one wins over a bar-only mark on the same line.
    .sort((a, b) => a.line - b.line || (b.label ? 1 : 0) - (a.label ? 1 : 0))
    .filter((m, i, all) => i === 0 || all[i - 1].line !== m.line)
    .map((m) => new HeatMarker(m).range(view.state.doc.line(m.line).from));
  view.dispatch({ effects: setHeat.of(RangeSet.of(ranges, true)) });
}
