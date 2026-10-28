/**
 * The geometry of dragging a tab to a new place, kept apart from the widget so it can be tested.
 *
 * The strip is a **sortable**: the tab being dragged follows the pointer, and the tabs it passes
 * slide aside to open the slot it would land in — so what you see while dragging is the order you
 * will get. Nothing here touches the DOM. A drag starts by measuring every visible tab once
 * ({@link TabSlot}, in layout order); from then on the answer to "where would it land?" is a
 * function of those rectangles and the pointer, and the tabs never need measuring again — their
 * transforms do not move their layout boxes, which is what keeps the measurements valid.
 *
 * Indices are positions in the strip's **item list**, which can be larger than the visible set (the
 * tabs hidden behind the overflow chevron have indices too), so a slot carries its own.
 */

/** One visible tab, as measured when the drag began. */
export interface TabSlot {
  /** Position in the item list. */
  index: number;
  left: number;
  width: number;
}

/**
 * The index the dragged tab would be inserted BEFORE (the item list as it is now, the dragged tab
 * still in it): the first other tab whose centre lies past the dragged tab's centre, or one past the
 * last visible tab when none does. `from + 1` while the tab has not moved past a neighbour's centre —
 * "before the tab after me" is how staying put reads here, because the tab itself is not an
 * "other" — and `{@link landingIndex}` turns that into no move at all.
 */
export function insertBefore(slots: readonly TabSlot[], from: number, draggedCentre: number): number {
  const others = slots.filter((s) => s.index !== from);
  const passed = others.filter((s) => s.left + s.width / 2 < draggedCentre).length;
  if (passed < others.length) return others[passed].index;
  return slots.length ? slots[slots.length - 1].index + 1 : from;
}

/** Which way the tab at `index` slides to make room: `-1` left, `1` right, `0` stays. */
export function shiftDirection(index: number, from: number, insert: number): -1 | 0 | 1 {
  if (index === from) return 0;
  if (insert > from && index > from && index < insert) return -1;
  if (insert < from && index >= insert && index < from) return 1;
  return 0;
}

/** Where the `from` index lands in the list once it is moved: the usual remove-then-insert shift. */
export function landingIndex(from: number, insert: number): number {
  return insert <= from ? insert : insert - 1;
}

/**
 * How far the dragged tab has to travel, from where it started, to sit exactly in the slot it opens:
 * the widths (and the gaps between) of the tabs it jumps over. The tab is animated there on release
 * so the drop is a slide into place, not a jump.
 */
export function settleOffset(slots: readonly TabSlot[], from: number, insert: number, gap: number): number {
  const span = (lo: number, hi: number) =>
    slots.filter((s) => s.index >= lo && s.index < hi).reduce((sum, s) => sum + s.width + gap, 0);
  if (insert > from) return span(from + 1, insert);
  if (insert < from) return -span(insert, from);
  return 0;
}

/** `dx`, held to what keeps the dragged tab inside the strip — between the first and last tab. */
export function clampOffset(slots: readonly TabSlot[], from: number, dx: number): number {
  const me = slots.find((s) => s.index === from);
  if (!me || slots.length === 0) return 0;
  const first = slots[0];
  const last = slots[slots.length - 1];
  const min = first.left - me.left;
  const max = last.left + last.width - (me.left + me.width);
  return Math.min(Math.max(dx, min), max);
}
