/**
 * Bennu context-menu store — a single shared right-click menu for the window.
 *
 * Any surface (editor, project tree, …) raises the menu with `show(x, y, items,
 * onSelect)`; BennuWindow mounts one `ContextMenu` bound to this state and routes the
 * pick back through `select(id)`. Keeps the menu logic (which items, what they do) at
 * the call site while the chrome is mounted once.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';

function createBennuContextMenuStore() {
  let open = $state(false);
  let x = $state(0);
  let y = $state(0);
  let items = $state<MenuItem[]>([]);
  // The pick handler for the currently-open menu (not reactive — read on select).
  let handler: (id: string) => void = () => {};

  return {
    get open() { return open; },
    get x() { return x; },
    get y() { return y; },
    get items() { return items; },

    /** Open the menu at viewport coords `x`/`y` with `menuItems`; `onSelect(id)` fires
     *  when the user picks an item. Empty `menuItems` is a no-op. */
    show(nx: number, ny: number, menuItems: MenuItem[], onSelect: (id: string) => void) {
      if (!menuItems.length) return;
      x = nx;
      y = ny;
      items = menuItems;
      handler = onSelect;
      open = true;
    },
    /** ContextMenu → an item was picked: close, then run the handler. */
    select(id: string) {
      open = false;
      handler(id);
    },
    close() {
      open = false;
    },
  };
}

export const bennuContextMenuStore = createBennuContextMenuStore();
