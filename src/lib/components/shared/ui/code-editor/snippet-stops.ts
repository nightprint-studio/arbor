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
 * ## Mirroring
 *
 * Two stops written with the same number — `${1:name}` twice — are one thing typed in two places:
 * type in either and the other follows, and Tab visits the pair once. The number rides along from
 * the parser as each stop's `group`, so nothing here has to guess which ranges belong together.
 *
 * The echo is a **transaction filter** rather than an update listener: an edit and the mirrors it
 * implies have to be one transaction, or undo takes them apart and leaves a name half-renamed. A
 * stop with no group (`0`) stands alone, which is every stop a plain completion has.
 */

import { Decoration, EditorView, keymap, type DecorationSet } from '@codemirror/view';
import {
  Annotation, EditorState, Prec, StateEffect, StateField,
  type ChangeDesc, type Extension, type TransactionSpec,
} from '@codemirror/state';

/** One stop, in document positions (UTF-16), once it has been placed in the buffer. */
interface Stop {
  from: number;
  to: number;
  /** The placeholder number it was written as; `0` for a stop that stands alone. Stops sharing a
   *  group are one value in several places. */
  group: number;
}

/** Marks the transaction this module appends, so echoing an echo is impossible. */
const mirroring = Annotation.define<boolean>();

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
    stops.push({ from, to, group: stop.group });
  }
  if (stops.length === 0) return null;
  return { stops, active: Math.min(activeIndex, stops.length - 1) };
}

/** A pending stop, so it is visible where Tab will go next. */
const pendingMark = Decoration.mark({ class: 'cm-snip-stop' });
/** The one the selection is on. */
const activeMark = Decoration.mark({ class: 'cm-snip-stop cm-snip-stop-active' });
/** Somewhere else the thing being typed also appears — drawn apart, or the text updating over there
 *  looks like the editor doing something of its own. */
const mirrorMark = Decoration.mark({ class: 'cm-snip-stop cm-snip-stop-mirror' });

function markFor(active: ActiveStops, index: number): Decoration {
  if (index === active.active) return activeMark;
  const group = active.stops[active.active]?.group ?? 0;
  return group !== 0 && active.stops[index].group === group ? mirrorMark : pendingMark;
}

