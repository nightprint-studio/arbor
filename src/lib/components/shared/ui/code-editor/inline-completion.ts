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
import { Annotation, Facet, StateEffect, StateField, type EditorState, type Extension, type Transaction } from '@codemirror/state';
import { completionStatus, selectedCompletion, type Completion } from '@codemirror/autocomplete';
import { acceptedTracking, trackAccepted, type SettledHandler } from './accepted-tracking';

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
  /**
   * Hide the text `replace` covers while the proposal is shown, so the screen reads as it will once accepted — a block
   * whose own text includes the closer the editor already added (`Order(|)` → `Order(a, b) { … }`) would otherwise show
   * that closer after the block. Off by default: a rewrite is clearer with the shorthand it consumes still in view.
   */
  hideReplaced?: boolean;
  /**
   * Whatever the source wants handed back when it is told what became of this proposal (see
   * {@link InlineCompletionOutcomeHandler}). Opaque here: it is never read, only returned.
   */
  meta?: unknown;
}

/** What became of a proposal drawn at the caret. */
export type InlineCompletionOutcomeKind = 'shown' | 'accepted' | 'dismissed';

/**
 * Told, once per proposal, that it was drawn (`shown`), taken with Tab (`accepted`), or went away because Esc was
 * pressed or the user did something other than type what it showed (`dismissed`). A proposal the user simply typed
 * out letter by letter is neither: it was right, and it is gone because it was written. `meta` is what the source put
 * on the proposal. Runs inside the editor update, so it must be cheap and must not dispatch.
 */
export type InlineCompletionOutcomeHandler = (kind: InlineCompletionOutcomeKind, meta: unknown) => void;

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

/**
 * What a completion OPTION may declare to be previewed inline while it is selected.
 *
 * A row whose label is `getCustomer` and whose insertion is a four-line method tells you almost
 * nothing about what pressing Enter will do. Declaring the text here draws it at the caret, in
 * grey, while that row is highlighted — so the list stays a list and the outcome is still visible.
 *
 * It is **not** a competing proposal, which is the whole reason it is allowed to coexist with the
 * popup: it is the popup's own selection, rendered. Tab still belongs to the popup, and accepting
 * the ghost is refused outright (see `preview` on {@link Suggestion}).
 */
export interface PreviewingCompletion {
  ghost?: string;
}

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
  /** Whether the text in `from..to` is hidden while this is drawn (see `InlineCompletion.hideReplaced`). */
  hide?: boolean;
  /**
   * Display only — the rendering of a completion row that is currently selected, not a proposal of
   * this module's own. Tab belongs to the popup, so accepting is refused and Esc is left to close
   * the popup rather than the preview.
   */
  preview?: boolean;
  /** The source's own note, returned with the outcome. */
  meta?: unknown;
  /**
   * Set when this is what is left of an earlier proposal after the user typed its first characters: the same proposal
   * still on screen, not a new one. It is not reported as shown again and it is not asked for again.
   */
  continues?: boolean;
}

/**
 * What the selected row would add to the word being typed — `ddPin(` for `addPin(` after `a` — so a popup with
 * one obvious row still shows its outcome in grey. Only for a row that inserts its own text as is: a row whose
 * `apply` is a function does something this module cannot render.
 */
function restOfRow(state: EditorState, row: Completion | null): string | null {
  if (!row || typeof row.apply === 'function') return null;
  const head = state.selection.main.head;
  const typed = /[\w$]*$/.exec(state.sliceDoc(Math.max(0, head - 64), head))?.[0] ?? '';
  const text = row.apply ?? row.label;
  if (!typed || !text.startsWith(typed) || text.length === typed.length) return null;
  const rest = text.slice(typed.length);
  return rest.includes('\n') ? null : rest;
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
    hide: proposal.hideReplaced && to > from,
    meta: proposal.meta,
  };
}

const setSuggestion = StateEffect.define<Suggestion | null>();

/**
 * What is left of `value` when the transaction only typed its first characters at the caret — `undefined` when it did
 * something else. The ghost text then shrinks as the user types it instead of vanishing and coming back after a new
 * request, which is the difference between typing along with a suggestion and fighting it. `null` when the whole
 * proposal has been typed.
 */
