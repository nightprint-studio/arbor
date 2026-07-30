/**
 * One right-click menu per window — the factory, shared by every product.
 *
 * Any surface (an editor, a tree, a tab strip) raises the menu with
 * `show(x, y, items, onSelect)`; the window mounts a single `ContextMenu` bound to
 * the store and routes the pick back through `select(id)`. Which items exist and
 * what they do stays at the call site; the chrome is mounted once.
 *
 * A **factory**, not a singleton: Arbor is one WebView with a tab per product, so
 * two products can be mounted at the same time and a shared instance would let one
 * window's right-click open in another's. Each product owns an instance.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';

export function createContextMenuStore() {
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

    /**
     * Open the menu at viewport coords `x`/`y` with `menuItems`; `onSelect(id)`
     * fires when the user picks one. Empty `menuItems` is a no-op — a menu with
     * nothing in it is worse than no menu, because it eats the right-click.
     */
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
