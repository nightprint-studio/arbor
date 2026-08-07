/**
 * Code lenses — the small line of text a provider puts **above** an item: how many implementations
 * a trait has, how many places use a type.
 *
 * A third pushed layer beside the semantic tokens and the fold ranges (`server-layers.ts`), and here
 * rather than there because it is the one that is *interactive*: the others are decoration, a lens is
 * a control. That difference is the whole shape of this module — a lens has to be identified when it
 * is pressed, so each one carries an opaque `key` the host issued, and the host is the one that knows
 * what pressing it means.
 *
 * ## Block widgets, one row per line
 *
 * Several lenses commonly land on the same item ("2 implementations", "7 references"), and drawing
 * each as its own block would push the item down by a line per lens. They are grouped into one row
 * per line, separated, in the order they arrived.
 *
 * The row is indented to match the item it belongs to, measured from the line's own leading
 * whitespace in `ch` units — the editor's font is monospace, so a character IS a column. A lens
 * flush against the gutter above an indented method reads as belonging to the block, not the method.
 *
 * ## Why the decorations live in the field
 *
 * A block widget **may not** be handed to CodeMirror through a function of the view — it changes the
 * document's height, which is computed before the view exists, and doing so throws
 * ("Block decorations may not be specified via plugins"). So the field holds the built set and
 * rebuilds it whenever the entries or the line boundaries move, rather than deriving it on demand.
 *
 * ## Staleness
 *
 * Same as the other pushed layers: the answer is a beat behind the buffer, so positions are mapped
 * through every change rather than cleared, and a lens whose line was deleted goes with it. What is
 * deliberately *not* done is re-requesting on every keystroke — a count that is one edit out of date
 * is worth having, and a flickering one is not.
 */

import { Decoration, EditorView, WidgetType, type DecorationSet } from '@codemirror/view';
import { MapMode, StateEffect, StateField, type Extension, type Text } from '@codemirror/state';

/** One lens, in document (UTF-16) positions. */
export interface LensEntry {
  /** Document position of the item the lens belongs to. The row is drawn above that line. */
  pos: number;
  title: string;
  /** Whether pressing it does anything. A lens with no command is a label the provider wrote. */
  actionable: boolean;
  /**
   * How loudly to draw it. `muted` (the default) is for a lens that is **information you may
   * happen to want** — a reference count sits above every type in the file, and forty accented
   * rows would make the file unreadable. `accent` is for one that is **an offer you would act on**,
   * of which a file has a handful: it has to survive being glanced past, because a grey line above
   * a line of code reads as a comment.
   */
  tone?: 'muted' | 'accent';
  /** The host's own identifier, handed back to `onPress` untouched. */
  key: number;
}

/** Replace the lens layer. An empty array clears it. */
export const setCodeLenses = StateEffect.define<LensEntry[]>();

/** The row of lenses drawn above one line. */
class LensRowWidget extends WidgetType {
  constructor(
    readonly entries: LensEntry[],
    readonly indent: number,
    readonly onPress: (key: number) => void,
  ) {
    super();
  }

  /**
   * Two rows are the same when they *say* the same thing in the same place.
   *
   * Load-bearing rather than an optimisation: the set is rebuilt on every document change, and
   * without this every rebuild would replace the DOM of every row — which makes them flash while
   * typing and drops a press that lands mid-rebuild.
   */
  eq(other: LensRowWidget): boolean {
    return (
      this.indent === other.indent &&
      this.entries.length === other.entries.length &&
      this.entries.every(
        (e, i) =>
          e.key === other.entries[i].key &&
          e.title === other.entries[i].title &&
          e.tone === other.entries[i].tone,
      )
    );
  }

  toDOM(): HTMLElement {
    const row = document.createElement('div');
    row.className = 'cm-lens-row';
    row.style.paddingLeft = `${this.indent}ch`;
    for (const [i, entry] of this.entries.entries()) {
      if (i > 0) {
        const sep = document.createElement('span');
        sep.className = 'cm-lens-sep';
        sep.textContent = '·';
        row.appendChild(sep);
      }
      const toned = entry.tone === 'accent' ? ' cm-lens-loud' : '';
      if (entry.actionable) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = `cm-lens cm-lens-action${toned}`;
        btn.textContent = entry.title;
        btn.onclick = (e) => {
          e.preventDefault();
          this.onPress(entry.key);
        };
        row.appendChild(btn);
      } else {
        const label = document.createElement('span');
        label.className = `cm-lens${toned}`;
        label.textContent = entry.title;
        row.appendChild(label);
      }
    }
    return row;
  }

  /** The height CodeMirror assumes before the row is measured, so the viewport does not jump. */
  get estimatedHeight(): number {
    return LENS_ROW_HEIGHT;
  }

  /** Clicks belong to the lens, not to the document — without this the editor would also move the
   *  caret to wherever the row happens to sit. */
  ignoreEvent(): boolean {
    return true;
  }
}

