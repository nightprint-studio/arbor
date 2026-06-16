/**
 * nemus "intentions" store — drives the Alt+Enter context-action popup.
 *
 * The editor (`TabbedEditor.showIntentions`) collects the actions applicable at
 * the caret/selection and opens a floating picker anchored there. Selecting one
 * stages it as `pending` (+ bumps `pendingSeq`); the editor relay applies it —
 * either dispatching its precomputed edits or re-opening the rename/extract input
 * flow. Window-local UI state, rune-store pattern.
 */

import type { IntentionItem } from '../editor/nemus-intentions';
import type { UsageAnchor } from './usages.svelte';

function createIntentionsStore() {
  let open    = $state(false);
  let items   = $state<IntentionItem[]>([]);
  let anchor  = $state<UsageAnchor | null>(null);
  let pending = $state<IntentionItem | null>(null);
  let pendingSeq = $state(0);

  return {
    get open()    { return open; },
    get items()   { return items; },
    get anchor()  { return anchor; },
    /** The action the user picked (consumed by the editor relay). */
    get pending() { return pending; },
    get pendingSeq() { return pendingSeq; },

    /** Open the popup with the actions for the current caret. */
    openWith(nextItems: IntentionItem[], nextAnchor: UsageAnchor | null) {
      items = nextItems;
      anchor = nextAnchor;
      open = true;
    },
    /** Stage the chosen action and close (the editor relay applies it). */
    choose(item: IntentionItem) {
      pending = item;
      pendingSeq++;
      open = false;
    },
    close() { open = false; },
  };
}

export const intentionsStore = createIntentionsStore();
