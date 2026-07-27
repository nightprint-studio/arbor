/**
 * Open-windows directory — the reactive view of every Arbor window.
 *
 * Arbor drives several top-level windows (Canopy, Corvus, Bennu, Merula, the
 * File Explorer, Tyto) and the OS gives no uniform way to move between them:
 * Windows has a taskbar button per window, macOS has none at all. This store
 * backs the in-app answer — the window switcher and the title bar's Window
 * menu — from a single query plus the shell's `windows-changed` broadcast.
 *
 * Cross-product on purpose: every window mounts it (see `+page.svelte`), so it
 * lives at the root of `stores/`, not under a product folder.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  listWindows, focusWindow, WINDOWS_CHANGED_EVENT, type ArborWindow,
} from '$lib/ipc/window';

function createWindowsStore() {
  let list = $state<ArborWindow[]>([]);
  let loading = $state(false);
  // The switcher overlay's open state lives here, not in the component, so any
  // surface can raise it — the keybinding, the title bar's Window menu, a
  // command palette entry — without threading a component ref around.
  let switcherOpen = $state(false);

  // The label of the window this store instance runs in — every window has its
  // own JS realm, so "self" is fixed for the lifetime of the store.
  let selfLabel = '';
  try { selfLabel = getCurrentWindow().label; } catch { /* non-Tauri / SSR */ }

  // One event listener shared by every consumer in this window, ref-counted:
  // the switcher and the Window menu both watch, and whichever unmounts last
  // tears the listener down.
  let watchers = 0;
  let unlisten: UnlistenFn | null = null;

  async function refresh(): Promise<void> {
    loading = true;
    try {
      list = await listWindows();
    } catch {
      // A failed listing is never worth a toast — the switcher just shows what
      // it last knew (or nothing, on the very first call).
    } finally {
      loading = false;
    }
  }

  return {
    get list() { return list; },
    /** Every window except the one we are running in — what a switcher offers. */
    get others() { return list.filter((w) => w.label !== selfLabel); },
    get selfLabel() { return selfLabel; },
    get loading() { return loading; },

    get switcherOpen() { return switcherOpen; },
    openSwitcher()   { switcherOpen = true; },
    closeSwitcher()  { switcherOpen = false; },
    toggleSwitcher() { switcherOpen = !switcherOpen; },

    refresh,

    /**
     * Keep the list live until the returned disposer runs. Safe to call from
     * several components at once — the underlying listener is shared.
     */
    watch(): () => void {
      watchers += 1;
      void refresh();
      if (watchers === 1) {
        void listen(WINDOWS_CHANGED_EVENT, () => void refresh())
          .then((un) => {
            // A disposer that already ran while we were awaiting wins: drop the
            // listener immediately rather than leaking it.
            if (watchers > 0) unlisten = un;
            else un();
          })
          .catch(() => { /* non-Tauri */ });
      }
      let disposed = false;
      return () => {
        if (disposed) return;
        disposed = true;
        watchers -= 1;
        if (watchers === 0) {
          unlisten?.();
          unlisten = null;
        }
      };
    },

    /** Bring another window to the front (unhides a tray'd one). */
    async focus(label: string): Promise<void> {
      try {
        await focusWindow(label);
      } catch { /* window went away between listing and click */ }
    },
  };
}

export const windowsStore = createWindowsStore();
