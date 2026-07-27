/**
 * macOS system menu bar bridge (DIRECT Tauri command, not the product rpc seam).
 *
 * Wire format mirrors `src-tauri/src/native_menu.rs` — keep the two in sync.
 * Off macOS the shell command is a no-op, so callers never need to branch.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** One node of a published menu. Native menus carry no icons/subtitles/badges. */
export type NativeMenuNode =
  | {
      kind: 'item';
      /** Handler key — opaque to the shell, echoed back on click. */
      id: string;
      label: string;
      /** Tauri accelerator ("CmdOrCtrl+Shift+O"); an unparsable value is dropped. */
      accelerator?: string;
      enabled?: boolean;
      /** Present → the row renders with a checkmark slot. */
      checked?: boolean;
    }
  | { kind: 'separator' }
  | { kind: 'submenu'; label: string; items: NativeMenuNode[] };

/** One top-level menu (`File`, `Project`, `Tools`, …). */
export interface NativeMenuGroup {
  title: string;
  items: NativeMenuNode[];
}

export interface NativeMenuSpec {
  /** Title of the first (application) menu — "Arbor", "Bennu", … */
  app_name: string;
  /** Heads the application menu; when non-empty it replaces the system About item. */
  app_items: NativeMenuNode[];
  /** The menus between the application menu and `Edit`. */
  menus: NativeMenuGroup[];
}

/** Event carrying the id of a clicked item, delivered only to the publisher. */
const MENU_CLICK_EVENT = 'arbor://menu-click';

/**
 * Install `spec` as the application menu bar and route its clicks to this
 * window. macOS has one menu bar per app, so the last window to publish owns it.
 */
export function setNativeMenu(spec: NativeMenuSpec): Promise<void> {
  return invoke('set_native_menu', { spec });
}

/** Subscribe to clicks on items this window published. Returns an unlisten fn. */
export function onNativeMenuClick(handler: (id: string) => void): Promise<() => void> {
  return listen<string>(MENU_CLICK_EVENT, (e) => handler(e.payload));
}
