/**
 * Motion for folding: the lines an unfold uncovers come in instead of snapping into place.
 *
 * What can and cannot be animated. A fold replaces its text with one widget, so the folded lines are not in
 * the document layout at all — there is nothing to shrink when a fold closes, and animating layout heights
 * under CodeMirror would corrupt its height map. So the two directions are not mirror images:
 *
 *  - **unfold**: the lines that reappear carry `cm-fold-reveal` for a moment (a short fade and slide, all of
 *    it opacity and transform, which cost no layout);
 *  - **fold**: the placeholder pops in and the gutter chevron turns, both plain CSS on the new elements
 *    (see `theme.ts`).
 *
 * Every duration is the app's animation setting (`--anim-dur-*`), so turning animations off there turns all
 * of this off with it.
 */

import { unfoldEffect } from '@codemirror/language';
import { StateEffect, StateField, type Extension, type Range } from '@codemirror/state';
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';

/** More lines than this are not animated: a revealed screenful is the effect, ten thousand lines is a stall. */
const MAX_ANIMATED_LINES = 300;
/** How long the marker class stays — the longest animation plus a margin, then it is removed so a later
 *  scroll over the same lines does not replay it. */
const REVEAL_MS = 400;

const revealed = Decoration.line({ class: 'cm-fold-reveal' });
const clearReveal = StateEffect.define<null>();

const revealField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    value = value.map(tr.changes);
    for (const effect of tr.effects) {
      if (effect.is(clearReveal)) {
        value = Decoration.none;
      } else if (effect.is(unfoldEffect)) {
        const doc = tr.state.doc;
        const head = doc.lineAt(Math.min(effect.value.from, doc.length)).number;
        const last = doc.lineAt(Math.min(effect.value.to, doc.length)).number;
        // The head line was never hidden, so only what comes after it moves.
        const to = Math.min(last, head + MAX_ANIMATED_LINES);
        const marks: Range<Decoration>[] = [];
        for (let n = head + 1; n <= to; n++) marks.push(revealed.range(doc.line(n).from));
        value = value.update({ add: marks, sort: true });
      }
    }
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** Takes the marker class off once the animation has played. */
const revealCleanup = ViewPlugin.fromClass(
  class {
    private timer: ReturnType<typeof setTimeout> | undefined;

    constructor(private readonly view: EditorView) {}

    update(u: ViewUpdate) {
      if (!u.transactions.some((tr) => tr.effects.some((e) => e.is(unfoldEffect)))) return;
      if (this.timer !== undefined) clearTimeout(this.timer);
      this.timer = setTimeout(() => {
        this.timer = undefined;
        this.view.dispatch({ effects: clearReveal.of(null) });
      }, REVEAL_MS);
    }

    destroy() {
      if (this.timer !== undefined) clearTimeout(this.timer);
    }
  },
);

/** The unfold animation. Install next to `codeFolding()`. */
export function foldAnimation(): Extension {
  return [revealField, revealCleanup];
}
