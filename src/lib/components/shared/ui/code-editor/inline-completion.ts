/**
 * Ghost text — the greyed-out continuation shown at the caret, accepted with Tab.
 *
 * Nothing in Arbor had this, Bennu included, so it is built here in the shared
 * core rather than inside one product: the moment it exists, every editor wants
 * it.
 *
 * ## It is not an AI completion
 *
 * In most editors "ghost text" means a language model guessing. Here it must not
 * be, and in Picus it is forbidden outright — but the constraint turns out to be
 * a feature rather than a limitation. After `INSERT INTO PARAMETRI (` the column
 * list is **not a guess**: it is a fact the schema already told us. A proposal
 * that is derived from what the tool knows is worth more than one that is
 * predicted, because it is either right or absent, never plausibly wrong.
 *
 * So the seam is a plain function: given a document position, return the text
 * that certainly follows, or `null`. What makes it correct is the caller's
 * knowledge, and this module is only responsible for showing it and accepting it.
 *
 * ## Continuing, and replacing
 *
 * The common proposal continues the text: return a string and Tab inserts it at
 * the caret. Some proposals are not continuations but **rewrites** of what is
 * already there — a source that recognises a shorthand and knows what it stands
 * for wants Tab to put the long form *in place of* the short one, not after it.
 * Returning an {@link InlineCompletion} says so: `replace` is the range Tab
 * overwrites and `insert` is what it writes, which frees the ghost text to be a
 * readable rendering of the outcome rather than a literal of it.
 *
 * Both halves are deliberately content-free. This module never learns what the
 * shorthand is or which language it belongs to; it learns only that accepting a
 * proposal may mean an edit wider than an insertion.
 *
 * ## How it behaves
 *
 * - Requested after a short pause once the caret settles, and only for a single
 *   empty selection — a proposal during a drag-select would be noise.
 * - **Never shown while the completion popup is open.** Both want Tab, and the
 *   popup is the more specific intent; ghost text stands down rather than
 *   competing for the key — and asks again as soon as the popup closes, so
 *   standing down never turns into staying down.
 * - Tab accepts, Esc dismisses. A dismissal sticks until the caret moves or the
 *   document changes, so Esc means "not here" rather than "not for a moment".
 * - Any edit or cursor move invalidates a pending request: a stale suggestion
 *   arriving late is worse than none, so replies are dropped unless the caret is
 *   still exactly where the question was asked.
 */

import {
  Decoration,
  EditorView,
  ViewPlugin,
  WidgetType,
  type Command,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';
import { Facet, StateEffect, StateField, type Extension } from '@codemirror/state';
import { completionStatus } from '@codemirror/autocomplete';

/**
 * A proposal that is more than a continuation.
 *
 * Every field beyond `text` is optional and defaults to the plain behaviour, so a
 * source that only ever continues text keeps returning a string.
 */
export interface InlineCompletion {
  /** The greyed text drawn after the caret. Shown, never necessarily written. */
  text: string;
  /**
   * What accepting writes. Defaults to `text`.
   *
   * Separate from `text` so a proposal can *read* as one thing and *do* another —
   * a rewrite is far clearer previewed as `→ <the result>` than as the result
   * alone butting up against the shorthand it is going to consume.
   */
  insert?: string;
  /** The range accepting overwrites. Defaults to an insertion at the caret. */
  replace?: { from: number; to: number };
}

/**
 * Produce what follows `pos` — or what should stand in place of the text around
 * it — and `null` when nothing certainly does.
 *
 * May be async — the caller owns any I/O. Returning `null` is the normal answer
 * and must stay cheap: this runs every time the caret settles.
 */
export type InlineCompletionSource = (
  view: EditorView,
  pos: number,
) => string | InlineCompletion | null | Promise<string | InlineCompletion | null>;

/** The suggestion currently on screen, normalised. */
interface Suggestion {
  /** What is drawn. */
  text: string;
  /** Document offset the ghost text is drawn at — always the caret it was asked for. */
  pos: number;
  /** The range accepting replaces (`from === to` for a plain insertion). */
  from: number;
  to: number;
  /** What accepting writes. */
  insert: string;
}

/** A source's answer, in the one shape the rest of the module handles. */
function normalise(answer: string | InlineCompletion, pos: number): Suggestion | null {
  const proposal = typeof answer === 'string' ? { text: answer } : answer;
  if (!proposal.text) return null;
  const { from, to } = proposal.replace ?? { from: pos, to: pos };
  return {
    text: proposal.text,
    pos,
    from,
    to,
    insert: proposal.insert ?? proposal.text,
  };
}

const setSuggestion = StateEffect.define<Suggestion | null>();

/** The rendered ghost text. `side: 1` keeps it after the caret. */
class GhostWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }

  eq(other: GhostWidget): boolean {
    return other.text === this.text;
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-ghost-text';
    // `textContent` rather than markup: the text comes from a product's source
    // and lands in the DOM, so it must never be interpreted as HTML. Line breaks
    // still render because the class sets `white-space: pre`.
    span.textContent = this.text;
    // Hidden from assistive tech: it is a proposal, not document content, and a
    // screen reader announcing it as text would misrepresent the buffer.
    span.setAttribute('aria-hidden', 'true');
    return span;
  }

  /** Clicks fall through to the editor — the widget is decoration, not a target. */
  ignoreEvent(): boolean {
    return false;
  }
}

