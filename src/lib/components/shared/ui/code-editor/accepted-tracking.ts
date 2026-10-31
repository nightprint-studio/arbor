/**
 * What becomes of ghost text after it is accepted.
 *
 * Tab writes the proposal, and then the user may leave it, cut its tail, write on, swap a part, re-lay it out or take
 * it back. This follows the written range through every later edit and reports once — with the proposal and what stands
 * in its place — when the user is done with it: the caret has left its lines, the range has been emptied, or nothing has
 * been typed in it for a while.
 *
 * Reported when the user is *done*, not at the first change, on purpose: a character typed into the proposal and deleted
 * again leaves the text as it was, and that is what is reported. A hesitation that was taken back is not a correction.
 *
 * Generic: it knows nothing of what the proposal is or who listens. The source's `meta` rides along, opaque.
 */

import { StateEffect, StateField, type ChangeDesc, type EditorState, type Extension } from '@codemirror/state';
import { ViewPlugin, type EditorView, type ViewUpdate } from '@codemirror/view';

/** What was accepted: the range it was written to, and the text of the proposal. */
export interface AcceptedProposal {
  from: number;
  to: number;
  ghost: string;
  meta: unknown;
  /** Set once reported; carried through the mapping, so a proposal is never reported twice. */
  done?: boolean;
}

/** The report: the proposal, and what stands in its place now (empty when it is gone). */
export interface SettledProposal {
  ghost: string;
  after: string;
  meta: unknown;
}

export type SettledHandler = (settled: SettledProposal) => void;

/** How long a proposal that is still being typed into must be left alone before it counts as finished. */
export const SETTLE_IDLE_MS = 6000;

/** Start following a proposal that was just written (positions in the document the transaction produces). */
export const trackAccepted = StateEffect.define<AcceptedProposal>();
const stopTracking = StateEffect.define<null>();

/**
 * The range after `changes`. An insertion at its end extends it (the user writing on); one at its start stays outside
 * (text typed before it is not part of it). A range wholly deleted comes out empty.
 */
export function mapAccepted(tracked: AcceptedProposal, changes: ChangeDesc): AcceptedProposal {
  if (changes.empty) return tracked;
  const from = changes.mapPos(tracked.from, 1);
  const to = Math.max(from, changes.mapPos(tracked.to, 1));
  return { ...tracked, from, to };
}

const trackedField = StateField.define<AcceptedProposal | null>({
  create: () => null,
  update(value, tr) {
    let next = value ? mapAccepted(value, tr.changes) : null;
    for (const e of tr.effects) {
      if (e.is(trackAccepted)) next = e.value;
      else if (e.is(stopTracking)) next = null;
    }
    return next;
  },
});

/** Whether the caret is still on the lines the proposal was written to. */
function caretNear(state: EditorState, tracked: AcceptedProposal): boolean {
  const head = state.selection.main.head;
  return head >= state.doc.lineAt(tracked.from).from && head <= state.doc.lineAt(tracked.to).to;
}

const trackingPlugin = (onSettled: SettledHandler) =>
  ViewPlugin.fromClass(
    class {
      private timer: ReturnType<typeof setTimeout> | null = null;
      constructor(private readonly view: EditorView) {}

      update(update: ViewUpdate) {
        const before = update.startState.field(trackedField);
        const now = update.state.field(trackedField);
        // a new proposal taken: the one before it is finished, wherever the user left it
        if (before && now !== before && update.transactions.some((t) => t.effects.some((e) => e.is(trackAccepted)))) {
          this.report(mapThrough(before, update), update.state);
        }
        if (!now) return this.disarm();
        if (now.done) return;
        if (now.from >= now.to) return this.settle(update.state, now);
        if (update.docChanged) this.arm();
        else if (update.selectionSet && !caretNear(update.state, now)) this.settle(update.state, now);
        else if (!this.timer) this.arm();
      }

      destroy() {
        this.disarm();
        const now = this.view.state.field(trackedField, false);
        if (now) this.report(now, this.view.state);
      }

      private arm() {
        this.disarm();
        this.timer = setTimeout(() => {
          this.timer = null;
          const now = this.view.state.field(trackedField, false);
          if (now) this.settle(this.view.state, now);
        }, SETTLE_IDLE_MS);
      }

      private disarm() {
        if (this.timer) clearTimeout(this.timer);
        this.timer = null;
      }

      private settle(state: EditorState, tracked: AcceptedProposal) {
        this.report(tracked, state);
        // dispatching is not allowed inside an update; the next tick is
        queueMicrotask(() => {
          if (this.view.state.field(trackedField, false) === tracked) this.view.dispatch({ effects: stopTracking.of(null) });
        });
      }

      private report(tracked: AcceptedProposal, state: EditorState) {
        if (tracked.done) return;
        tracked.done = true;
        this.disarm();
        const after = tracked.to > tracked.from ? state.doc.sliceString(tracked.from, Math.min(tracked.to, state.doc.length)) : '';
        onSettled({ ghost: tracked.ghost, after, meta: tracked.meta });
      }
    },
  );

function mapThrough(tracked: AcceptedProposal, update: ViewUpdate): AcceptedProposal {
  return mapAccepted(tracked, update.changes);
}

/** Follow every accepted proposal and report it to `onSettled` once the user is done with it. */
export function acceptedTracking(onSettled: SettledHandler): Extension {
  return [trackedField, trackingPlugin(onSettled)];
}
