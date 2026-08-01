/**
 * The title bar's **Window** section — one authored menu, every product.
 *
 * Arbor drives several top-level windows and the platforms disagree on how you
 * move between them: Windows gives each window a taskbar button, macOS gives
 * none and expects a Window menu instead. So every product title bar carries
 * the same section — the switcher, the container's detach action, the Move &
 * Resize zones, and the live list of open windows — and each product spreads it
 * into its own hamburger rather than re-authoring it.
 *
 * On macOS the *listing* and the *zones* are dropped: the system menu bar
 * already owns a native Window menu that enumerates the windows, and the real
 * green button pops the OS tiling panel. The actions stay — nothing else
 * offers them.
 */
import { AppWindow, EyeOff, GitBranch, FolderTree, Music, Video, Coffee, Database, NotebookPen, LayoutGrid, PictureInPicture2, CornerUpLeft, Monitor } from 'lucide-svelte';
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import type { IconComponent } from '$lib/types/icon';
import { windowsStore } from '$lib/stores/windows.svelte';
import { displaysStore } from '$lib/stores/displays.svelte';
import { surfaceStore } from '$lib/stores/surfaces.svelte';
import { detachSurface } from '$lib/utils/open-product';
import { isMac } from '$lib/utils/platform';
import {
  applyZone, restorePrevious, moveToDisplay,
  ZONE_GROUPS, ZONE_LABELS, type DisplayInfo,
} from '$lib/utils/window-tiling';

/**
 * Keyboard path to the same zones — and the same categories — the zoom button
 * offers on hover. `displays` comes from the caller so this stays synchronous;
 * pass none and the display switcher is simply absent.
 */
function moveResizeSubmenu(displays: DisplayInfo[] = []): DropdownItem {
  const items: DropdownItem[] = [];
  for (const group of ZONE_GROUPS) {
    items.push({ kind: 'separator', label: group.title });
    for (const zone of group.zones) {
      items.push({
        kind: 'item', id: `tile:${zone}`, label: ZONE_LABELS[zone],
        onclick: () => void applyZone(zone),
      });
    }
  }
  if (displays.length > 1) {
    items.push({ kind: 'separator', label: 'Displays' });
    for (const d of displays) {
      items.push({
        kind: 'item', id: `display:${d.index}`, label: `Move to ${d.label}`,
        icon: Monitor, meta: `${d.width}×${d.height}`, disabled: d.current,
        onclick: () => void moveToDisplay(d.index),
      });
    }
  }
  items.push(
    { kind: 'separator' },
    {
      kind: 'item', id: 'tile:restore', label: 'Return to Previous Size',
      icon: CornerUpLeft, onclick: () => void restorePrevious(),
    },
  );
  return { kind: 'submenu', id: 'move-resize', label: 'Move & Resize', icon: LayoutGrid, items };
}

const PRODUCT_ICONS: Record<string, IconComponent> = {
  corvus: GitBranch,
  sitta:  FolderTree,
  merula: Music,
  tyto:   Video,
  bennu:  Coffee,
  picus:  Database,
  garrulus: NotebookPen,
};

/**
 * The Window section for a product hamburger, live against the open-window set.
 * Call it inside a `$derived` and spread it into the menu.
 */
export function windowMenuItems(): DropdownItem[] {
  const items: DropdownItem[] = [
    { kind: 'separator', label: 'Window' },
    {
      kind: 'item', id: 'switch-window', label: 'Switch Window…', icon: AppWindow,
      action: 'switch_window', onclick: () => windowsStore.openSwitcher(),
    },
  ];

  // Only meaningful inside the tabbed container: pull the current tab out into
  // a window of its own (a second monitor, or simply more room).
  const active = surfaceStore.active;
  if (surfaceStore.inContainer && active) {
    items.push({
      kind: 'item', id: 'detach-tab', label: 'Move Tab to New Window',
      icon: PictureInPicture2, onclick: () => void detachSurface(active),
    });
  }

  // macOS owns both halves of this natively: the system Window menu enumerates
  // the windows, and the real green button pops the OS Move & Resize panel.
  if (isMac) return items;

  return [
    ...items,
    moveResizeSubmenu(displaysStore.list),
    ...windowsStore.others.map((w) => ({
      kind: 'item' as const,
      id: `window:${w.label}`,
      label: w.title,
      // A tray'd window still lists — picking it is the way back — but it reads
      // as hidden so the user isn't surprised by a window appearing from nowhere.
      icon: w.visible ? (PRODUCT_ICONS[w.product ?? ''] ?? AppWindow) : EyeOff,
      onclick: () => void windowsStore.focus(w.label),
    })),
  ];
}
