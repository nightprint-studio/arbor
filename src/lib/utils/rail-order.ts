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
 *  2. **A new item must appear.** Anything the saved order does not mention goes at the end,
 *     visible. The alternative is a feature that ships invisible to everybody who ever
 *     opened the customise dialog, which is the worst possible audience to hide it from.
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

/**
 * The rail as it should be drawn: `items` in the saved order, without the hidden ones.
 *
 * Items the saved order does not mention keep their natural position **relative to each
 * other** and land at the end — the ordering is a preference expressed about the items that
 * existed when it was expressed, and it says nothing about the others.
 */
export function applyRailOrder<T extends Identified>(
  items: T[],
  saved: ActivityBarItemConfig[] | undefined,
  mandatory: ReadonlySet<string> = new Set(),
): T[] {
  if (!saved || saved.length === 0) return items;
  const byId = new Map(items.map((i) => [i.id, i]));
  const out: T[] = [];
  const placed = new Set<string>();
  for (const s of saved) {
    const item = byId.get(s.id);
    if (!item) continue; // saved but not offered by this project — silently skipped
    placed.add(s.id);
    if (s.visible || mandatory.has(s.id)) out.push(item);
  }
  for (const item of items) {
    if (!placed.has(item.id)) out.push(item);
  }
  return out;
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
  const decorate = (item: T, visible: boolean): RailEditorItem<T> => ({
    ...item,
    visible: mandatory.has(item.id) ? true : visible,
    mandatory: mandatory.has(item.id),
  });
  if (!saved || saved.length === 0) return items.map((i) => decorate(i, true));

  const byId = new Map(items.map((i) => [i.id, i]));
  const out: RailEditorItem<T>[] = [];
  const placed = new Set<string>();
  for (const s of saved) {
    const item = byId.get(s.id);
    if (!item) continue;
    placed.add(s.id);
    out.push(decorate(item, s.visible));
  }
  for (const item of items) {
    if (!placed.has(item.id)) out.push(decorate(item, true));
  }
  return out;
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
