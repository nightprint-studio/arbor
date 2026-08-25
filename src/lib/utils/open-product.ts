/**
 * The single way to launch a Canopy product — window or tab.
 *
 * Every entry point (the launcher tiles, the Command Palette, deep links)
 * funnels through here so the user's `window_mode` is honoured in one place
 * instead of at each call site. In `windows` mode this is exactly what the
 * launcher did before; in `tabbed` mode the workspace products land as tabs in
 * the container and everything else still opens its own window.
 */
import {
  PRODUCT_WINDOW_OPENERS, closeProductWindow, listRunningProducts, openLauncherWindow,
} from '$lib/ipc/app';
import { openWorkspaceWindow } from '$lib/ipc/window';
import { setOpenIntent } from '$lib/ipc/recents';
import { windowModeStore } from '$lib/stores/window-mode.svelte';
import { SURFACES, surfaceStore, type SurfaceId } from '$lib/stores/surfaces.svelte';

/** Products the container can host — the workspace surfaces, minus the home
 *  tab. Sitta is freely multi-instance and Tyto belongs in the tray, so both
 *  keep their own windows in either mode. */
const TABBABLE = new Set(SURFACES.map((s) => s.id).filter((id) => id !== 'home'));

/** True when `id` would open as a tab under the current setting. */
export function opensAsTab(id: string): boolean {
  return windowModeStore.tabbed && TABBABLE.has(id as never);
}

/**
 * Open (or focus) a product, honouring the window mode.
 *
 * The tabbed path is best-effort: if the container can't be opened for any
 * reason, the product still opens in its own window. Launching a product is the
 * one action that must never fail — a broken container would otherwise leave
 * the user with an app that opens nothing at all.
 */
export async function openProduct(id: string): Promise<void> {
  const opener = PRODUCT_WINDOW_OPENERS[id];
  try {
    await windowModeStore.ensure();
    if (opensAsTab(id)) {
      await openWorkspaceWindow(id);
      return;
    }
  } catch (e) {
    console.error(`openProduct(${id}): tabbed path failed, falling back to a window`, e);
  }
  await opener?.();
}

/** How long {@link restartProduct} waits for the old surfaces (and their backend) to be gone. */
const STOP_WAIT_MS = 5000;
/** How often it asks. The shell answers from memory — this is cheap. */
const STOP_POLL_MS = 120;

/**
 * Stop a product and start it again — the way to pick up a rebuilt backend without restarting
 * Arbor itself.
 *
 * Here rather than in either home, for the same reason {@link openProduct} is: Arbor has two of
 * them (the Canopy launcher window and the container's welcome tab) and they must mean the same
 * thing by Restart.
 *
 * The wait in the middle is the whole verb. A product's headless backend is torn down when its
 * last surface goes, and that teardown is asynchronous — for a tab it even happens in another
 * window. Launching straight after the stop races it, and the race is one the NEW backend loses:
 * it gets attached and then detached by the old one's teardown, leaving a window whose every call
 * answers "unknown method". So this waits for the shell to agree the product is gone — and gives
 * up waiting rather than hanging, because a Restart that never completes is worse than one that
 * goes ahead.
 */
export async function restartProduct(id: string): Promise<void> {
  await closeProductWindow(id).catch(() => {});
  const until = Date.now() + STOP_WAIT_MS;
  for (;;) {
    let running: string[];
    try { running = await listRunningProducts(); } catch { break; }
    if (!running.includes(id) || Date.now() >= until) break;
    await new Promise((done) => setTimeout(done, STOP_POLL_MS));
  }
  await openProduct(id);
}

/**
 * Open a product **on a specific project** — the recents click.
 *
 * The path is parked first and pulled by the product's shell as it mounts:
 * window openers take no arguments, and the shell boots long after the click.
 * If that product is already open the shell reacts to the same intent through
 * its focus path, so a second click on a recent still lands on the project.
 */
export async function openProjectIn(product: string, path: string): Promise<void> {
  await setOpenIntent(product, path);
  await openProduct(product);
}

/**
 * Pull a tab out of the container into its own window — the browser gesture,
 * for a second monitor or simply more room. The tab closes once the window is
 * on its way; the product's state is per-window, so this is a fresh start of
 * that product, not a move of the live session.
 */
export async function detachSurface(id: SurfaceId): Promise<void> {
  if (id === 'home') await openLauncherWindow();
  else await PRODUCT_WINDOW_OPENERS[id]?.();
  void surfaceStore.close(id);
}
