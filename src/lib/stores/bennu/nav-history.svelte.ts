/**
 * Bennu navigation history — the reactive face of {@link NavHistory}.
 *
 * All the rules (what is a stop, merging, truncation, mapping through edits, forgetting files that
 * are gone) live in the pure, unit-tested `nav-ring.ts`. This file only makes them observable: a
 * `version` rune bumped after every mutation, which the getters read so the palette entries and the
 * Recent Locations popup update.
 *
 * Rune store (CLAUDE.md): private `$state`, exposed via getters + methods. Session state — the
 * history dies with the window, like IntelliJ's does with the project.
 *
 * Mutators bump `version` inside `untrack`: some callers run inside an `$effect` (a programmatic
 * scroll fires CodeMirror's update listener synchronously), and a tracked read-modify-write there
 * re-runs the effect for ever (`effect_update_depth_exceeded`).
 */

import { untrack } from 'svelte';
import {
  NavHistory, type LineEdit, type NavPlace, type RecentPlace,
} from '$lib/components/bennu/nav-ring';
import { isSamePath } from '$lib/utils/paths';

export type { LineEdit, NavPlace, RecentPlace };

function createBennuNavStore() {
  const history = new NavHistory((a, b) => isSamePath(a, b));
  let version = $state(0);

  function mutate<T>(change: () => T): T {
    return untrack(() => {
      const result = change();
      version += 1;
      return result;
    });
  }

  return {
    get canBack() { void version; return history.canBack; },
    get canForward() { void version; return history.canForward; },
    get hasEdits() { void version; return history.hasEdits; },
    get recent(): RecentPlace[] { void version; return [...history.recent]; },

    /** A navigation is leaving `origin` — record it as a stop, truncating Forward. */
    leave(origin: NavPlace) { mutate(() => history.leave(origin)); },
    /** A navigation landed at `place` — a visit for Recent Locations, not a stop. */
    arrive(place: NavPlace) { mutate(() => history.arrive(place)); },
    /** Step back from `current`; records nothing. */
    back(current: NavPlace | null): NavPlace | null { return mutate(() => history.back(current)); },
    /** Step forward from `current`; records nothing. */
    forward(current: NavPlace | null): NavPlace | null { return mutate(() => history.forward(current)); },

    /** The document was edited at `place` (merged by region). */
    noteEdit(place: NavPlace) { mutate(() => history.noteEdit(place)); },
    /** The next place back through the edit history. */
    stepEdit(): NavPlace | null { return mutate(() => history.stepEdit()); },

    /** `file` changed — move every remembered line in it with the text. */
    applyEdits(file: string, edits: readonly LineEdit[]) {
      if (edits.length === 0) return;
      mutate(() => history.applyEdits(file, edits));
    },
    /** `file` cannot be opened any more — drop its stops. */
    forget(file: string) { mutate(() => history.forget(file)); },

    /** Drop everything. Called when the window changes project: the paths in here belong to the
     *  project that was open, and stepping back into them would open files from somewhere else. */
    reset() { mutate(() => history.reset()); },
  };
}

export const bennuNavStore = createBennuNavStore();
