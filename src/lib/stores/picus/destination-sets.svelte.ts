/**
 * Named sets of destinations — "where a change like this always goes".
 *
 * The list, and the three things you do to it, live here rather than in the
 * component that first showed them: a set is reachable from the sidebar, from
 * the Destinations card and from the palette, and three copies of "read them,
 * apply one, save these" would be three places for the reload to be forgotten.
 *
 * ## The list is never held across a change to the repository
 *
 * An entry stores a **folder**, and the backend turns that into this release's
 * file and the versions its guard should carry. So the answer depends on the
 * tree: a new release folder changes what every set means. The effect below
 * re-reads on both the root and the tree for that reason — a set showing last
 * release's file would be worse than one showing none.
 */

import { untrack } from 'svelte';

import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  deleteDestinationSet,
  destinationSets,
  saveDestinationSet,
  type ResolvedSet,
} from '$lib/ipc/picus/project';

import { dmlStore } from './dml.svelte';
import { picusProjectStore } from './project.svelte';

function createDestinationSetsStore() {
  let sets = $state<ResolvedSet[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  /** Guards the reload against an older round trip landing last. */
  let seq = 0;

  async function reload(root: string) {
    if (!root) {
      sets = [];
      return;
    }
    const mine = ++seq;
    loading = true;
    try {
      const found = await destinationSets(root);
      if (mine === seq) sets = found;
    } catch {
      // Silent: a repository that cannot be read is already being complained
      // about by the panel that reads it, and a toast per tree change would be
      // noise about something the user did not just do.
      if (mine === seq) sets = [];
    } finally {
      if (mine === seq) loading = false;
    }
  }

  // Window-lifetime singleton, so it owns its effect root rather than borrowing
  // whichever consumer happened to mount first — the sidebar and the card header
  // must not disagree about what exists.
  $effect.root(() => {
    $effect(() => {
      const root = picusProjectStore.root;
      void picusProjectStore.tree;
      untrack(() => void reload(root));
    });
  });

  /** What the current destinations would be saved as — the comparison key. */
  function shape(files: string[]): string {
    return [...files].sort().join('\n');
  }

  return {
    get sets() { return sets; },
    get loading() { return loading; },
    get saving() { return saving; },

    /** Whether a set of that name already exists, matched as the backend does. */
    has(name: string) {
      const wanted = name.trim().toLowerCase();
      return sets.some((s) => s.name.trim().toLowerCase() === wanted);
    },

    /**
     * The set the current destinations came from, if they still match it.
     *
     * Derived rather than remembered on apply: the point is to say "what is
     * armed *is* Release", which stops being true the moment a destination is
     * added or removed — and a flag set on apply would keep claiming it.
     */
    get activeName(): string | null {
      if (!dmlStore.targets.length) return null;
      const current = shape(dmlStore.targets.map((t) => t.file));
      return (
        sets.find(
          (s) => shape(s.destinations.filter((d) => d.file).map((d) => d.file)) === current,
        )?.name ?? null
      );
    },

    /** Re-read from the repository. The tree effect above is the usual trigger. */
    refresh() {
      return reload(picusProjectStore.root);
    },

    /**
     * Arm a set: its destinations **replace** whatever was on screen.
     *
     * Merging would leave whatever was already armed — most dangerously the
     * previous release's update script — in a list nobody chose.
     */
    apply(name: string) {
      const set = sets.find((s) => s.name === name);
      if (!set) return;
      const refused = dmlStore.applyDestinationSet(set.destinations);
      const used = set.destinations.length - refused.length;
      if (!used) {
        toastStore.show(`None of ${name}'s destinations could be used. ${refused[0]?.reason ?? ''}`, 'error');
        return;
      }
      if (refused.length) {
        // One refusal gets its reason; several get their **folders**, because two
        // reasons concatenated are a paragraph in a toast and the first thing the
        // reader needs is which destinations went missing, not why each did. The
        // reasons stay on the set's own row, where they can be read one at a time.
        const detail =
          refused.length === 1
            ? refused[0].reason
            : `Skipped: ${refused.map((r) => r.folder).join(', ')}.`;
        toastStore.show(
          `${name}: ${used} of ${set.destinations.length} armed. ${detail}`,
          'warning',
        );
        return;
      }
      toastStore.show(`${name}: ${used} destination${used === 1 ? '' : 's'} armed.`, 'success');
    },

    /** Save the destinations as they stand, under a name. Answers whether it landed. */
    async save(name: string): Promise<boolean> {
      const root = picusProjectStore.root;
      const trimmed = name.trim();
      if (!trimmed || !root) return false;
      saving = true;
      try {
        await saveDestinationSet(root, dmlStore.captureDestinationSet(trimmed));
        await reload(root);
        // What the backend decided is worth reporting, because it is the one
        // thing about a set the user cannot see and would otherwise discover next
        // release: an update folder whose file names the naming scheme cannot
        // read keeps its file, so the set writes into *that* file for ever.
        const pinned = sets
          .find((s) => s.name.trim().toLowerCase() === trimmed.toLowerCase())
          ?.destinations.filter((d) => d.pinned && d.role === 'update').length;
        toastStore.show(
          pinned
            ? `Saved as ${trimmed}. ${pinned} update destination${pinned === 1 ? '' : 's'} kept a fixed file name — that folder is not named in a way the scheme can follow, so the set will not move to the next release on its own.`
            : `Saved as ${trimmed}, with the repository.`,
          pinned ? 'warning' : 'success',
        );
        return true;
      } catch (e) {
        toastStore.show(`${trimmed} could not be saved — ${e}`, 'error');
        return false;
      } finally {
        saving = false;
      }
    },

    async remove(name: string) {
      const root = picusProjectStore.root;
      if (!root) return;
      try {
        await deleteDestinationSet(root, name);
        await reload(root);
        toastStore.show(`${name} forgotten.`, 'success');
      } catch (e) {
        toastStore.show(`${name} could not be removed — ${e}`, 'error');
      }
    },
  };
}

export const destinationSetsStore = createDestinationSetsStore();
