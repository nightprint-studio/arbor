/**
 * Arrow-key movement between rows of a list, in the DOM.
 *
 * The shared `Tree` deliberately leaves `ArrowUp` / `ArrowDown` to the browser's
 * tab order ("so we don't fight focus traps in modal hosts" — `Tree.svelte`), and
 * a note sidebar without them is not keyboard-usable. It is done in the DOM
 * rather than by index because the tree is virtualised: the row above and the row
 * below are always mounted (the widget keeps an overscan), but the row two
 * screens down is not, so there is nothing to focus by index anyway.
 *
 * Shared by the tree and by the pinned / recent lists so the two behave
 * identically — the user should not be able to tell which widget they are in.
 */

/** Rows of `container`, in visual order. */
function rowsOf(container: HTMLElement, selector: string): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(selector));
}

/**
 * Move focus `delta` rows from `current`.
 *
 * Returns `false` when the move ran off the top (the caller usually wants to hand
 * focus back to the filter box) or off the bottom (where it does nothing).
 */
export function moveRowFocus(
  container: HTMLElement | undefined | null,
  current: HTMLElement | null,
  delta: number,
  selector = '.tree-row',
): boolean {
  if (!container) return false;
  const rows = rowsOf(container, selector);
  if (rows.length === 0) return false;
  const index = current ? rows.indexOf(current) : -1;
  const next = index < 0 ? (delta > 0 ? 0 : rows.length - 1) : index + delta;
  if (next < 0 || next >= rows.length) return false;
  rows[next].focus();
  return true;
}

/** Focus the first row, if there is one. */
export function focusFirstRow(
  container: HTMLElement | undefined | null,
  selector = '.tree-row',
): boolean {
  if (!container) return false;
  const first = container.querySelector<HTMLElement>(selector);
  if (!first) return false;
  first.focus();
  return true;
}
