/**
 * Batches the outcomes of ghost-text suggestions on their way to the backend's counters.
 *
 * The editor reports an outcome for every suggestion it draws; sending each as its own call would put a request on the
 * path of nearly every keystroke. They are collected here and sent together every few seconds, and at once when the
 * window is hidden, so a closing window loses nothing. Only numbers travel: the origin, the confidence, what happened.
 */

import { flcFeedback, flcLearnSettled, type FlcFeedbackEvent, type FlcSettledEvent } from '$lib/ipc/bennu/flc';

/** How long outcomes wait before they are sent. */
export const FEEDBACK_INTERVAL_MS = 4000;
/** More than this waiting means the backend is not answering; the oldest are dropped rather than growing. */
const MAX_PENDING = 500;

export interface FeedbackQueue<T = FlcFeedbackEvent> {
  push(event: T): void;
  /** Sends what is waiting now. `final` also asks the backend to write it to disk. */
  flush(final?: boolean): void;
}

export function createFeedbackQueue<T = FlcFeedbackEvent>(
  send: (events: T[], flush: boolean) => Promise<unknown>,
  intervalMs = FEEDBACK_INTERVAL_MS,
): FeedbackQueue<T> {
  let pending: T[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  function flush(final = false) {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    if (pending.length === 0 && !final) return;
    const batch = pending;
    pending = [];
    // Counting is a courtesy: a call that fails loses a few counters and nothing else.
    void send(batch, final).catch(() => {});
  }

  return {
    push(event) {
      pending.push(event);
      if (pending.length > MAX_PENDING) pending = pending.slice(-MAX_PENDING);
      if (timer === null) timer = setTimeout(() => flush(false), intervalMs);
    },
    flush,
  };
}

/** The queue the Java editor reports into. */
export const flcFeedbackQueue = createFeedbackQueue(flcFeedback);

/** The queue for what became of each accepted ghost text (see `bennu-learn`). */
export const flcLearnQueue = createFeedbackQueue<FlcSettledEvent>(async (events, flush) => {
  const notes = await flcLearnSettled(events, flush);
  // loaded when there is something to show: the toast store is a rune module, and the queue is also used where none runs
  if (notes.length) (await import('$lib/feedback/stores/toasts.svelte')).toastStore.show(`Bennu learned: ${notes.join('  |  ')}`, 'info', 6000);
});

if (typeof document !== 'undefined') {
  // Hidden or closing: send what is waiting, and have it written.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      flcFeedbackQueue.flush(true);
      flcLearnQueue.flush(true);
    }
  });
}
