/**
 * Completion requests paced to the typing — one round-trip for a burst of keys, not one per key.
 *
 * A completion source that asks a backend on every keystroke does needless work exactly when the
 * user is least interested in the answer: typing `identity_resolver` at speed asks seventeen questions
 * whose answers are thrown away the moment the next key lands. Three rules, all in one place so every
 * language's source paces the same way:
 *
 * - **Settle during a burst.** A key that arrives within {@link BURST_MS} of the previous one is part
 *   of a burst: the request waits {@link SETTLE_MS} first, and is dropped if another key arrives
 *   meanwhile. A key after a pause is asked at once — the first letter, a `.`, an explicit
 *   <kbd>Ctrl</kbd>+<kbd>Space</kbd> never wait.
 * - **Sample a long burst.** Settling alone would leave the list frozen for as long as the typing
 *   lasts, so once {@link MAX_STALE_MS} have passed since the last request that was actually sent,
 *   the next one goes out without waiting. A list that lags a few letters behind is useful; one that
 *   lags a whole identifier is not.
 * - **Reuse the same question.** A request identical to the one in flight — same file, offset and
 *   text, as when the editor re-asks without the document having changed — shares its answer instead
 *   of sending a second.
 *
 * Only the newest request resolves: an answer arriving for a superseded one comes back `null`, which
 * a CodeMirror source reads as "nothing to show for this query" while the newer query is pending.
 */

/** Two keys closer than this are one burst of typing. */
export const BURST_MS = 140;
/** How long a request inside a burst waits for the typing to pause. */
export const SETTLE_MS = 80;
/** The longest a burst goes without a request actually being sent. */
export const MAX_STALE_MS = 350;

/** What the pacing reads the time from — injectable so the rules can be tested without timers. */
export interface Clock {
  now(): number;
  sleep(ms: number): Promise<void>;
}

const realClock: Clock = {
  now: () => performance.now(),
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
};

export interface CompletionRequests<T> {
  /**
   * Ask `fetch` for the answer to the question identified by `key`, paced as the module describes.
   * `null` when the request was superseded by a newer one before it was sent or before it answered.
   * `explicit` requests (the user asked) are never delayed.
   */
  request(key: string, fetch: () => Promise<T>, explicit?: boolean): Promise<T | null>;
  /** Whether the request that was started as `generation` is still the newest. */
  isCurrent(generation: number): boolean;
  /** The generation of the newest request — read right after `request` starts, to guard later awaits. */
  readonly generation: number;
}

export function createCompletionRequests<T>(clock: Clock = realClock): CompletionRequests<T> {
  let generation = 0;
  let lastKeyAt = Number.NEGATIVE_INFINITY;
  let lastSentAt = Number.NEGATIVE_INFINITY;
  let inFlight: { key: string; promise: Promise<T> } | null = null;

  return {
    get generation() {
      return generation;
    },
    isCurrent(which: number) {
      return which === generation;
    },
    async request(key, fetch, explicit = false) {
      const mine = ++generation;
      const now = clock.now();
      const bursting = !explicit && now - lastKeyAt < BURST_MS;
      const stale = now - lastSentAt >= MAX_STALE_MS;
      lastKeyAt = now;
      if (bursting && !stale) {
        await clock.sleep(SETTLE_MS);
        if (mine !== generation) return null;
      }
      let promise: Promise<T>;
      if (inFlight && inFlight.key === key) {
        promise = inFlight.promise;
      } else {
        lastSentAt = clock.now();
        promise = fetch();
        const entry = { key, promise };
        inFlight = entry;
        // Forget it once it has answered: a later identical question is a new question about a
        // backend that may have learned something since (an index build finishing, say).
        const forget = () => {
          if (inFlight === entry) inFlight = null;
        };
        promise.then(forget, forget);
      }
      const answer = await promise;
      return mine === generation ? answer : null;
    },
  };
}
