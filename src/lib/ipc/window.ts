/**
 * Cross-window shell commands (DIRECT Tauri commands, not the product rpc bridge).
 */
import { invoke } from '@tauri-apps/api/core';

/**
 * Reveal the current window now that its shell has painted — the app-wide
 * anti-white-flash signal.
 *
 * Every launcher/product window is built HIDDEN by the shell: an opaque WebView2
 * window (a transparent one gets no input on Windows) would otherwise flash its white
 * default page during load. This tells the shell to show + focus the caller's own
 * window once the first frame is up. Fire it after the shell mounts + two frames (so
 * a real frame has painted); it's idempotent and only ever reveals the caller.
 */
export function signalWindowReady(): Promise<void> {
  return invoke('window_ready');
}

// ───────────────────────────────────────────────────────────────────────────
//  Window directory — titles, listing, focus
// ───────────────────────────────────────────────────────────────────────────

/** Behavioural class of a window — mirrors the shell's `SurfaceKind`. */
export type SurfaceKind = 'workspace' | 'utility' | 'ambient' | 'launcher' | 'overlay';

/** One switchable Arbor window, as reported by {@link listWindows}. */
export interface ArborWindow {
  label:    string;
  title:    string;
  product:  string | null;
  kind:     SurfaceKind;
  focused:  boolean;
  /** False for a product hidden by close-to-tray — still listed, still focusable. */
  visible:  boolean;
}

/** Broadcast by the shell whenever a window opens, closes or is retitled. */
export const WINDOWS_CHANGED_EVENT = 'arbor://windows-changed';

/**
 * Publish the calling window's real title — the repository, project or folder
 * it is showing.
 *
 * Every window is built with a static title ("Corvus — Arbor"), which makes
 * three open repositories indistinguishable in the Windows taskbar, in Alt-Tab
 * and in the macOS Window menu. Call this whenever the window's subject
 * changes; a shell can only ever retitle itself.
 */
export function setWindowTitle(title: string): Promise<void> {
  return invoke('set_window_title', { title });
}

/** Every switchable window (overlays excluded), launcher first then by title. */
export function listWindows(): Promise<ArborWindow[]> {
  return invoke('list_windows');
}

/** Bring a window to the front, unhiding it if it was closed to the tray. */
export function focusWindow(label: string): Promise<void> {
  return invoke('focus_window', { label });
}

// ───────────────────────────────────────────────────────────────────────────
//  Tabbed container
// ───────────────────────────────────────────────────────────────────────────

/** Pushed to an already-open container to focus (or open) a product's tab. */
export const WORKSPACE_OPEN_PRODUCT_EVENT = 'workspace://open-product';

/**
 * Open the tabbed container, or focus it if it's already up, optionally landing
 * on `product`'s tab. Used instead of `open_<product>_window` when the user's
 * window mode is `tabbed`.
 */
export function openWorkspaceWindow(product?: string): Promise<void> {
  return invoke('open_workspace_window', { product: product ?? null });
}

/**
 * Pull the "show this product" intent parked by the shell before the container
 * existed. Returns null once consumed — the container calls it once on mount,
 * and listens to {@link WORKSPACE_OPEN_PRODUCT_EVENT} from then on.
 */
export function takeWorkspaceIntent(): Promise<string | null> {
  return invoke('take_workspace_intent');
}

/**
 * Tell the shell a product tab is now open, so it can spawn that product's
 * backend and light it up in the launcher — everything `open_<product>_window`
 * does for a windowed product. Must be called for restored tabs too: those
 * never went through `openWorkspaceWindow`.
 */
export function workspaceTabOpened(product: string): Promise<void> {
  return invoke('workspace_tab_opened', { product });
}

/** Tell the shell a product tab closed, so its backend is torn down and the
 *  launcher node clears — the contract of closing that product's window. */
export function workspaceTabClosed(product: string): Promise<void> {
  return invoke('workspace_tab_closed', { product });
}