const suggestionField = StateField.define<Suggestion | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setSuggestion)) return effect.value;
    }
    // Any edit or cursor move makes the proposal stale. Cleared here rather than
    // in the plugin so it is impossible for a decoration to outlive the position
    // it was computed for.
    if (tr.docChanged || tr.selection) return null;
    return value;
  },
  provide: (field) =>
    EditorView.decorations.from(field, (value): DecorationSet => {
      if (!value) return Decoration.none;
      return Decoration.set([
        Decoration.widget({ widget: new GhostWidget(value.text), side: 1 }).range(value.pos),
      ]);
    }),
});

/** Apply the visible suggestion. `false` when there is nothing to accept, so Tab
 *  falls through to indentation as usual.
 *
 *  One dispatch whether it inserts or replaces — a replacement whose range is empty
 *  *is* an insertion, so there is no second path to keep in step with this one. */
export const acceptInlineCompletion: Command = (view) => {
  const current = view.state.field(suggestionField, false);
  if (!current) return false;
  view.dispatch({
    changes: { from: current.from, to: current.to, insert: current.insert },
    selection: { anchor: current.from + current.insert.length },
    effects: setSuggestion.of(null),
    userEvent: 'input.complete',
  });
  return true;
};

/** Dismiss the visible suggestion, and remember not to offer it again until
 *  something changes. `false` when there is nothing shown, so Esc keeps its other
 *  meanings (closing a panel, clearing a selection). */
export const dismissInlineCompletion: Command = (view) => {
  const current = view.state.field(suggestionField, false);
  if (!current) return false;
  view.dispatch({ effects: setSuggestion.of(null) });
  view.plugin(inlineCompletionPlugin)?.dismiss(current.pos);
  return true;
};

/** Is a suggestion on screen? Lets a host show a hint in the status bar. */
export function inlineCompletionActive(view: EditorView): boolean {
  return view.state.field(suggestionField, false) != null;
}

/** Per-editor configuration.
 *
 * A facet rather than module state: two editors can be alive at once — Bennu and
 * Picus in a tabbed window, or two query tabs — and each needs its own source.
 * Module-level configuration would give whichever mounted last to all of them. */
const inlineCompletionConfig = Facet.define<
  { source: InlineCompletionSource; delay: number },
  { source: InlineCompletionSource | null; delay: number }
>({
  combine: (values) => values[0] ?? { source: null, delay: 120 },
});

