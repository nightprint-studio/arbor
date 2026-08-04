/**
 * Pasting into a string literal, the part every language shares.
 *
 * Text that arrives from the clipboard was written for a human, not for a literal:
 * it carries the quote that closes the literal, the backslash that changes the
 * meaning of what follows it, and — in a language whose literals cannot span lines
 * — the newline that ends the statement. Pasted verbatim it produces a syntax error
 * at best and a different value at worst, and an editor that leaves that to the user
 * has handed them a bug to find at run time.
 *
 * The rules are entirely the language's: what a literal is, what has to be escaped
 * inside one, whether a newline is legal there at all, and whether the text can be
 * held by a literal at all. So this file holds none of them. It owns the CodeMirror
 * wiring and the details that are easy to get subtly wrong wherever they are
 * repeated:
 *
 *   - the paste lands at the **start** of the selection, so that is the position
 *     whose context decides — not the caret's head, which for a backwards selection
 *     is the other end;
 *   - a paste that needs no rewriting must fall through to CodeMirror rather than be
 *     re-dispatched, so nothing about ordinary pasting changes;
 *   - the dispatch is labelled `input.paste`, so it joins the undo history as one
 *     gesture and `Ctrl+Z` takes the whole insertion back instead of unpicking it;
 *   - a refusal has to be **visible**. A paste that silently does nothing reads as a
 *     broken editor, so the language's reason is shown at the caret.
 *
 * A language supplies {@link LiteralPasteRenderer}; see `bennu/java-string-paste.ts`
 * (escapes, breaks a multi-line paste into concatenated literals, and refuses what a
 * Java constant cannot hold) and `picus/sql-intel/paste-escape.ts` (doubles the
 * quote) for the two shapes.
 */

import { EditorView, showTooltip, type Tooltip } from '@codemirror/view';
import { StateEffect, StateField, type Extension } from '@codemirror/state';

/** A language's answer for a paste it will not perform, and the reason to show. */
export interface LiteralPasteRefusal {
  /** Shown at the caret, verbatim. One sentence: what happened, and why. */
  refuse: string;
}

/**
 * The text to insert for a paste of `text` at `offset` in `doc`.
 *
 * Return `null` when the caret is not inside a literal — that is the ordinary case
 * and it must stay an ordinary paste. Returning `text` unchanged is equally fine and
 * means the same thing: nothing needed rewriting. Return a {@link LiteralPasteRefusal}
 * when no correct result exists, and the user is better off being told than handed
 * something that cannot work.
 *
 * `offset` is a UTF-16 document offset (CodeMirror's coordinate), and a returned
 * string is inserted verbatim, so a renderer that wants continuation lines indented
 * has to read that indentation out of `doc` itself.
 */
export type LiteralPasteRenderer = (
  doc: string,
  offset: number,
  text: string,
) => string | LiteralPasteRefusal | null;

// ── The refusal hint ───────────────────────────────────────────────────────────

const setHint = StateEffect.define<{ pos: number; message: string } | null>();

/** The hint currently shown, if any. It is about the gesture that just failed, so
 *  the next edit or cursor move retires it — no timer, and nothing to clean up. */
const hintField = StateField.define<Tooltip | null>({
  create: () => null,
  update(current, tr) {
    for (const effect of tr.effects) {
      if (!effect.is(setHint)) continue;
      const next = effect.value;
      if (!next) return null;
      return {
        pos: next.pos,
        above: true,
        create: () => {
          const dom = document.createElement('div');
          dom.className = 'cm-paste-hint';
          // textContent, not innerHTML: the message is composed by a language, but
          // the numbers in it come from the clipboard.
          dom.textContent = next.message;
          return { dom };
        },
      };
    }
    if (tr.docChanged || tr.selection) return null;
    return current;
  },
  provide: (f) => showTooltip.from(f),
});

// ── The extension ──────────────────────────────────────────────────────────────

/**
 * The CodeMirror extension around a {@link LiteralPasteRenderer}.
 *
 * The paste handler is registered through `domEventHandlers`, whose handlers
 * CodeMirror runs **before** its own — so returning `true` is what stops the default
 * paste from also inserting the untouched text.
 */
export function pasteIntoLiteral(render: LiteralPasteRenderer): Extension {
  return [
    hintField,
    EditorView.domEventHandlers({
      paste(event, view) {
        const text = event.clipboardData?.getData('text/plain') ?? '';
        if (!text) return false;

        // With several cursors CodeMirror spreads the pasted lines across them, one
        // each — a feature this cannot reproduce and must not quietly replace with a
        // single insertion at the primary caret.
        if (view.state.selection.ranges.length > 1) return false;

        const { from, to } = view.state.selection.main;
        let result: string | LiteralPasteRefusal | null;
        try {
          result = render(view.state.doc.toString(), from, text);
        } catch {
          // A renderer that throws must cost the user their paste, not their text:
          // fall through and let CodeMirror insert what the clipboard actually holds.
          return false;
        }
        if (result === null) return false;

        if (typeof result === 'object') {
          event.preventDefault();
          view.dispatch({ effects: setHint.of({ pos: from, message: result.refuse }) });
          return true;
        }

        if (result === text) return false;
        event.preventDefault();
        view.dispatch({
          changes: { from, to, insert: result },
          selection: { anchor: from + result.length },
          effects: setHint.of(null),
          userEvent: 'input.paste',
        });
        return true;
      },
    }),
  ];
}
