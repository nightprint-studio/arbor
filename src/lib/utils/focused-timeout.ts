/**
 * A `setTimeout` that does not run its task while the window is in the background — it waits,
 * and runs once the window is focused and visible again.
 *
 * Almost everything the editor asks the backend for is a function of what is on screen: the
 * squiggles, the usage counts, the inlay hints, the folds. Re-asking while nobody is looking (an
 * index rebuild that lands while the window is behind another one, a file changed on disk, a git
 * status tick) is the backend working for a screen that is not being read — and on a big project
 * those are the expensive answers. Deferring them costs nothing: the moment the window comes back
 * every pending question is asked, and the answer is there about when the eye is.
 *
 * ## Why queued, not dropped
 *
 * The task may be the only thing that would ever bring the view up to date: the effect that
 * scheduled it ran because something changed, and will not run again for the same change. So a
 * task that comes due in the background is **kept** and flushed on focus. Nothing piles up: an
 * effect that re-runs cancels its own previous timeout ([`clearFocusedTimeout`]), so what is
 * waiting is at most one task per effect — the latest. They are flushed in the order they were
 * scheduled.
 *
 * ## Shape
 *
 * Same contract as `setTimeout` / `clearTimeout`, so adopting it is a rename. The id is a plain
 * number (negative, so it can never collide with a real timer id).
 */

/** What the scheduler needs to know about the window — injected so the logic is testable. */
export interface FocusEnv {
  /** The window is visible and has focus. */
  isActive(): boolean;
  /** Call `cb` whenever the window may have become active. Returns an unsubscribe. */
  onActivate(cb: () => void): () => void;
}

export interface FocusedTimers {
  setFocusedTimeout(task: () => void, ms: number): number;
  clearFocusedTimeout(id: number | undefined | null): void;
  /** Tasks that came due while inactive and are waiting for focus. */
  pending(): number;
}

interface Entry {
  task: () => void;
  /** The live timer; `undefined` once it fired and the task is waiting for focus. */
  timer: ReturnType<typeof setTimeout> | undefined;
}

export function createFocusedTimers(env: FocusEnv): FocusedTimers {
  const entries = new Map<number, Entry>();
  let nextId = -1;
  let unsubscribe: (() => void) | null = null;

  const flush = () => {
    if (!env.isActive()) return;
    for (const [id, entry] of [...entries]) {
      if (entry.timer !== undefined) continue; // still counting down — not due yet
      entries.delete(id);
      try {
        entry.task();
      } catch (e) {
        // One failing task must not stop the others from running.
        console.error('[focused-timeout] task failed', e);
      }
    }
    if (entries.size === 0) stopListening();
  };

  const startListening = () => {
    if (!unsubscribe) unsubscribe = env.onActivate(flush);
  };
  const stopListening = () => {
    unsubscribe?.();
    unsubscribe = null;
  };

  return {
    setFocusedTimeout(task, ms) {
      const id = nextId--;
      const entry: Entry = { task, timer: undefined };
      entries.set(id, entry);
      entry.timer = setTimeout(() => {
        entry.timer = undefined;
        if (env.isActive()) {
          entries.delete(id);
          task();
          return;
        }
        startListening(); // due, but nobody is looking: wait for focus
      }, ms);
      return id;
    },

    clearFocusedTimeout(id) {
      if (id === undefined || id === null) return;
      const entry = entries.get(id);
      if (!entry) return;
      if (entry.timer !== undefined) clearTimeout(entry.timer);
      entries.delete(id);
      if (entries.size === 0) stopListening();
    },

    pending() {
      let n = 0;
      for (const entry of entries.values()) if (entry.timer === undefined) n += 1;
      return n;
    },
  };
}

/** The real window: visible and focused, re-checked on `focus` and `visibilitychange`. */
const domEnv: FocusEnv = {
  isActive: () =>
    typeof document === 'undefined' || (document.visibilityState === 'visible' && document.hasFocus()),
  onActivate(cb) {
    window.addEventListener('focus', cb);
    document.addEventListener('visibilitychange', cb);
    return () => {
      window.removeEventListener('focus', cb);
      document.removeEventListener('visibilitychange', cb);
    };
  },
};

const shared = createFocusedTimers(domEnv);

/** `setTimeout`, held back while the window is in the background. See the module doc. */
export const setFocusedTimeout = shared.setFocusedTimeout;
/** `clearTimeout` for [`setFocusedTimeout`]. Accepts `undefined`, like the real one. */
export const clearFocusedTimeout = shared.clearFocusedTimeout;
