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

import type { MenuAction, MenuItem } from '$lib/components/shared/ContextMenu.svelte';

export function createContextMenuStore() {
  let open = $state(false);
  let x = $state(0);
  let y = $state(0);
  let items = $state<MenuItem[]>([]);
  let above = $state(false);
  let actions = $state<MenuAction[] | undefined>(undefined);
  // The pick handler for the currently-open menu (not reactive — read on select).
  let handler: (id: string) => void = () => {};

  return {
    get open() { return open; },
    get x() { return x; },
    get y() { return y; },
    get items() { return items; },
    /** Whether the open menu sits above its anchor rather than below. */
    get above() { return above; },
    /** The icon bar pinned to the top of the open menu (the few verbs everyone reaches for), if any. */
    get actions() { return actions; },

    /**
     * Open the menu at viewport coords `x`/`y` with `menuItems`; `onSelect(id)`
     * fires when the user picks one. Empty `menuItems` is a no-op — a menu with
     * nothing in it is worse than no menu, because it eats the right-click.
     */
    show(nx: number, ny: number, menuItems: MenuItem[], onSelect: (id: string) => void, opts?: { above?: boolean; actions?: MenuAction[] }) {
      if (!menuItems.length && !opts?.actions?.length) return;
      x = nx;
      y = ny;
      above = !!opts?.above;
      actions = opts?.actions;
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
