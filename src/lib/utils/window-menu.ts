/**
 * The title bar's **Window** section — one authored menu, every product.
 *
 * Arbor drives several top-level windows and the platforms disagree on how you
 * move between them: Windows gives each window a taskbar button, macOS gives
 * none and expects a Window menu instead. So every product title bar carries
 * the same section — the switcher, the container's detach action, and the live
 * list of open windows — and each product spreads it into its own hamburger
 * rather than re-authoring it.
 *
 * On macOS the *listing* is dropped: the system menu bar already owns a native
 * Window menu that enumerates the windows, and a second list beside it would
 * read as a bug. The actions stay — nothing else offers them.
 */
import { AppWindow, EyeOff, GitBranch, FolderTree, Music, Video, Coffee, LayoutGrid, PictureInPicture2 } from 'lucide-svelte';
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import type { IconComponent } from '$lib/types/icon';
import { windowsStore } from '$lib/stores/windows.svelte';
import { surfaceStore } from '$lib/stores/surfaces.svelte';
import { detachSurface } from '$lib/utils/open-product';
import { isMac } from '$lib/utils/platform';

const PRODUCT_ICONS: Record<string, IconComponent> = {
  corvus: GitBranch,
  sitta:  FolderTree,
  merula: Music,
  tyto:   Video,
  bennu:  Coffee,
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

  if (isMac) return items;

  return [
    ...items,
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
