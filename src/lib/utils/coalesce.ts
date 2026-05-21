// =============================================================================
// Event coalescing helpers — collapse bursts of repeated events into a single
// reactivity tick per animation frame.
//
// Motivation: when the WebView2 process is throttled (EcoQoS / IDLE priority)
// while the Arbor window is unfocused, Tauri IPC events keep arriving in the
// channel but the JS message loop drains them slowly.  When focus returns,
// hundreds or thousands of buffered events run their handlers back-to-back,
// each touching `$state` and re-scheduling Svelte reactivity.  The result is
// the brief UI freeze that users observe right after Alt-Tab back to Arbor.
//
// These helpers wrap any event handler so a burst is folded into one delivery
// per animation frame:
//
//   coalesceBatch:        accumulate all payloads, hand the array to the
//                         handler on the next frame.  Use for streaming text
//                         (job stdout, log lines) where every payload matters.
//
//   coalesceLatest:       keep only the last payload, drop intermediates.
//                         Use for idempotent "state changed" notifications —
//                         re-running the handler once with the final payload
//                         captures the final state regardless of how many
//                         events fired.
//
//   coalesceLatestByKey:  keep the last payload **per key** and deliver a
//                         map's worth of entries on the next frame.  Use for
//                         per-entity status streams (pipeline-update per
//                         run_id, refresh per repo_id, …) where N entities
//                         share one event stream.
//
// Scheduling uses `requestAnimationFrame` when available so the flush is
// naturally aligned with the next paint, and so background-tab/unfocused
// throttling keeps the queue dormant until the window is visible again.
// Outside a browser (or before rAF is wired up) we fall back to a 0 ms
// timer — still yields the microtask queue so a burst coalesces.
// =============================================================================

type Schedule = (cb: () => void) => void;

const schedule: Schedule = typeof requestAnimationFrame === 'function'
  ? (cb) => { requestAnimationFrame(() => cb()); }
  : (cb) => { setTimeout(cb, 0); };

/** Wrap `handle` so every payload that arrives within the same frame is
 *  collected, and `handle` runs once per frame with the full array.  Order
 *  is preserved.  Empty bursts never call the handler. */
export function coalesceBatch<P>(handle: (batch: P[]) => void): (e: P) => void {
  let queue: P[] = [];
  let scheduled  = false;
  return (e: P) => {
    queue.push(e);
    if (scheduled) return;
    scheduled = true;
    schedule(() => {
      const drained = queue;
      queue        = [];
      scheduled    = false;
      handle(drained);
    });
  };
}

/** Wrap `handle` so only the **most recent** payload is delivered per frame.
 *  All intermediate payloads are dropped.  Use for "something changed —
 *  reload" semantics where the handler is idempotent. */
export function coalesceLatest<P>(handle: (latest: P) => void): (e: P) => void {
  let latest: P | undefined;
  let scheduled = false;
  let has = false;
  return (e: P) => {
    latest = e;
    has = true;
    if (scheduled) return;
    scheduled = true;
    schedule(() => {
      scheduled = false;
      if (!has) return;
      const p = latest as P;
      latest = undefined;
      has = false;
      handle(p);
    });
  };
}

/** Wrap `handle` so the latest payload is kept **per key** within the same
 *  frame.  At flush time `handle` runs once per distinct key with that key's
 *  latest payload.  Insertion order of distinct keys is preserved. */
export function coalesceLatestByKey<P>(
  handle: (latest: P) => void,
  keyOf:  (p: P) => string,
): (e: P) => void {
  const latest = new Map<string, P>();
  let scheduled = false;
  return (e: P) => {
    const k = keyOf(e);
    if (!latest.has(k)) latest.set(k, e);   // preserve first-seen order
    latest.set(k, e);                       // but overwrite value with newest
    if (scheduled) return;
    scheduled = true;
    schedule(() => {
      scheduled = false;
      const drained = Array.from(latest.values());
      latest.clear();
      for (const p of drained) handle(p);
    });
  };
}
