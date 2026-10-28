/**
 * A tab being dragged out of its strip, as the rest of the screen needs to know it.
 *
 * The strip owns the drag; what it cannot do alone is tell the places a tab can be DROPPED that one is
 * coming. Those places — the editor panes, and the edge of a lone editor that a tab dropped there
 * splits off — mark themselves with `data-tab-drop="<zone>"`. The strip hit-tests them under the pointer
 * and reports which is `hot`; they light up from it, and the editor offers its split zone only while
 * `active`.
 */
function createTabDragStore() {
  let active = $state(false);
  let hot = $state<string | null>(null);
  return {
    /** A tab is being held outside its own strip's reach — worth showing the places it can land. */
    get active() { return active; },
    /** The zone the pointer is over, if it is one the tab can be dropped on. */
    get hot() { return hot; },
    begin() { active = true; },
    setHot(zone: string | null) { if (hot !== zone) hot = zone; },
    end() { active = false; hot = null; },
  };
}

export const tabDragStore = createTabDragStore();
