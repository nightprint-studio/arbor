/**
 * Bennu navigation history — IntelliJ's back / forward jump stack, plus the two things that
 * make it usable on a real codebase: a separate history of the places you *edited*, and the
 * list of recent places behind Recent Locations.
 *
 * ## What counts as a stop
 *
 * A stop is recorded when an **action** navigates — go to declaration, a usage, a structure or
 * find hit, a diagnostic, a tab switch. Never when the caret merely moves: arrow keys, a click,
 * page-down and scrolling are reading, not navigation, and while they recorded stops the ring
 * filled with places nobody chose to go to, which is what Back then walked through.
 *
 * Each navigation stacks **two** places: where you were (so the first Back lands exactly where
 * you jumped from, column included) and where you went. The caller supplies the origin, because
 * only the editor knows where the caret was before the buffer changed under it.
 *
 * Rune store (CLAUDE.md): private `$state`, exposed via getters + methods. Session state — the
 * ring dies with the window, like IntelliJ's does with the project.
 *
 * The mutators read-modify-write `places`/`index`, and some callers run inside an `$effect` (a
 * programmatic scroll fires CodeMirror's update listener synchronously). Without `untrack`, those
 * reads register as dependencies of the calling effect and the writes invalidate them → the effect
 * re-runs for ever (`effect_update_depth_exceeded`). Every mutator's RMW is therefore `untrack`ed;
 * the getters stay tracked so the toolbar buttons and the popup update.
 */

import { untrack } from 'svelte';

export interface NavPlace {
  file: string;
  line: number;
  col: number;
}

/** A place, and why it is remembered — the popup shows edits differently from visits. */
export interface RecentPlace extends NavPlace {
  kind: 'visit' | 'edit';
  /** Monotonic, for ordering and for a stable keyed `{#each}`. */
  at: number;
}

/** Cap the ring so a long session can't grow it without bound. */
const MAX_PLACES = 100;
/** How many recent places the popup offers. Beyond this it stops being a list you scan. */
const MAX_RECENT = 40;
/** Two places this close in one file are the same place: a jump that lands on a declaration and
 *  one that lands in its body are not two stops. Wider than the old ±1 because the question is
 *  "did I go somewhere else", and three lines down is not somewhere else. */
const MERGE_LINES = 5;

function createBennuNavStore() {
  let places = $state<NavPlace[]>([]);
  // Points at the current place in `places`; -1 when empty.
  let index = $state(-1);
  // Where edits happened, oldest first. Stepped through separately from the jump ring, because
  // "where was I reading" and "where was I typing" are different questions with different answers.
  let edits = $state<NavPlace[]>([]);
  // How far back through `edits` the last-edit stepper has walked; reset by every new edit.
  let editStep = -1;
  let recent = $state<RecentPlace[]>([]);
  let clock = 0;

  function samePlace(a: NavPlace, b: NavPlace): boolean {
    return a.file === b.file && Math.abs(a.line - b.line) <= MERGE_LINES;
  }

  /** Front of the recent list, collapsing a place that is already there. */
  function touchRecent(place: NavPlace, kind: RecentPlace['kind']) {
    clock += 1;
    const kept = recent.filter((r) => !samePlace(r, place));
    kept.unshift({ ...place, kind, at: clock });
    recent = kept.slice(0, MAX_RECENT);
  }

  return {
    get canBack() { return index > 0; },
    get canForward() { return index < places.length - 1; },
    /** The place the ring is currently sitting on — what a caller compares an origin against. */
    get current(): NavPlace | null { return index >= 0 ? places[index] : null; },
    get recent(): RecentPlace[] { return recent; },
    get hasEdits() { return edits.length > 0; },

    /** Record a place the user navigated to (or away from).
     *
     *  Collapses into the current entry when it is the same place, and truncates any forward
     *  history — navigating after a Back starts a new branch, exactly like a browser. */
    push(place: NavPlace) {
      untrack(() => {
        touchRecent(place, 'visit');
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

    /** Keep the stop we are sitting on pointing at where the caret actually is.
     *
     *  Called on caret movement, and it is what makes leaving a file remember where you were in
     *  it: the entry for the file you are in tracks you, so the moment you jump away it already
     *  says the right line. Nothing is added and nothing is dropped — the ring cannot grow from
     *  reading — and the recent list is left alone, because moving the caret is not visiting
     *  somewhere new.
     *
     *  Seeds the ring when it is empty: the first place a session sees has to be in it, or the
     *  first jump would have nothing to come back to. */
    refine(place: NavPlace) {
      untrack(() => {
        if (index < 0) {
          places = [place];
          index = 0;
          touchRecent(place, 'visit');
          return;
        }
        places[index] = place;
      });
    },

    /** Move the current place without adding one, and count it as a visit.
     *
     *  What makes a cross-file jump ONE stop instead of two. Opening a file and scrolling to the
     *  line you asked for are two events and a single navigation: recording both leaves a stop at
     *  line 1 of a file nobody asked to be at, which is where Back took you. The second event
     *  refines the slot the first opened. */
    replace(place: NavPlace) {
      untrack(() => {
        touchRecent(place, 'visit');
        if (index < 0) {
          places = [place];
          index = 0;
          return;
        }
        places[index] = place;
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

    /** Note that the document was edited at `place`.
     *
     *  Called on every keystroke, so it merges by region: one burst of typing is one edit place,
     *  and the stepper below walks *edits*, not characters. */
    noteEdit(place: NavPlace) {
      untrack(() => {
        editStep = -1;
        const last = edits[edits.length - 1];
        if (last && samePlace(last, place)) {
          edits[edits.length - 1] = place;
          touchRecent(place, 'edit');
          return;
        }
        const next = [...edits, place];
        if (next.length > MAX_PLACES) next.shift();
        edits = next;
        touchRecent(place, 'edit');
      });
    },

    /** The next place back through the edit history — the newest on the first call, the one
     *  before it on the next, so pressing the shortcut repeatedly walks back through the session's
     *  edits. Returns `null` when there are none left. */
    stepEdit(): NavPlace | null {
      return untrack(() => {
        if (editStep + 1 >= edits.length) return null;
        editStep += 1;
        return edits[edits.length - 1 - editStep];
      });
    },

    /** Drop everything. Called when the window changes project: the paths in here belong to the
     *  project that was open, and stepping back into them would open files from somewhere else. */
    reset() {
      untrack(() => {
        places = [];
        index = -1;
        edits = [];
        editStep = -1;
        recent = [];
      });
    },
  };
}

export const bennuNavStore = createBennuNavStore();
