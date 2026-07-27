/**
 * The title bar's **Window** section — one authored menu, every product.
 *
 * Arbor drives several top-level windows and the platforms disagree on how you
 * move between them: Windows gives each window a taskbar button, macOS gives
 * none and expects a Window menu instead. So every product title bar carries
 * the same section — switcher entry plus the live list of open windows — and
 * each product spreads it into its own hamburger rather than re-authoring it.
 *
 * macOS is deliberately excluded: the system menu bar already owns a native
 * Window menu (see `native_menu.rs`), and publishing a second one next to it
 * would read as a bug. The keyboard path (`switch_window`) works everywhere.
 */
import { AppWindow, EyeOff, GitBranch, FolderTree, Music, Video, Coffee, LayoutGrid } from 'lucide-svelte';
import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
import type { IconComponent } from '$lib/types/icon';
import { windowsStore } from '$lib/stores/windows.svelte';
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
 * Call it inside a `$derived` and spread it into the menu; returns `[]` on
 * macOS, where the native Window menu already does this job.
 */
export function windowMenuItems(): DropdownItem[] {
  if (isMac) return [];

  const others = windowsStore.others;
  return [
    { kind: 'separator', label: 'Window' },
    {
      kind: 'item', id: 'switch-window', label: 'Switch Window…', icon: AppWindow,
      action: 'switch_window', onclick: () => windowsStore.openSwitcher(),
    },
    ...others.map((w) => ({
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
