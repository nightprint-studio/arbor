/**
 * Title-bar menu → macOS system menu bar.
 *
 * Windows and Linux get Arbor's own chrome, where the hamburger (☰) IS the menu.
 * macOS expects a real menu bar at the top of the screen, so on the Mac the
 * shared `TitleBar` hides the hamburger and hands the very same `DropdownItem[]`
 * to a publisher built here: it derives a serializable tree, ships it to the
 * shell (`set_native_menu`) and runs the original `onclick` when the OS reports
 * a click.
 *
 * Deriving beats hand-authoring a second menu per product: one source of truth,
 * no drift between the burger and the bar. The grouping rule is the one the
 * menus already follow — **a labelled separator starts a new top-level menu**:
 *
 *     { kind: 'separator', label: 'File' }   → the "File" menu
 *     { kind: 'item', … }                    → its entries
 *     { kind: 'separator' }                  → a divider inside it
 *
 * Menus whose items carry no labelled separator land under `fallbackTitle`.
 * When the derived shape isn't the mac-idiomatic one, pass `groups` to regroup
 * the items yourself — everything else (ids, accelerators, handlers) still comes
 * from the same items.
 *
 * Two ids are reserved at the TOP level, because macOS already owns those verbs:
 *   • `about`                  → heads the application menu, in place of the
 *                                system About item
 *   • `exit` / `quit` / `close`→ dropped (App ▸ Quit and Window ▸ Close Window)
 *
 * Off macOS every entry point here is inert.
 */
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import {
  onNativeMenuClick, setNativeMenu,
  type NativeMenuGroup, type NativeMenuNode, type NativeMenuSpec,
} from '$lib/ipc/native-menu';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { acceleratorFromLabel } from '$lib/utils/keybindings';
import { isMac } from '$lib/utils/platform';
import { acceleratorForAction } from '$lib/utils/shortcut';

/** Top-level ids the application menu absorbs. */
const APP_MENU_IDS = new Set(['about']);
/** Top-level ids macOS already provides natively — dropped while deriving. */
const NATIVE_IDS = new Set(['exit', 'quit', 'close']);

export interface NativeMenuOptions {
  /** Menu title for items preceding the first labelled separator (default `File`). */
  fallbackTitle?: string;
  /** Override the automatic grouping; ids, accelerators and handlers still derive. */
  groups?: (items: DropdownItem[]) => { title: string; items: DropdownItem[] }[];
}

type Handlers = Map<string, () => void>;

// ───────────────────────────────────────────────────────────────────────────
//  Derivation
// ───────────────────────────────────────────────────────────────────────────

/** Recursively convert dropdown items, registering each handler under its path id. */
function convert(items: DropdownItem[], path: string, handlers: Handlers): NativeMenuNode[] {
  const out: NativeMenuNode[] = [];
  items.forEach((item, i) => {
    const id = `${path}${i}`;
    if (item.kind === 'separator') {
      // Native menus have no labelled separators, and a leading or doubled
      // divider just draws a stray line.
      if (out.length && out[out.length - 1].kind !== 'separator') out.push({ kind: 'separator' });
      return;
    }
    if (item.kind === 'item') {
      handlers.set(id, item.onclick);
      const node: NativeMenuNode = {
        kind: 'item', id, label: item.label, enabled: !item.disabled,
      };
      const accelerator =
        acceleratorForAction(item.action) ??
        (item.shortcut ? acceleratorFromLabel(item.shortcut) : null);
      if (accelerator) node.accelerator = accelerator;
      if (item.active !== undefined) node.checked = item.active;
      out.push(node);
      return;
    }
    // 'submenu' and 'group' both become a native submenu — the collapsible
    // group affordance has no native counterpart, its label does.
    const children = convert(item.items, `${id}.`, handlers);
    if (children.length) out.push({ kind: 'submenu', label: item.label, items: children });
  });
  while (out.length && out[out.length - 1].kind === 'separator') out.pop();
  return out;
}

/** Split a flat menu into top-level menus at every labelled separator. */
function autoGroups(items: DropdownItem[], fallbackTitle: string) {
  const groups: { title: string; items: DropdownItem[] }[] = [];
  let current = { title: fallbackTitle, items: [] as DropdownItem[] };
  for (const item of items) {
    if (item.kind === 'separator' && item.label) {
      if (current.items.length) groups.push(current);
      current = { title: item.label, items: [] };
      continue;
    }
    current.items.push(item);
  }
  if (current.items.length) groups.push(current);
  return groups;
}

/**
 * Turn a title-bar menu into a publishable spec plus the click handlers it maps
 * to. Exported for tests and for callers that need the spec without publishing.
 */
export function deriveNativeMenu(
  items: DropdownItem[],
  appName: string,
  opts: NativeMenuOptions = {},
): { spec: NativeMenuSpec; handlers: Handlers } {
  const handlers: Handlers = new Map();

  const appItems = items.filter(i => i.kind === 'item' && APP_MENU_IDS.has(i.id));
  const rest = items.filter(
    i => i.kind !== 'item' || (!APP_MENU_IDS.has(i.id) && !NATIVE_IDS.has(i.id)),
  );

  const grouped = opts.groups?.(rest) ?? autoGroups(rest, opts.fallbackTitle ?? 'File');
  const menus: NativeMenuGroup[] = grouped
    .map((g, gi) => ({ title: g.title, items: convert(g.items, `${gi}.`, handlers) }))
    .filter(g => g.items.length > 0);

  return {
    spec: { app_name: appName, app_items: convert(appItems, 'app.', handlers), menus },
    handlers,
  };
}

// ───────────────────────────────────────────────────────────────────────────
//  Publisher
// ───────────────────────────────────────────────────────────────────────────
//
// Module-level, not per-publisher: a window has exactly one title bar, and
// keeping the click listener a true singleton means a hot-reloaded title bar
// can't leave a stale listener firing dead handlers behind it.

let handlers: Handlers = new Map();
let lastJson = '';
let lastSpec: NativeMenuSpec | null = null;
let wired = false;

function wire(): void {
  if (wired) return;
  wired = true;
  void onNativeMenuClick(id => handlers.get(id)?.());
  // The bar is app-wide on macOS, so another window may have replaced ours
  // while we were in the background — reclaim it whenever we come back to front.
  void getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (focused && lastSpec) void setNativeMenu(lastSpec);
  });
}

/**
 * Build the `onNativeMenu` callback for a product's title bar:
 *
 *     const publishNativeMenu = createNativeMenuPublisher('Arbor');
 *     <TitleBar menu={hamburgerMenu} onNativeMenu={publishNativeMenu} … />
 *
 * The callback is cheap to call on every change — it re-publishes only when the
 * derived tree actually differs, so reactive churn (a theme flag, a plugin pill)
 * doesn't rebuild the OS menu. Off macOS it does nothing at all.
 */
export function createNativeMenuPublisher(
  appName: string,
  opts: NativeMenuOptions = {},
): (items: DropdownItem[]) => void {
  if (!isMac) return () => {};
  return (items: DropdownItem[]) => {
    const derived = deriveNativeMenu(items, appName, opts);
    // Always adopt the fresh closures, even when the shape is unchanged: the
    // items are rebuilt on every reactive pass and the old ones capture stale state.
    handlers = derived.handlers;
    const json = JSON.stringify(derived.spec);
    if (json === lastJson) return;
    lastJson = json;
    lastSpec = derived.spec;
    wire();
    // On failure forget the cache so the next change retries instead of
    // believing a menu is installed when none is.
    void setNativeMenu(derived.spec).catch(() => { lastJson = ''; });
  };
}