function decorationsFor(active: ActiveStops | null, docLength: number): DecorationSet {
  if (!active) return Decoration.none;
  const ranges = active.stops
    // A zero-width stop is a caret position; there is nothing to underline.
    .map((s, i) => ({ s, i }))
    .filter(({ s }) => s.to > s.from && s.from >= 0 && s.to <= docLength)
    .map(({ s, i }) => markFor(active, i).range(s.from, s.to));
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

/**
 * The next stop in `dir`, skipping the mirrors of the one we are on: they hold the same value, and
 * tabbing through the second copy of a name you have just typed is a press that does nothing.
 */
function nextStop(active: ActiveStops, dir: 1 | -1): number {
  const group = active.stops[active.active]?.group ?? 0;
  let i = active.active + dir;
  while (group !== 0 && i >= 0 && i < active.stops.length && active.stops[i].group === group) {
    i += dir;
  }
  return i;
}

/** Move to the stop `dir` away. `false` when there is none, so the key falls through. */
function move(dir: 1 | -1) {
  return (view: EditorView): boolean => {
    const active = view.state.field(stopsField, false);
    if (!active) return false;
    const next = nextStop(active, dir);
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

/**
 * Echo what was typed in the active stop into the other stops of its group.
 *
 * A filter and not an update listener, for one reason that shows up immediately: the edit and its
 * echo have to be **one transaction**. Dispatched separately, undo takes them apart and the first
 * press leaves the name changed in one place and not the other — which is the state the feature
 * exists to prevent.
 *
 * `sequential` because the appended changes are written in the coordinates of the document *after*
 * the user's edit, which is the only frame in which the mirrors' positions are known.
 */
const mirror = EditorState.transactionFilter.of((tr): TransactionSpec | readonly TransactionSpec[] => {
  if (!tr.docChanged || tr.annotation(mirroring)) return tr;
  const active = tr.startState.field(stopsField, false);
  const current = active?.stops[active.active];
  if (!active || !current || current.group === 0) return tr;

  // Where the stop being typed in ended up, and what it now holds.
  const from = tr.changes.mapPos(current.from, -1);
  const to = tr.changes.mapPos(current.to, 1);
  if (to < from) return tr;
  const text = tr.state.doc.sliceString(from, to);

  const changes: { from: number; to: number; insert: string }[] = [];
  for (const [i, stop] of active.stops.entries()) {
    if (i === active.active || stop.group !== current.group) continue;
    const at = tr.changes.mapPos(stop.from, -1);
    const end = tr.changes.mapPos(stop.to, 1);
    // Deleted by this very edit, or already holding the text: nothing to say.
    if (end < at || tr.state.doc.sliceString(at, end) === text) continue;
    changes.push({ from: at, to: end, insert: text });
  }
  if (changes.length === 0) return tr;
  return [tr, { changes, sequential: true, annotations: mirroring.of(true), scrollIntoView: false }];
});

/** Abandon the run, leaving the text as it is. */
function clear(view: EditorView): boolean {
  if (!view.state.field(stopsField, false)) return false;
  view.dispatch({ effects: setStops.of(null) });
  return true;
}

/**
 * A multi-line body, re-indented to where it is being inserted.
 *
 * A snippet is written flush-left — it has to be, it does not know where it will land — so every
 * line after the first arrives at column 0. Inserted three levels deep into a class that reads as
 * a body that fell out of the code, and the user re-indents by hand what an abbreviation was
 * supposed to save them typing.
 *
 * `shift` maps an offset in the original text to the same place in the indented one, because the
 * tab stops are offsets into the body and every added indent moves everything after it.
 *
 * Exported for its test: it is pure, and it is the half of the insertion that is easy to get
 * subtly wrong.
 */
export function indentBody(text: string, indent: string): { text: string; shift: (i: number) => number } {
  if (!indent || !text.includes('\n')) return { text, shift: (i) => i };
  const breaks: number[] = [];
  for (let i = 0; i < text.length; i++) if (text[i] === '\n') breaks.push(i);
  return {
    text: text.split('\n').join(`\n${indent}`),
    shift: (i) => {
      let added = 0;
      for (const b of breaks) {
        if (b < i) added += indent.length;
        else break;
      }
      return i + added;
    },
  };
}

/**
 * Insert `text` at `[from, to)` and arm its tab stops.
 *
 * `stops` are byte-range pairs **into `text`** — the shape the backend sends. They are converted to
 * document positions here, which is the only place that knows where the text landed.
 *
 * A body with more than one line is re-indented to the line it lands on ({@link indentBody}) —
 * without it, `psvm` and every multi-line snippet a language server sends arrive with their bodies
 * at column 0.
 *
 * Returns `true` when stops were armed. `false` means it was a plain insertion (no stops, or none of
 * them survived being placed), and the caller need do nothing else.
 */
export function insertWithStops(
  view: EditorView,
  from: number,
  to: number,
  text: string,
  stops: readonly { start: number; end: number; group?: number }[],
  /** Byte offset → UTF-16 offset within `text`. */
  toU16: (byte: number) => number,
): boolean {
  // The indentation of the line the insertion starts on — what every line after the first is
  // missing. Taken from the line's own leading whitespace rather than from the editor's indent
  // unit: what matters is lining up with the code that is already there.
  const lineText = view.state.doc.lineAt(from).text;
  const indent = /^[ \t]*/.exec(lineText)?.[0] ?? '';
  const body = indentBody(text, indent);

  const placed: Stop[] = [];
  for (const stop of stops) {
    const start = from + body.shift(toU16(stop.start));
    const end = from + body.shift(toU16(stop.end));
    if (end >= start && start >= from && end <= from + body.text.length) {
      placed.push({ from: start, to: end, group: stop.group ?? 0 });
    }
  }

  const first = placed[0];
  view.dispatch({
    changes: { from, to, insert: body.text },
    // The caret goes to the first stop, or to the end of the insertion when there are none.
    selection: first
      ? { anchor: first.from, head: first.to }
      : { anchor: from + body.text.length },
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
    mirror,
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
      // The same value, over there. Fainter than the stop being typed in and not underlined
      // differently: it is not somewhere you are going, it is somewhere this text also is.
      '.cm-snip-stop-mirror': {
        backgroundColor: 'color-mix(in srgb, var(--accent-primary, #888) 7%, transparent)',
        borderBottomStyle: 'dashed',
      },
    }),
  ];
}
