/**
 * Ordering and visibility for an icon rail, as pure functions.
 *
 * Two products let you rearrange their activity bar, and both of them face the same three
 * awkward facts:
 *
 *  1. **The saved order is older than the item list.** A rail is built fresh from what the
 *     project offers right now — a Cargo root has no Java tools, a plugin was uninstalled —
 *     so a saved order always names things that are not there, and the item list always
 *     contains things the saved order has never heard of.
 *  2. **A new item must appear.** Anything the saved order does not mention is shown, visible,
 *     in the place the default order gives it relative to its neighbours. The alternative is a
 *     feature that ships invisible to everybody who ever opened the customise dialog, which is
 *     the worst possible audience to hide it from.
 *  3. **Some items cannot be hidden.** A rail whose every button can be turned off has a
 *     state in which nothing can be turned back on.
 *
 * Keeping this here rather than in either product's store is what makes the two bars behave
 * the same way — including in the corners, which is where "the same way" actually gets
 * decided.
 */

import type { ActivityBarItemConfig } from '$lib/types/config';

/** Anything that can sit on a rail: an id is all this module needs. */
export interface Identified {
  id: string;
}

/** An item as the customise dialog edits it — the rail item plus its visibility. */
export type RailEditorItem<T extends Identified> = T & {
  visible: boolean;
  /** Always visible, and not draggable: hiding it would strand something. */
  mandatory: boolean;
};

/** An item with whether the saved arrangement shows it. */
interface Placed<T> {
  item: T;
  visible: boolean;
}

/**
 * Every offered item, in the saved order, the ones the saved order has never heard of slipped in
 * where the default order puts them.
 *
 * "Where the default order puts them" means: right after the item that precedes them in `items`
 * (hidden or not — a hidden neighbour still marks the place), or, with nothing before them placed,
 * right before the first one that follows. A saved arrangement is a preference about the buttons
 * that existed when it was made; it says nothing on where a later one belongs, and the default
 * order does. Appending at the end instead is what made a new build tool land after Imber.
 */
function placeWithDefaults<T extends Identified>(
  items: T[],
  saved: ActivityBarItemConfig[] | undefined,
): Placed<T>[] {
  const byId = new Map(items.map((i) => [i.id, i]));
  const out: Placed<T>[] = [];
  const placed = new Set<string>();
  for (const s of saved ?? []) {
    const item = byId.get(s.id);
    if (!item || placed.has(s.id)) continue; // saved but not offered by this project — skipped
    placed.add(s.id);
    out.push({ item, visible: s.visible });
  }
  items.forEach((item, i) => {
    if (placed.has(item.id)) return;
    const at = (id: string) => out.findIndex((p) => p.item.id === id);
    let index = -1;
    for (let j = i - 1; j >= 0 && index < 0; j--) {
      const found = at(items[j].id);
      if (found >= 0) index = found + 1;
    }
    if (index < 0) {
      for (let j = i + 1; j < items.length && index < 0; j++) {
        const found = at(items[j].id);
        if (found >= 0) index = found;
      }
    }
    out.splice(index < 0 ? out.length : index, 0, { item, visible: true });
    placed.add(item.id);
  });
  return out;
}

/**
 * The rail as it should be drawn: `items` in the saved order, without the hidden ones.
 *
 * Items the saved order does not mention take the place the default order gives them, relative to
 * the items around them (see {@link placeWithDefaults}); without a saved order the default order is
 * the rail.
 */
export function applyRailOrder<T extends Identified>(
  items: T[],
  saved: ActivityBarItemConfig[] | undefined,
  mandatory: ReadonlySet<string> = new Set(),
): T[] {
  if (!saved || saved.length === 0) return items;
  return placeWithDefaults(items, saved)
    .filter((p) => p.visible || mandatory.has(p.item.id))
    .map((p) => p.item);
}

/**
 * Every item, in the saved order, each carrying its visibility — what the customise dialog
 * edits.
 *
 * The difference from {@link applyRailOrder} is that nothing is dropped: a hidden item still
 * needs a row, or there would be no way to bring it back.
 */
export function mergeRailOrder<T extends Identified>(
  items: T[],
  saved: ActivityBarItemConfig[] | undefined,
  mandatory: ReadonlySet<string> = new Set(),
): RailEditorItem<T>[] {
  return placeWithDefaults(items, saved).map(({ item, visible }) => ({
    ...item,
    visible: mandatory.has(item.id) ? true : visible,
    mandatory: mandatory.has(item.id),
  }));
}

/**
 * What to persist for a section, merged with what was already there.
 *
 * `previous` matters: the dialog only ever sees the items the current project offers, and
 * saving just those would forget the arrangement of every tool that happens to be absent
 * today — switch back to a Maven project and the Java rail is in default order again. So
 * entries for ids the dialog did not show are carried through, after the ones it did.
 */
export function railOrderToConfig<T extends Identified>(
  edited: RailEditorItem<T>[],
  previous: ActivityBarItemConfig[] | undefined,
  mandatory: ReadonlySet<string> = new Set(),
): ActivityBarItemConfig[] {
  const shown = new Set(edited.map((i) => i.id));
  const out: ActivityBarItemConfig[] = edited.map((i) => ({
    id: i.id,
    visible: mandatory.has(i.id) ? true : i.visible,
  }));
  for (const p of previous ?? []) {
    if (!shown.has(p.id)) out.push({ ...p });
  }
  return out;
}