function consumeTyped(value: Suggestion | null, tr: Transaction): Suggestion | null | undefined {
  if (!value || value.preview || value.insert !== value.text || value.from !== value.pos || value.to !== value.pos) return undefined;
  const seen: { typed: string | null; single: boolean } = { typed: null, single: true };
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    if (seen.typed !== null || fromA !== value.pos || toA !== value.pos) seen.single = false;
    seen.typed = inserted.toString();
  });
  const typed = seen.typed;
  if (!seen.single || !typed || !value.text.startsWith(typed)) return undefined;
  const rest = value.text.slice(typed.length);
  if (!rest) return null;
  const pos = value.pos + typed.length;
  return { ...value, text: rest, insert: rest, pos, from: pos, to: pos, continues: true };
}

/** Marks the transaction of a Tab or an Esc, so the outcome is read off what the user did and not guessed. */
const outcomeNote = Annotation.define<'accepted' | 'dismissed'>();

/** The effect that draws `answer` at `pos` without asking the source — for a host that already holds the answer, and
 *  for tests. `null` clears whatever is drawn. */
export function setInlineSuggestion(answer: string | InlineCompletion | null, pos: number): StateEffect<unknown> {
  return setSuggestion.of(answer ? normalise(answer, pos) : null);
}

/** The rendered ghost text. `side: 1` keeps it after the caret. */
class GhostWidget extends WidgetType {
  /** `fresh`: a proposal that just appeared, as opposed to what is left of one being typed along or a popup
   *  row being previewed. Only a fresh one may play the reveal; it is not part of `eq`, so a redraw of the same
   *  text keeps its DOM (and does not replay it). */
  constructor(readonly text: string, readonly fresh = false) {
    super();
  }

  eq(other: GhostWidget): boolean {
    return other.text === this.text;
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = this.fresh ? 'cm-ghost-text cm-ghost-fresh' : 'cm-ghost-text';
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
    // Typing the start of the proposal keeps what is left of it.
    if (tr.docChanged) {
      const rest = consumeTyped(value, tr);
      if (rest !== undefined) return rest;
    }
    // Any other edit or cursor move makes the proposal stale. Cleared here rather than
    // in the plugin so it is impossible for a decoration to outlive the position
    // it was computed for.
    if (tr.docChanged || tr.selection) return null;
    return value;
  },
  provide: (field) =>
    EditorView.decorations.from(field, (value): DecorationSet => {
      if (!value) return Decoration.none;
      const widget = new GhostWidget(value.text, !value.continues && !value.preview);
      if (!(value.hide && value.to > value.from)) return Decoration.set([Decoration.widget({ widget, side: 1 }).range(value.pos)]);
      // what accepting overwrites is hidden, and the ghost stands where it began: a widget inside a hidden range is not drawn
      const at = Math.min(value.from, value.pos);
      return Decoration.set([Decoration.widget({ widget, side: at < value.pos ? -1 : 1 }).range(at), Decoration.replace({}).range(value.from, value.to)], true);
    }),
});

/** The field holding what is drawn — exported for tests. */
export const inlineSuggestionField = suggestionField;

/** Apply the visible suggestion. `false` when there is nothing to accept, so Tab
 *  falls through to indentation as usual.
 *
 *  One dispatch whether it inserts or replaces — a replacement whose range is empty
 *  *is* an insertion, so there is no second path to keep in step with this one. */
export const acceptInlineCompletion: Command = (view) => {
  const current = view.state.field(suggestionField, false);
  // A preview is the popup's selection drawn inline. The popup owns Tab; accepting here would be
  // two things doing the same insertion, and only one of them would tell the completion machinery
  // that a row was chosen.
  if (!current || current.preview) return false;
  view.dispatch({
    changes: { from: current.from, to: current.to, insert: current.insert },
    selection: { anchor: current.from + current.insert.length },
    effects: [
      setSuggestion.of(null),
      trackAccepted.of({ from: current.from, to: current.from + current.insert.length, ghost: current.insert, meta: current.meta }),
    ],
    annotations: outcomeNote.of('accepted'),
    userEvent: 'input.complete',
  });
  return true;
};

