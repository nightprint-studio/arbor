/**
 * Turns a flat list of actions into a menu with submenus.
 *
 * An action may name a `group`; the actions of one group become the children of a single parent item that sits
 * where the first of them was, so the menu reads as a handful of verbs ("Improve ▸", "Translate ▸") instead of
 * a column of near-identical sentences. Inside its group an action may use a shorter label (`child`: "English",
 * not "Translate to English"). A group with one member would be a submenu of one: that member stays a plain
 * item, with its full label. The action lists themselves stay flat — ids and order are what the requests and
 * the tests are built on; only the menu is grouped.
 */

import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';

export interface Groupable {
  id: string;
  label: string;
  /** The submenu this action belongs to; none = a top-level item. */
  group?: string;
  /** The label used inside the group. */
  child?: string;
}

export function groupedMenuItems(actions: readonly Groupable[], icon?: unknown): MenuItem[] {
  const size = new Map<string, number>();
  for (const a of actions) if (a.group) size.set(a.group, (size.get(a.group) ?? 0) + 1);

  const out: MenuItem[] = [];
  const parents = new Map<string, MenuItem>();
  for (const a of actions) {
    const grouped = !!a.group && (size.get(a.group) ?? 0) > 1;
    if (!grouped) {
      out.push({ id: a.id, label: a.label, icon });
      continue;
    }
    let parent = parents.get(a.group!);
    if (!parent) {
      parent = { id: `group:${a.group}`, label: a.group!, icon, children: [] };
      parents.set(a.group!, parent);
      out.push(parent);
    }
    parent.children!.push({ id: a.id, label: a.child ?? a.label });
  }
  return out;
}
