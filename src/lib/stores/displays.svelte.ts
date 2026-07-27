import { listDisplays, type DisplayInfo } from '$lib/utils/window-tiling';

/**
 * The monitors this window can be moved to.
 *
 * The list is async (a platform call) but every consumer — the Window ▸ Move &
 * Resize menu, the zoom panel's display switcher — is built synchronously
 * inside a `$derived`, so it lives here as reactive state instead: read
 * `displaysStore.list`, and it fills in (and re-renders the menu) on its own.
 *
 * Refreshed once at startup and again whenever a surface that shows displays
 * opens, since monitors come and go with docks and cables.
 */
function createDisplaysStore() {
  let list = $state<DisplayInfo[]>([]);
  let inFlight = false;

  async function refresh() {
    if (inFlight) return;
    inFlight = true;
    try {
      list = await listDisplays();
    } catch {
      // Non-Tauri context (tests, SSR) — leave the list empty; every consumer
      // treats "fewer than two displays" as "no switcher".
    } finally {
      inFlight = false;
    }
  }

  void refresh();

  return {
    get list() { return list; },
    /** True once there is somewhere else to move the window to. */
    get hasMultiple() { return list.length > 1; },
    refresh,
  };
}

export const displaysStore = createDisplaysStore();
