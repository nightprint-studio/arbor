/**
 * Tab stops for an inserted completion — the `${1:value}` places you tab between.
 *
 * ## Why not CodeMirror's own `snippet()`
 *
 * It takes a *template string* in its own dialect and parses it, and two things make that the wrong
 * seam here:
 *
 * 1. **The stops already arrive as ranges.** The backend parses the provider's snippet grammar
 *    (`bennu-lsp`'s `snippet.rs`, where there is a test runner) and sends plain text plus
 *    `[start, end)` offsets. Re-encoding those into a template so a second parser can decode them is
 *    a round trip through a lossy format.
 * 2. **That format is lossy.** CodeMirror's field pattern is `\$\{(\d+)(?::([^{}]*))?\}` — the
 *    default content may not contain braces, so a **nested** stop (`${1:Some(${2:x})}`, which
 *    rust-analyzer does emit) cannot be expressed and mis-parses into something else.
 *
 * Ranges have neither problem: a nested stop is simply another range that happens to sit inside
 * one, and it is visited in its turn.
 *
 * ## What is deliberately not implemented
 *
 * **Mirroring.** In a real snippet engine two stops sharing an index update together as you type.
 * Here they are two stops you tab between. The provider tells us which stops shared a number, so the
 * information is not lost on the wire — it is unused, and adding it means tracking edits inside a
 * range and echoing them, which is a feature rather than a detail. Nothing silently half-works: two
 * mirrored stops behave as two ordinary ones.
 */

import { Decoration, EditorView, keymap, type DecorationSet } from '@codemirror/view';
import {
  Prec, StateEffect, StateField, type ChangeDesc, type Extension,
} from '@codemirror/state';

/** One stop, in document positions (UTF-16), once it has been placed in the buffer. */
interface Stop {
  from: number;
  to: number;
}

/** The stops of the insertion currently being tabbed through. */
interface ActiveStops {
  stops: Stop[];
  /** Index of the stop the selection is on. */
  active: number;
}

/** Arm the stops of a fresh insertion, or clear them with `null`. */
const setStops = StateEffect.define<ActiveStops | null>();

/**
 * Map the stops through a document change, dropping any the edit deleted.
 *
 * `TrackDel` semantics by hand: a stop whose text was replaced wholesale is gone, and keeping a
 * collapsed remnant would leave Tab visiting a position that means nothing.
 */
function mapStops(active: ActiveStops, changes: ChangeDesc): ActiveStops | null {
  const stops: Stop[] = [];
  let activeIndex = active.active;
  for (const [i, stop] of active.stops.entries()) {
    const from = changes.mapPos(stop.from, -1);
    const to = changes.mapPos(stop.to, 1);
    if (to < from) {
      // Deleted. If it was the one we were on, the next press should land on what follows it.
      if (i < activeIndex) activeIndex -= 1;
      continue;
    }
    stops.push({ from, to });
  }
  if (stops.length === 0) return null;
  return { stops, active: Math.min(activeIndex, stops.length - 1) };
}

/** A pending stop, so it is visible where Tab will go next. */
const pendingMark = Decoration.mark({ class: 'cm-snip-stop' });
/** The one the selection is on. */
const activeMark = Decoration.mark({ class: 'cm-snip-stop cm-snip-stop-active' });

function decorationsFor(active: ActiveStops | null, docLength: number): DecorationSet {
  if (!active) return Decoration.none;
  const ranges = active.stops
    // A zero-width stop is a caret position; there is nothing to underline.
    .filter((s) => s.to > s.from && s.from >= 0 && s.to <= docLength)
    .map((s, i) => (i === active.active ? activeMark : pendingMark).range(s.from, s.to));
  ranges.sort((a, b) => a.from - b.from || a.to - b.to);
  return Decoration.set(ranges, true);
}

