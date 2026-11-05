/**
 * Edge "peek" — a hidden rail that slides in while the pointer is on the window's edge.
 *
 * One state per edge, and each is closed by a global `mousemove` watcher rather than by
 * `mouseleave` on the thin trigger: the trigger is a few pixels wide, so leaving it by moving ONTO
 * the rail that just appeared would close the peek the instant it opened. Watching the pointer
 * against an explicit width (the rail's own) keeps it open exactly while the pointer is over the
 * rail, and closes it the moment it is not.
 *
 * Used by every window that can hide its activity bars: Corvus's "hidden" position, Bennu's
 * distraction-free mode. The triggers themselves are `EdgePeekTriggers`.
 */

/** The activity bar's width — how far from the edge the pointer may roam before the peek ends. */
const RAIL_WIDTH = 38;

export function createEdgePeek(width = RAIL_WIDTH) {
  let left = $state(false);
  let right = $state(false);
  let leftWatch: ((e: MouseEvent) => void) | null = null;
  let rightWatch: ((e: MouseEvent) => void) | null = null;

  function endLeft() {
    if (leftWatch) window.removeEventListener('mousemove', leftWatch);
    leftWatch = null;
    left = false;
  }
  function endRight() {
    if (rightWatch) window.removeEventListener('mousemove', rightWatch);
    rightWatch = null;
    right = false;
  }
  function startLeft() {
    if (leftWatch) return;
    left = true;
    leftWatch = (e) => { if (e.clientX > width) endLeft(); };
    window.addEventListener('mousemove', leftWatch);
  }
  function startRight() {
    if (rightWatch) return;
    right = true;
    rightWatch = (e) => { if (e.clientX < window.innerWidth - width) endRight(); };
    window.addEventListener('mousemove', rightWatch);
  }

  return {
    /** The left rail is being peeked. */
    get left() { return left; },
    /** The right rail is being peeked. */
    get right() { return right; },
    startLeft,
    startRight,
    /** Close both — when the mode that needed the peek is over, so no watcher fires into nothing. */
    reset() { endLeft(); endRight(); },
  };
}
