/**
 * Opt-in timing for the interactive paths (completion, ghost text, go-to), to tell the frontend's share
 * from the backend's.
 *
 * Switch it on from the devtools console with `__bennuPerf = true` (off again with `false`) — a debugging
 * aid, not a setting, so it is deliberately not persisted. Each timed step prints
 * `[bennu-perf] label 12.3ms` to the console. Set the backend's half with `ARBOR_BE_PERF=<ms>` in the
 * environment of the app: its `[perf]` lines on stderr give wait / run / write per request, and the
 * difference between the round trip printed here and the `wait+run+write` there is the IPC and
 * serialisation cost of shipping the source across.
 */

const on = (): boolean => (globalThis as { __bennuPerf?: boolean }).__bennuPerf === true;

/** Run `fn`, printing how long it took when timing is on. Free (one property read) when off. */
export async function timed<T>(label: string, fn: () => Promise<T> | T, detail?: () => string): Promise<T> {
  if (!on()) return fn();
  const t0 = performance.now();
  try {
    return await fn();
  } finally {
    const ms = (performance.now() - t0).toFixed(1);
    console.debug(`[bennu-perf] ${label} ${ms}ms${detail ? ` (${detail()})` : ''}`);
  }
}

/** Start a stopwatch; call the returned function to print the elapsed time under `label`. */
export function stopwatch(label: string): (detail?: string) => void {
  if (!on()) return () => {};
  const t0 = performance.now();
  return (detail) => {
    console.debug(`[bennu-perf] ${label} ${(performance.now() - t0).toFixed(1)}ms${detail ? ` (${detail})` : ''}`);
  };
}
