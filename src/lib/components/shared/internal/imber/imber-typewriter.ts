/**
 * How much of the assistant's text to show now.
 *
 * Text arrives from the network in bursts — nothing for a moment, then a paragraph. Showing it as it comes
 * looks jerky; this paces it: a steady reading speed, faster when there is a long backlog so the display
 * never falls far behind what has been received. Pure, so it is tested; the component only calls it once a
 * frame.
 */

/** Characters per millisecond when nothing is waiting: about 90 a second, a comfortable reading pace. */
const BASE_RATE = 0.09;
/** A backlog is worked off in about this long, however long it is. */
const CATCH_UP_MS = 320;
/** A stalled frame (a background tab, a long task) is not allowed to jump the text ahead. */
const MAX_FRAME_MS = 64;

/** The length to show after `dtMs` more, given `shown` characters shown and `total` received. */
export function nextLength(shown: number, total: number, dtMs: number): number {
  if (shown >= total) return total;
  const backlog = total - shown;
  const rate = Math.max(BASE_RATE, backlog / CATCH_UP_MS);
  const step = Math.max(1, Math.ceil(rate * Math.min(Math.max(dtMs, 0), MAX_FRAME_MS)));
  return Math.min(total, shown + step);
}