/** The row's height, in px. One line, and the same number in the theme and in the estimate. */
const LENS_ROW_HEIGHT = 17;

/** How many columns of leading whitespace `text` has, with a tab counted as one column.
 *
 *  One column per tab is wrong in general and right here: the indentation is measured to line the
 *  lens up with the *text* below it, and CodeMirror renders a tab at its own configured width, so
 *  either way this is an approximation. Being narrow is the safe direction — a lens slightly left of
 *  its item still reads as belonging to it, one pushed past it reads as belonging to nothing. */
function indentOf(text: string): number {
  let n = 0;
  while (n < text.length && (text[n] === ' ' || text[n] === '\t')) n += 1;
  return n;
}

function build(
  entries: readonly LensEntry[],
  doc: Text,
  onPress: (key: number) => void,
): DecorationSet {
  if (entries.length === 0) return Decoration.none;
  // Grouped by the line they sit above, keyed by the line's start — a block widget has to land on a
  // line boundary, or CodeMirror splits the line around it.
  const rows = new Map<number, { indent: number; entries: LensEntry[] }>();
  for (const entry of entries) {
    if (entry.pos < 0 || entry.pos > doc.length) continue;
    const line = doc.lineAt(entry.pos);
    const row = rows.get(line.from);
    if (row) row.entries.push(entry);
    else rows.set(line.from, { indent: indentOf(line.text), entries: [entry] });
  }
  return Decoration.set(
    [...rows.entries()].map(([from, row]) =>
      Decoration.widget({
        widget: new LensRowWidget(row.entries, row.indent, onPress),
        // `side: -1` with `block: true` — the row goes ABOVE the line, which is what makes it read
        // as an annotation of the item rather than as part of the code.
        side: -1,
        block: true,
      }).range(from),
    ),
    true,
  );
}

/**
 * The code-lens layer.
 *
 * Installed only by a host that can answer a press — the `onPress` callback is what makes a lens a
 * control rather than a decoration, and a host with nothing to do with a press should not be drawing
 * one. The state field is created here, per editor, because it closes over that callback.
 */
export function codeLensLayer(onPress: (key: number) => void): Extension {
  const field = StateField.define<{ entries: LensEntry[]; deco: DecorationSet }>({
    create() {
      return { entries: [], deco: Decoration.none };
    },
    update(value, tr) {
      for (const effect of tr.effects) {
        if (effect.is(setCodeLenses)) {
          return { entries: effect.value, deco: build(effect.value, tr.state.doc, onPress) };
        }
      }
      if (!tr.docChanged) return value;
      // Mapped rather than cleared, and a lens whose text the edit removed goes with it.
      // `assoc: 1` keeps a lens on the line it annotated when text is inserted at its start.
      const entries: LensEntry[] = [];
      for (const entry of value.entries) {
        const pos = tr.changes.mapPos(entry.pos, 1, MapMode.TrackDel);
        if (pos !== null) entries.push({ ...entry, pos });
      }
      // Rebuilt rather than mapped: the widgets are keyed by LINE start, and an edit moves lines
      // around independently of the positions inside them.
      return { entries, deco: build(entries, tr.state.doc, onPress) };
    },
    provide: (f) => EditorView.decorations.from(f, (v) => v.deco),
  });

  return [
    field,
    EditorView.baseTheme({
      '.cm-lens-row': {
        display: 'flex',
        alignItems: 'center',
        gap: '4px',
        fontSize: '85%',
        lineHeight: `${LENS_ROW_HEIGHT}px`,
        // Never taller than one line: a provider that writes a paragraph into a lens title must not
        // be able to reflow the file around it.
        height: `${LENS_ROW_HEIGHT}px`,
        overflow: 'hidden',
        whiteSpace: 'nowrap',
        color: 'var(--text-muted, #888)',
      },
      '.cm-lens': {
        font: 'inherit',
        color: 'inherit',
        background: 'none',
        border: 'none',
        padding: '0',
      },
      '.cm-lens-action': {
        cursor: 'pointer',
      },
      '.cm-lens-action:hover': {
        color: 'var(--accent-primary, #6cf)',
        textDecoration: 'underline',
      },
      // The loud tone. Coloured *and* a shade heavier, because colour alone at 85% of a small type
      // size is what a muted row already looks like on a light theme.
      '.cm-lens-loud': {
        color: 'var(--accent-primary, #6cf)',
        fontWeight: '600',
      },
      '.cm-lens-loud:hover': {
        textDecoration: 'underline',
      },
      '.cm-lens-sep': {
        opacity: '0.5',
      },
    }),
  ];
}
