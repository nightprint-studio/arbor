/**
 * Bennu navigation history — an IntelliJ-style back / forward jump stack.
 *
 * The editor records a "place" (file + line) whenever the caret makes a real jump
 * (a different file, or a large in-file hop from a go-to / structure / find click),
 * and the window steps through them with IntelliJ's <kbd>Ctrl+Alt+←</kbd> /
 * <kbd>Ctrl+Alt+→</kbd>.
 *
 * Rune store (CLAUDE.md): private `$state`, exposed via getters + methods. Holds pure
 * session state — no persistence.
 *
 * The mutators read-modify-write `places`/`index`, and `record` is called from the editor's
 * caret callback — which runs *inside* the goto / value `$effect`s (a programmatic scroll or a
 * controlled value-replace fires CodeMirror's update listener synchronously). Without `untrack`,
 * those reads register as dependencies of the calling effect and the writes invalidate them →
 * the effect re-runs forever (`effect_update_depth_exceeded`). Every mutator's RMW is therefore
 * `untrack`ed; the getters (`canBack`/`canForward`) stay tracked so the toolbar buttons update.
 */

import { untrack } from 'svelte';

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
     *  exactly like a browser). Untracked: this runs inside the editor's caret `$effect`,
     *  and its read-modify-write of `places`/`index` must not make those a dependency of
     *  the calling effect (that self-invalidation is the freeze regression). */
    record(place: NavPlace) {
      untrack(() => {
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
      });
    },

    /** Step back one place, or `null` when already at the oldest. */
    back(): NavPlace | null {
      return untrack(() => {
        if (index <= 0) return null;
        index -= 1;
        return places[index];
      });
    },

    /** Step forward one place, or `null` when already at the newest. */
    forward(): NavPlace | null {
      return untrack(() => {
        if (index >= places.length - 1) return null;
        index += 1;
        return places[index];
      });
    },

    /** Drop the whole history (e.g. on project switch). */
    reset() {
      untrack(() => {
        places = [];
        index = -1;
      });
    },
  };
}

export const bennuNavStore = createBennuNavStore();