const inlineCompletionPlugin = ViewPlugin.fromClass(
  class {
    private timer: number | null = null;
    /** Monotonic, so a slow reply overtaken by a fast one cannot resurrect itself. */
    private seq = 0;
    /** Caret offset the user pressed Esc at; nothing is offered there again. */
    private dismissedAt = -1;

    /**
     * Whether the completion popup was up last time we looked.
     *
     * Tracked because standing down for the popup (see `request`) used to be
     * **permanent**: the request was skipped and only a further edit or caret move
     * would ever schedule another. So a proposal was silently lost whenever the
     * popup happened to be open — or merely still `pending` — at the instant the
     * caret settled, and the way back was to press a key and have the second
     * attempt find a warm cache. That reads as "it does not work, then for no
     * reason it does".
     *
     * Dismissing the popup with Escape had the same shape and no way out at all:
     * nothing about it changes the document or the selection, so no proposal was
     * ever asked for again.
     */
    private popupWasOpen = false;

    constructor(private readonly view: EditorView) {}

    update(update: ViewUpdate) {
      // A transaction that only opened or closed the popup changes neither, and it
      // is exactly the moment the answer to "may I show something?" flips.
      const popupOpen = completionStatus(update.state) !== null;
      const popupChanged = popupOpen !== this.popupWasOpen;
      this.popupWasOpen = popupOpen;

      if (!update.docChanged && !update.selectionSet && !popupChanged) return;
      // Typing means the situation changed, so a previous dismissal no longer
      // applies. Moving the caret elsewhere is handled by the position check.
      if (update.docChanged) this.dismissedAt = -1;
      this.schedule();
    }

    dismiss(pos: number) {
      this.dismissedAt = pos;
      this.cancel();
    }

    destroy() {
      this.cancel();
    }

    private cancel() {
      if (this.timer !== null) {
        window.clearTimeout(this.timer);
        this.timer = null;
      }
      // Invalidate any request already in flight.
      this.seq += 1;
    }

    private schedule() {
      this.cancel();
      const { delay } = this.view.state.facet(inlineCompletionConfig);
      this.timer = window.setTimeout(() => void this.request(), delay);
    }

    private async request() {
      this.timer = null;
      const state = this.view.state;
      const { source } = state.facet(inlineCompletionConfig);
      if (!source) return;

      const cursor = state.selection.main;
      // One caret, nothing selected: a proposal during a selection or with
      // multiple cursors would be ambiguous about where it lands.
      if (!cursor.empty || state.selection.ranges.length !== 1) return;
      // The completion popup owns Tab while it is open. Standing down is
      // deliberate — two things competing for one key is worse than one of them
      // being unavailable for a moment — and it is only ever *for a moment*: the
      // plugin watches the popup's status and asks again the instant it closes.
      // `pending` counts as open, because a source that is still thinking is about
      // to own the key.
      if (completionStatus(state) !== null) return;
      if (cursor.head === this.dismissedAt) return;

      const seq = ++this.seq;
      const pos = cursor.head;
      let answer: string | InlineCompletion | null = null;
      try {
        answer = await source(this.view, pos);
      } catch {
        // A source that throws must not take the editor with it; no proposal is
        // always a valid answer.
        return;
      }
      // Superseded, or the caret moved while we were waiting.
      if (seq !== this.seq) return;
      const now = this.view.state.selection.main;
      if (!now.empty || now.head !== pos) return;
      if (!answer) return;

      const suggestion = normalise(answer, pos);
      if (!suggestion) return;
      this.view.dispatch({ effects: setSuggestion.of(suggestion) });
    }
  },
);

const ghostTheme = EditorView.baseTheme({
  '.cm-ghost-text': {
    color: 'var(--text-disabled, #5c5f66)',
    fontStyle: 'italic',
    // Renders the newlines in a multi-line proposal (a VALUES skeleton, a block
    // closer) without putting markup in the widget.
    whiteSpace: 'pre',
    // It is a proposal, not content: selecting it with the mouse would put text
    // on the clipboard that is not in the document.
    userSelect: 'none',
    pointerEvents: 'none',
  },
});

/**
 * Install ghost text driven by `source`.
 *
 * The keymap is **not** included: the caller installs it at the precedence its
 * editor needs, because Tab is contested (completion popup, Emmet, indentation)
 * and only the assembling code knows the right order. See `extensions.ts`.
 */
export function inlineCompletion(options: {
  source: InlineCompletionSource;
  /** Pause after the caret settles before asking. Default 120 ms. */
  delay?: number;
}): Extension {
  return [
    inlineCompletionConfig.of({ source: options.source, delay: options.delay ?? 120 }),
    suggestionField,
    inlineCompletionPlugin,
    ghostTheme,
  ];
}