/**
 * Take the first line of a multi-line proposal and keep the rest drawn: `Mod-→`, for a block one wants line by line. The
 * line's own break comes with it, so the caret lands where the next line starts and what is left is shown from there. `false`
 * when the proposal is a single line (Tab takes that) or is not an insertion of its own text, so the key keeps its usual move.
 */
export const acceptInlineCompletionLine: Command = (view) => {
  const current = view.state.field(suggestionField, false);
  if (!current || current.preview || current.insert !== current.text) return false;
  const brk = current.text.indexOf('\n');
  if (brk < 0) return false;
  const head = current.text.slice(0, brk + 1);
  const rest = current.text.slice(brk + 1);
  const pos = current.from + head.length;
  view.dispatch({
    changes: { from: current.from, to: current.to, insert: head },
    selection: { anchor: pos },
    effects: [
      setSuggestion.of(rest ? { ...current, text: rest, insert: rest, pos, from: pos, to: pos, hide: false, continues: true } : null),
      trackAccepted.of({ from: current.from, to: pos, ghost: head, meta: current.meta }),
    ],
    annotations: outcomeNote.of('accepted'),
    userEvent: 'input.complete',
  });
  return true;
};

/** Dismiss the visible suggestion, and remember not to offer it again until
 *  something changes. `false` when there is nothing shown, so Esc keeps its other
 *  meanings (closing a panel, clearing a selection). */
export const dismissInlineCompletion: Command = (view) => {
  const current = view.state.field(suggestionField, false);
  // Esc over a preview means "close the popup", which is what the preview belongs to.
  if (!current || current.preview) return false;
  view.dispatch({ effects: setSuggestion.of(null), annotations: outcomeNote.of('dismissed') });
  view.plugin(inlineCompletionPlugin)?.dismiss(current.pos);
  return true;
};

/** Is a suggestion on screen? Lets a host show a hint in the status bar. */
export function inlineCompletionActive(view: EditorView): boolean {
  return view.state.field(suggestionField, false) != null;
}

/** Did the transactions only type out what `prev` was showing, from the caret it was drawn at? */
function typedOut(prev: Suggestion, trs: readonly Transaction[]): boolean {
  const inserted: string[] = [];
  let other = false;
  for (const tr of trs) {
    tr.changes.iterChanges((fromA, toA, _fromB, _toB, text) => {
      if (fromA === toA && fromA === prev.pos && text.length > 0) inserted.push(text.toString());
      else other = true;
    });
  }
  return !other && inserted.length === 1 && prev.text.startsWith(inserted[0]);
}

/**
 * What the transactions that just ran did to the proposal on screen, as the handler is to be told.
 *
 * Pure — states in, outcomes out — so the rules (a Tab accepts, an Esc or an unrelated edit or move dismisses,
 * typing the proposal out ends it without a verdict, a popup preview is not a proposal) can be tested without a view.
 */
export function inlineOutcomes(
  before: EditorState,
  trs: readonly Transaction[],
  after: EditorState,
): { kind: InlineCompletionOutcomeKind; meta: unknown }[] {
  const prev = before.field(suggestionField, false) ?? null;
  const next = after.field(suggestionField, false) ?? null;
  if (prev === next) return [];
  const out: { kind: InlineCompletionOutcomeKind; meta: unknown }[] = [];
  // what is left of the same proposal after typing its start is neither a verdict nor a new proposal
  if (next?.continues && prev) return [];
  if (prev && !prev.preview) {
    const noted = trs.map((t) => t.annotation(outcomeNote)).find((n) => n !== undefined);
    const edited = trs.some((t) => t.docChanged);
    const moved = trs.some((t) => t.selection);
    const kind: InlineCompletionOutcomeKind | null =
      noted ?? (edited ? (typedOut(prev, trs) ? null : 'dismissed') : moved ? 'dismissed' : null);
    if (kind) out.push({ kind, meta: prev.meta });
  }
  if (next && !next.preview) out.push({ kind: 'shown', meta: next.meta });
  return out;
}

