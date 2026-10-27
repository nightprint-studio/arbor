/**
 * Pointer-driven drag and drop for the workspace board.
 *
 * Built on pointer events, not the HTML5 drag-and-drop API: native DnD does not work in WebView2
 * (the drag starts and never delivers `drop`), which is why the project tree's drag is mouse-driven
 * too. The cost of doing it by hand is small here — a threshold, a ghost that follows the pointer,
 * and a hit test — and it buys what native DnD never gives: the drop *position* (an insertion line
 * between two rows) rather than only "dropped on this element".
 *
 * Two kinds of thing are dragged, and they drop on different targets:
 *
 * - a **project** onto any workspace column, at the gap between two of its rows;
 * - a **workspace** onto the board, at the gap between two columns.
 *
 * The markup opts in with data attributes — `data-board` on the container, `data-ws-col="<id>"` on a
 * column, `data-proj-row` on a project row — and the controller only reads their rectangles, so the
 * component stays in charge of how things look.
 *
 * Esc cancels. A drag that never passed the threshold is a click and is left alone; one that did
 * suppresses the click that follows it, so dropping a row does not also select it.
 */

export type DragItem =
  | { kind: 'project'; wsId: string; root: string; label: string }
  | { kind: 'workspace'; wsId: string; label: string };

/** Where it would land. `index` is a gap in the list *as it is now*, dragged item included. */
export type DropTarget =
  | { kind: 'project'; wsId: string; index: number }
  | { kind: 'workspace'; index: number };

/** Pixels the pointer must travel before a press becomes a drag. */
const THRESHOLD = 5;

export function createBoardDrag(onDrop: (item: DragItem, target: DropTarget) => void) {
  let item = $state<DragItem | null>(null);
  let active = $state(false);
  let x = $state(0);
  let y = $state(0);
  let target = $state<DropTarget | null>(null);
  let startX = 0;
  let startY = 0;
  let justDragged = false;

  /** Index of the first element whose midpoint is after `at` along an axis, else `elements.length`. */
  function gapIndex(elements: HTMLElement[], at: number, axis: 'x' | 'y'): number {
    for (let i = 0; i < elements.length; i++) {
      const r = elements[i].getBoundingClientRect();
      const mid = axis === 'x' ? r.left + r.width / 2 : r.top + r.height / 2;
      if (at < mid) return i;
    }
    return elements.length;
  }

  function hit(e: PointerEvent): DropTarget | null {
    if (!item) return null;
    const under = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    if (!under) return null;

    if (item.kind === 'workspace') {
      const board = under.closest<HTMLElement>('[data-board]');
      if (!board) return null;
      const cols = [...board.querySelectorAll<HTMLElement>('[data-ws-col]')];
      return { kind: 'workspace', index: gapIndex(cols, e.clientX, 'x') };
    }

    const col = under.closest<HTMLElement>('[data-ws-col]');
    if (!col) return null;
    const rows = [...col.querySelectorAll<HTMLElement>('[data-proj-row]')];
    return { kind: 'project', wsId: col.dataset.wsCol!, index: gapIndex(rows, e.clientY, 'y') };
  }

  function onMove(e: PointerEvent) {
    x = e.clientX;
    y = e.clientY;
    if (!active && Math.hypot(x - startX, y - startY) > THRESHOLD) active = true;
    if (active) target = hit(e);
  }

  function onUp() {
    const dropped = active && item && target ? { item, target } : null;
    if (active) {
      // The click that follows a drag belongs to the drag.
      justDragged = true;
      setTimeout(() => (justDragged = false), 0);
    }
    stop();
    if (dropped) onDrop(dropped.item, dropped.target);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !item) return;
    e.preventDefault();
    e.stopPropagation(); // the first Esc ends the drag; the modal stays
    stop();
  }

  function stop() {
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointercancel', stop);
    window.removeEventListener('keydown', onKey, true);
    item = null;
    active = false;
    target = null;
  }

  return {
    get item() { return item; },
    get active() { return active; },
    get x() { return x; },
    get y() { return y; },
    get target() { return target; },
    /** True for the click event right after a drag ended — handlers ignore it. */
    get justDragged() { return justDragged; },

    /** Press on a draggable. Nothing is dragged until the pointer travels past the threshold. */
    begin(e: PointerEvent, dragged: DragItem) {
      if (e.button !== 0) return;
      item = dragged;
      active = false;
      target = null;
      startX = x = e.clientX;
      startY = y = e.clientY;
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', stop);
      window.addEventListener('keydown', onKey, true);
    },

    /** For teardown (the modal closing mid-drag). */
    cancel: stop,
  };
}
