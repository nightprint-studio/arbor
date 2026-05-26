import type { Component } from 'svelte';
import { appearanceStore } from '$lib/stores/appearance.svelte';

/**
 * One entry in the parked-modals dock.
 *
 * Unlike the original "modal stays mounted, display:none" approach, parked
 * entries are now ACTION RECORDS: the modal is actually closed at minimize
 * time, and `execute` knows how to re-open it from scratch (switching to
 * the right tab first, then dispatching the open-detail flow). This is
 * what makes the entries survive workspace / tab switches — the modal
 * lifecycle is decoupled from the chip lifecycle.
 *
 * Tradeoff: state local to the modal (scroll position, unsubmitted
 * comments, fetched detail data) is lost across minimize → restore.
 * Workflow continuity is preserved; ephemeral input is not.
 */
export interface ParkedModalEntry {
  id:       string;
  title:    string;
  /** Optional Lucide icon component shown to the left of the title. */
  icon?:    Component<{ size?: number; class?: string }>;
  /** Re-open the modal from scratch. May switch tabs / open closed
   *  projects from the registry; returns a Promise so the chip can show
   *  a spinner while async work runs. May throw — the dock catches and
   *  toasts. */
  execute:  () => void | Promise<void>;
}

function createParkedModalsStore() {
  let entries = $state<ParkedModalEntry[]>([]);

  return {
    get entries() { return entries; },
    get count()   { return entries.length; },

    /** Attempt to park a modal. Returns `false` when the user-configured
     *  cap has been reached and the entry was rejected — the caller is
     *  expected to show a toast and leave the modal open. Re-parking
     *  an existing id (same modal minimized twice) always succeeds. */
    park(entry: ParkedModalEntry): boolean {
      const alreadyParked = entries.some(e => e.id === entry.id);
      if (!alreadyParked && entries.length >= appearanceStore.parkedModalsMax) {
        return false;
      }
      // Replace any prior entry with the same id so a re-park after a
      // remount doesn't leave a duplicate chip behind. Push to the end so
      // newly-parked items appear on the right (LRU at the head).
      entries = [...entries.filter(e => e.id !== entry.id), entry];
      return true;
    },

    /** Drop an entry without running its action. Used by the chip's ✕
     *  button and by the restore path (after `execute` succeeds, the
     *  modal is up — the chip can go). */
    unpark(id: string) {
      entries = entries.filter(e => e.id !== id);
    },
  };
}

export const parkedModalsStore = createParkedModalsStore();