/** Per-editor configuration.
 *
 * A facet rather than module state: two editors can be alive at once — Bennu and
 * Picus in a tabbed window, or two query tabs — and each needs its own source.
 * Module-level configuration would give whichever mounted last to all of them. */
const inlineCompletionConfig = Facet.define<
  { source: InlineCompletionSource; delay: number; onOutcome?: InlineCompletionOutcomeHandler },
  { source: InlineCompletionSource | null; delay: number; onOutcome?: InlineCompletionOutcomeHandler }
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

    /**
     * The label of the popup row previewed last time we looked.
     *
     * Arrowing through the popup changes neither the document nor the selection, so without this
     * the preview would be drawn for whichever row happened to be selected when the popup opened
     * and never follow the arrows — which is worse than not drawing it, because it would be
     * showing one row's outcome under another row's highlight.
     */
    private previewedLabel: string | null = null;

    constructor(private readonly view: EditorView) {}

    update(update: ViewUpdate) {
      const { onOutcome } = update.state.facet(inlineCompletionConfig);
      if (onOutcome) {
        for (const { kind, meta } of inlineOutcomes(update.startState, update.transactions, update.state)) {
          try {
            onOutcome(kind, meta);
          } catch {
            // A reporting hook that throws must not take the editor with it.
          }
        }
      }
      // A transaction that only opened or closed the popup changes neither, and it
      // is exactly the moment the answer to "may I show something?" flips.
      const popupOpen = completionStatus(update.state) !== null;
      const popupChanged = popupOpen !== this.popupWasOpen;
      this.popupWasOpen = popupOpen;

      const label = selectedCompletion(update.state)?.label ?? null;
      const selectionMoved = label !== this.previewedLabel;
      this.previewedLabel = label;

      if (!update.docChanged && !update.selectionSet && !popupChanged && !selectionMoved) return;
      // Typing means the situation changed, so a previous dismissal no longer
      // applies. Moving the caret elsewhere is handled by the position check.
      if (update.docChanged) this.dismissedAt = -1;
      // while the user types along with a proposal there is nothing to ask for: it is still the answer
      if (update.docChanged && update.state.field(suggestionField, false)?.continues) {
        this.cancel();
        return;
      }
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
      //
      // What DOES appear there is the selected row's own preview: not a proposal of ours, so it
      // competes for nothing — see `PreviewingCompletion`.
      if (completionStatus(state) !== null) {
        const selected = selectedCompletion(state);
        const ghost = (selected as PreviewingCompletion | null)?.ghost ?? restOfRow(state, selected);
        const shown = state.field(suggestionField, false);
        const next: Suggestion | null = ghost
          ? { text: ghost, pos: cursor.head, from: cursor.head, to: cursor.head, insert: '', preview: true }
          : null;
        // Only when something actually changes: this runs on every popup transaction, and a
        // dispatch per keystroke that redraws the same widget is work for nothing.
        if ((shown?.text ?? null) !== (next?.text ?? null)) {
          this.view.dispatch({ effects: setSuggestion.of(next) });
        }
        return;
      }
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
  // The reveal: the line is uncovered from the left over the app's fast animation duration (0 when animations are
  // off). Opt-in through `data-motion` on the editor, set by the `animate` completion preference.
  '@keyframes cm-ghost-reveal': {
    '0%': { clipPath: 'inset(0 100% 0 0)' },
    '100%': { clipPath: 'inset(0 0 0 0)' },
  },
  '&[data-motion~="completion"] .cm-ghost-fresh': {
    animation: 'cm-ghost-reveal var(--anim-dur-fast, 0ms) ease-out',
  },
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
  /** Told what became of each proposal; see {@link InlineCompletionOutcomeHandler}. */
  onOutcome?: InlineCompletionOutcomeHandler;
  /** Told, once the user is done with an accepted proposal, what stands in its place; see {@link acceptedTracking}. */
  onSettled?: SettledHandler;
}): Extension {
  return [
    ...(options.onSettled ? [acceptedTracking(options.onSettled)] : []),
    inlineCompletionConfig.of({ source: options.source, delay: options.delay ?? 120, onOutcome: options.onOutcome }),
    suggestionField,
    inlineCompletionPlugin,
    ghostTheme,
  ];
}
