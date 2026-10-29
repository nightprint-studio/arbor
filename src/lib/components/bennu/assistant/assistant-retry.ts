/**
 * The status line shown while a run waits to send a failed request again.
 *
 * The backend reports each retry once (attempt, how many to expect, and the moment it will be made); the
 * countdown is worked out here from the clock, so nothing is sent per second. Pure, so the wording is tested.
 */

export interface RetryStatus {
  /** Which retry this is (1 for the first). */
  attempt: number;
  /** About how many there will be at most. */
  max: number;
  /** When the request is sent again, in ms since the epoch. */
  until: number;
  /** `offline`, `timeout`, `stream_broke`, `rate_limited`, `overloaded`, `server`. */
  kind: string;
  /** The few words for what went wrong: "No connection", "The API is overloaded". */
  message: string;
  /** The provider's or the network's own words. */
  detail: string;
}

/** Whole seconds left before the retry, never negative. */
export function secondsLeft(r: RetryStatus, now: number): number {
  return Math.max(0, Math.ceil((r.until - now) / 1000));
}

/** `No connection — retrying in 8 s (attempt 3/11)`, and `… retrying now …` once the wait is over. */
export function retryLine(r: RetryStatus, now: number): string {
  const left = secondsLeft(r, now);
  const when = left > 0 ? `retrying in ${left} s` : 'retrying now';
  return `${r.message} — ${when} (attempt ${r.attempt}/${r.max})`;
}
