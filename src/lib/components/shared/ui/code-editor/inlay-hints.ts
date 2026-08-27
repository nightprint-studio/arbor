/**
 * Inlay hints — text a provider draws *between* the code rather than in it.
 *
 * `repo.transfer(from, to, 500)` tells you nothing about which argument is which; with hints it
 * reads `repo.transfer(source: from, target: to, amount: 500)`, and the three words were never in
 * the file. Same for a type the language inferred and nobody wrote: `var order = load()` with a
 * `: Order` after the name.
 *
 * ## Widget decorations, not text
 *
 * The hints are zero-width decorations, so every offset in the document is exactly what it was:
 * the caret cannot land inside one, selecting a line and copying it yields the code, and a
 * diagnostic's span still points where it pointed. That is the whole reason to do it this way
 * rather than by inserting text — a hint that shifted offsets would put every other feature in the
 * editor one hint out of step.
 *
 * ## Positions are document offsets
 *
 * The provider (a language server, or Bennu's own resolver) reports in UTF-8 bytes; converting is
 * the caller's job, the same as for diagnostics. What arrives here is already CodeMirror
 * coordinates.
 */

import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet } from '@codemirror/view';
import { StateEffect, StateField, type Extension } from '@codemirror/state';

/** One hint: some text, drawn at a document position. */
export interface InlayHint {
  /** Document offset (UTF-16) the hint is drawn at. */
  pos: number;
  /** The text to draw — `source:` for a parameter name, `: Order` for a type. */
  label: string;
  /**
   * Which side of the position the hint sits on, and therefore how it reads.
   *
   * `'before'` draws it in front of what is at `pos` (a parameter name in front of its argument);
   * `'after'` draws it behind (a type after the name it belongs to). It also settles the ordering
   * when a hint of each kind lands on the same offset.
   */
  side?: 'before' | 'after';
}

/** Replace the whole hint set. */
export const setInlayHints = StateEffect.define<readonly InlayHint[]>();

class HintWidget extends WidgetType {
  constructor(
    readonly label: string,
    readonly side: 'before' | 'after',
  ) {
    super();
  }

  eq(other: HintWidget) {
    return other.label === this.label && other.side === this.side;
  }

  toDOM() {
    const span = document.createElement('span');
    span.className = `cm-inlay cm-inlay-${this.side}`;
    span.textContent = this.label;
    // Out of the accessibility tree and out of any text the user asks for: it is not in the file.
    span.setAttribute('aria-hidden', 'true');
    return span;
  }

  /** Never let a click put the caret "inside" a hint — there is nothing inside it. */
  ignoreEvent() {
    return true;
  }
}

function decorationsFor(hints: readonly InlayHint[], docLength: number): DecorationSet {
  const ranges = hints
    .filter((h) => h.pos >= 0 && h.pos <= docLength)
    .map((h) => {
      const side = h.side ?? 'before';
      return Decoration.widget({
        widget: new HintWidget(h.label, side),
        // A `before` hint belongs to what follows it, so it must sit on the left of anything else
        // at the same offset (and of the caret); an `after` hint on the right.
        side: side === 'before' ? -1 : 1,
      }).range(h.pos);
    });
  // `Decoration.set` requires sorted ranges, and a provider's order is its own business.
  return Decoration.set(ranges, true);
}

const hintField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(deco, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setInlayHints)) {
        return decorationsFor(effect.value, tr.state.doc.length);
      }
    }
    // Between refreshes the hints ride along with the edits, so they stay attached to what they
    // annotate while the next set is being fetched instead of scattering or vanishing.
    return deco.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

const hintTheme = EditorView.baseTheme({
  '.cm-inlay': {
    color: 'var(--text-faint, #8a8a8a)',
    backgroundColor: 'var(--bg-hover, rgba(127,127,127,0.12))',
    borderRadius: '3px',
    padding: '0 3px',
    margin: '0 1px',
    fontSize: '0.85em',
    // The hint must never be mistaken for code that is there: no selection, no caret, no copy.
    userSelect: 'none',
    pointerEvents: 'none',
    verticalAlign: 'baseline',
  },
});

/**
 * The inlay-hint extension. Push a set with {@link setInlayHints}; an empty array clears them.
 *
 * The plugin exists only to keep the field alive in the extension list — the field itself does all
 * the work, and having a single owner for the decorations is what makes "clear" a matter of pushing
 * an empty set rather than of tearing anything down.
 */
export function inlayHints(): Extension {
  return [
    hintField,
    hintTheme,
    ViewPlugin.define(() => ({})),
  ];
}
