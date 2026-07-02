/**
 * Bennu "intentions" store — drives the Alt+Enter context-action popup.
 *
 * Mirrors merula's `stores/intentions.svelte.ts`: the editor host collects the
 * actions applicable at the caret (`collectIntentions`) and opens a floating
 * picker anchored there via `openWith(items, anchor)`. `BennuIntentionsOverlay`
 * renders off this state and calls the picked item's `run()` directly, so unlike
 * merula there is no `pending`/`seq` relay — the action is self-contained.
 *
 * Window-local session UI state (no persistence needed). Rune-store pattern:
 * private `$state`, returned getters + methods (CLAUDE.md · "Store pattern").
 */

import type { IntentionItem } from '$lib/components/bennu/bennu-intentions';

/** Caret-anchored popup position, in viewport coords (from `coordsAtPos`). */
export interface IntentionAnchor {
  x: number;
  y: number;
}

function createBennuIntentionsStore() {
  let open = $state(false);
  let items = $state<IntentionItem[]>([]);
  let anchor = $state<IntentionAnchor | null>(null);

  return {
    get open() { return open; },
    get items() { return items; },
    get anchor() { return anchor; },

    /** Open the popup with the actions for the current caret, anchored at
     *  `nextAnchor` (viewport coords). Passing an empty list is a no-op — the
     *  host should toast "No context actions here" instead of opening empty. */
    openWith(nextItems: IntentionItem[], nextAnchor: IntentionAnchor | null) {
      if (!nextItems.length) return;
      items = nextItems;
      anchor = nextAnchor;
      open = true;
    },

    /** Close the popup (Esc / outside-click / after running an action). */
    close() {
      open = false;
    },
  };
}

export const bennuIntentionsStore = createBennuIntentionsStore();
