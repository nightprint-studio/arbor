/**
 * The context menu of a **build unit** — a Cargo crate or a Maven module.
 *
 * Two panels list the project by build unit (Cargo and Dependencies) and a third would if the
 * Maven one were ever wired to a real reactor. The rows there answer what a unit *declares*; the
 * question they leave behind is where it actually lives, which is a fact about the Project tree.
 * Hence one menu, defined once: the same four verbs in the same order, so where they are is
 * learned once rather than per panel.
 *
 * A crate and a module are the same idea wearing two names — the folder where a build target is
 * declared — which is why {@link BuildUnit} is two fields and not two types. The only thing the
 * ecosystem changes is what the manifest is called, and that is derived from the manifest itself.
 */

import { Copy, FileCode2, LocateFixed } from 'lucide-svelte';

import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
import { copyToClipboard } from '$lib/utils/clipboard';

/** One crate or module, as little of it as the menu needs. */
export interface BuildUnit {
  /** What to call it in the labels — the crate name, or the module's `<name>`. */
  name: string;
  /** Absolute path of its `Cargo.toml` / `pom.xml`. */
  manifest: string;
}

/** The directory the unit occupies: its manifest's parent, which is the row the tree shows. */
export function buildUnitDir(unit: BuildUnit): string {
  const fwd = unit.manifest.replace(/\\/g, '/');
  const cut = fwd.lastIndexOf('/');
  return cut > 0 ? fwd.slice(0, cut) : fwd;
}

/** `Cargo.toml` / `pom.xml` — taken from the path rather than from an ecosystem flag, so the
 *  label cannot disagree with the file the entry opens. */
function manifestName(unit: BuildUnit): string {
  return unit.manifest.replace(/\\/g, '/').split('/').pop() || 'manifest';
}

/**
 * Open the menu at a point — see `SidebarSection`'s `onContextMenu`, which is where the
 * coordinates come from for both the right-click and the keyboard route.
 */
export function openBuildUnitMenu(x: number, y: number, unit: BuildUnit): void {
  const dir = buildUnitDir(unit);
  const items: MenuItem[] = [
    { id: 'focus', label: 'Focus in Project', icon: LocateFixed },
    { id: 'manifest', label: `Open ${manifestName(unit)}`, icon: FileCode2 },
    { separator: true, id: 'sep', label: '' },
    { id: 'copy-path', label: 'Copy path', icon: Copy },
    { id: 'copy-rel', label: 'Copy relative path', icon: Copy },
  ];
  bennuContextMenuStore.show(x, y, items, (id) => {
    switch (id) {
      case 'focus':     bennuUiStore.focusInTree(dir); break;
      case 'manifest':  void projectStore.openFile(unit.manifest); break;
      case 'copy-path': void copyToClipboard(dir); break;
      case 'copy-rel':  void copyToClipboard(projectStore.relativePath(dir)); break;
    }
  });
}
