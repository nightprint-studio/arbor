/**
 * The capped append behind every Bennu console: the build log, the JUnit run, the Cargo run, and
 * each run tab's own transcript.
 *
 * Four copies of the same slice-then-push lived in three stores before this — four places to get
 * the cap off by one, and four places that had to learn the same lesson about batching.
 *
 * It takes a BATCH, not a line, because a batch is what the event stream actually delivers: a
 * program's stdout arrives as a burst of small frames, and a window that has been in the background
 * hands its whole backlog over at once when it regains focus (the webview is power-throttled while
 * unfocused, the backend that feeds it is not). One rebuilt array per batch instead of one per line
 * is the difference between a console that keeps up and one that spends its first second back
 * copying a ten-thousand-element array a thousand times.
 *
 * Type-only import: the edge to `run.svelte` is erased at build, so there is no module cycle.
 */

import type { RunLogLine } from './run.svelte';

/**
 * Cap the retained log so a chatty build/run can't grow the buffer unbounded.
 *
 * It used to be 3000, which was really a cap on the DOM: every retained line was a rendered row,
 * and a Tomcat or Spring Boot startup reached it in seconds — so the beginning of the run, which is
 * where the interesting failures are, had already scrolled out of existence by the time you looked.
 * The console renders only what is on screen now, so what this bounds is memory, and memory affords
 * a great deal more.
 */
export const MAX_LOG_LINES = 10_000;

/** `prev` with `entries` appended, trimmed to the last {@link MAX_LOG_LINES}. Returns `prev`
 *  itself for an empty batch, so an empty flush propagates no reactivity. */
export function appendLogLines(prev: RunLogLine[], entries: RunLogLine[]): RunLogLine[] {
  if (entries.length === 0) return prev;
  const next = prev.concat(entries);
  return next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next;
}