const stopsField = StateField.define<ActiveStops | null>({
  create() {
    return null;
  },
  update(value, tr) {
    // An explicit arm/clear wins, and is read before mapping — the ranges in the effect are already
    // in the coordinates of the transaction that carries them.
    for (const effect of tr.effects) {
      if (effect.is(setStops)) return effect.value;
    }
    let next = value;
    if (next && tr.docChanged) next = mapStops(next, tr.changes);
    if (!next) return null;
    // The selection left the insertion entirely — clicking elsewhere, or moving past the last stop.
    // Dropping the stops there is what stops Tab from yanking the caret back into text the user has
    // finished with.
    if (tr.selection) {
      const head = tr.state.selection.main.head;
      const inside = next.stops.some((s) => head >= s.from && head <= s.to);
      if (!inside) return null;
    }
    return next;
  },
  provide: (field) =>
    EditorView.decorations.from(field, (value) => (view) =>
      decorationsFor(value, view.state.doc.length),
    ),
});

/** Move to the stop `dir` away. `false` when there is none, so the key falls through. */
function move(dir: 1 | -1) {
  return (view: EditorView): boolean => {
    const active = view.state.field(stopsField, false);
    if (!active) return false;
    const next = active.active + dir;
    if (next < 0 || next >= active.stops.length) {
      // Past the end: the run is over. Consumed rather than passed on, because inserting a tab
      // character at the last stop is never what the press meant.
      if (dir > 0) {
        view.dispatch({ effects: setStops.of(null) });
        return true;
      }
      return false;
    }
    const stop = active.stops[next];
    view.dispatch({
      selection: { anchor: stop.from, head: stop.to },
      effects: setStops.of({ stops: active.stops, active: next }),
      scrollIntoView: true,
    });
    return true;
  };
}

/** Abandon the run, leaving the text as it is. */
function clear(view: EditorView): boolean {
  if (!view.state.field(stopsField, false)) return false;
  view.dispatch({ effects: setStops.of(null) });
  return true;
}

/**
 * Insert `text` at `[from, to)` and arm its tab stops.
 *
 * `stops` are byte-range pairs **into `text`** — the shape the backend sends. They are converted to
 * document positions here, which is the only place that knows where the text landed.
 *
 * Returns `true` when stops were armed. `false` means it was a plain insertion (no stops, or none of
 * them survived being placed), and the caller need do nothing else.
 */
export function insertWithStops(
  view: EditorView,
  from: number,
  to: number,
  text: string,
  stops: readonly { start: number; end: number }[],
  /** Byte offset → UTF-16 offset within `text`. */
  toU16: (byte: number) => number,
): boolean {
  const placed: Stop[] = [];
  for (const stop of stops) {
    const start = from + toU16(stop.start);
    const end = from + toU16(stop.end);
    if (end >= start && start >= from && end <= from + text.length) {
      placed.push({ from: start, to: end });
    }
  }

  const first = placed[0];
  view.dispatch({
    changes: { from, to, insert: text },
    // The caret goes to the first stop, or to the end of the insertion when there are none.
    selection: first
      ? { anchor: first.from, head: first.to }
      : { anchor: from + text.length },
    // Only worth arming when there is somewhere to tab TO: a single stop is just a caret placement,
    // and leaving the state armed would have Tab swallow an indent for no reason.
    effects: placed.length > 1 ? setStops.of({ stops: placed, active: 0 }) : setStops.of(null),
    scrollIntoView: true,
  });
  return placed.length > 1;
}

/**
 * The tab-stop extension.
 *
 * Installed by the editor core next to the completion keymap and **after** it, so within that
 * precedence group `acceptCompletion` is tried first: while the popup is open Tab accepts, and only
 * once it is closed does Tab walk the stops. Every binding returns `false` when no run is active, so
 * Tab keeps its ordinary meaning the rest of the time.
 */
export function snippetStops(): Extension {
  return [
    stopsField,
    Prec.highest(
      keymap.of([
        { key: 'Tab', run: move(1) },
        { key: 'Shift-Tab', run: move(-1) },
        { key: 'Escape', run: clear },
      ]),
    ),
    EditorView.baseTheme({
      // Dotted rather than filled: the text is real and already correct, and a solid highlight
      // reads as a selection the user has to deal with.
      '.cm-snip-stop': {
        borderBottom: '1px dotted var(--accent-primary, #888)',
      },
      '.cm-snip-stop-active': {
        backgroundColor: 'color-mix(in srgb, var(--accent-primary, #888) 14%, transparent)',
      },
    }),
  ];
}
