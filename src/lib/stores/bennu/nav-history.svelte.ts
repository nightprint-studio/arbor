/**
 * Bennu navigation history — an IntelliJ-style back / forward jump stack.
 *
 * The editor records a "place" (file + line) whenever the caret makes a real jump
 * (a different file, or a large in-file hop from a go-to / structure / find click),
 * and the window steps through them with the browser-standard <kbd>Alt+←</kbd> /
 * <kbd>Alt+→</kbd> (IntelliJ's own chord, <kbd>Ctrl+Alt+←/→</kbd>, collides with the
 * Intel/NVIDIA screen-rotation hotkeys on Windows, so we mirror the browser instead).
 *
 * Rune store (CLAUDE.md): private `$state`, exposed via getters + methods. Holds pure
 * session state — no persistence.
 */

export interface NavPlace {
  file: string;
  line: number;
  col: number;
}

/** Cap the ring so a long editing session can't grow it without bound. */
const MAX_PLACES = 100;

function createBennuNavStore() {
  let places = $state<NavPlace[]>([]);
  // Points at the current place in `places`; -1 when empty.
  let index = $state(-1);

  /** Two places are "the same spot" when they share a file and sit within a line of
   *  each other — so a jump that lands one line off (a decl vs its body) collapses
   *  instead of littering the history with near-duplicates. */
  function samePlace(a: NavPlace, b: NavPlace): boolean {
    return a.file === b.file && Math.abs(a.line - b.line) <= 1;
  }

  return {
    get canBack() { return index > 0; },
    get canForward() { return index < places.length - 1; },

    /** Record a visited place. Collapses a near-duplicate of the current entry, and
     *  truncates any forward history (recording after a Back starts a new branch,
     *  exactly like a browser). */
    record(place: NavPlace) {
      const cur = index >= 0 ? places[index] : null;
      if (cur && samePlace(cur, place)) {
        places[index] = place; // refine the exact line/col, keep the slot
        return;
      }
      const next = places.slice(0, index + 1);
      next.push(place);
      if (next.length > MAX_PLACES) next.shift();
      places = next;
      index = next.length - 1;
    },

    /** Step back one place, or `null` when already at the oldest. */
    back(): NavPlace | null {
      if (index <= 0) return null;
      index -= 1;
      return places[index];
    },

    /** Step forward one place, or `null` when already at the newest. */
    forward(): NavPlace | null {
      if (index >= places.length - 1) return null;
      index += 1;
      return places[index];
    },

    /** Drop the whole history (e.g. on project switch). */
    reset() {
      places = [];
      index = -1;
    },
  };
}

export const bennuNavStore = createBennuNavStore();
