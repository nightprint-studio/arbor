/**
 * Garrulus window chrome state — which sidebar section is showing, whether the
 * sidebar is showing at all, and which overlay is in front.
 *
 * Session-shaped: it describes what is on screen right now, not a preference, so
 * nothing here is persisted (a real setting would go to
 * `profiles/<p>/garrulus/config.toml` through `garrulus-be`, per CLAUDE.md).
 *
 * Overlays live here rather than in whichever component opens them because every
 * one of them has more than one way in — the vault picker alone is reached from
 * the start pane, from the palette, from a shortcut and from the title bar — and
 * a dialog must not depend on which of them was hit.
 */

/** The sidebar's sections, in the order the activity rail lists them. */
export type SidebarSection = 'notes' | 'search' | 'tags' | 'types';

/** Which page of the vault dialog to land on. Both are the same decision made
 *  twice, so they are one dialog rather than two. */
export type VaultPickerPage = 'open' | 'create';

/** The bottom dock's panels. Only `conflicts` has a component today; the other
 *  two are named because the palette already offers them and the ids have to
 *  agree in one place rather than in two string literals. */
export const DOCK_PANELS = ['conflicts', 'tasks', 'problems'] as const;
export type DockPanel = (typeof DOCK_PANELS)[number];

function createGarrulusUiStore() {
  let sidebarOpen = $state(true);
  let sidebarSection = $state<SidebarSection>('notes');

  /** The bottom dock. Its *visibility* lives here rather than in the shell
   *  because three things open it — the sync control on a conflict, the palette,
   *  and `Ctrl+J` — and a flag owned by one of them would be a flag the other two
   *  cannot reach. Its height stays in the shell: that is layout, not state. */
  let dockOpen = $state(false);
  let dockPanel = $state<DockPanel>('conflicts');

  let remoteConfigOpen = $state(false);
  let commitMessageOpen = $state(false);
  let vaultPickerOpen = $state(false);
  let vaultPickerPage = $state<VaultPickerPage>('open');
  let paletteOpen = $state(false);
  let docsOpen = $state(false);
  /** Topic the docs panel lands on; `''` means "wherever it opens by default". */
  let docsSection = $state('');

  /**
   * Is a dialog in front? The window's keydown handler defers to it.
   *
   * The palette counts — it owns the keyboard while it is up — which is why the
   * shell tests `Ctrl+K` *before* consulting this: an overlay that this getter
   * hides the toggle for could be opened and never closed.
   *
   * The docs panel deliberately does not count: it is a reading surface beside
   * the work, and F1 has to close it again from wherever the focus went.
   */
  const anyModalOpen = $derived(
    remoteConfigOpen || commitMessageOpen || vaultPickerOpen || paletteOpen,
  );

  return {
    get sidebarOpen() { return sidebarOpen; },
    get sidebarSection() { return sidebarSection; },
    get remoteConfigOpen() { return remoteConfigOpen; },
    get commitMessageOpen() { return commitMessageOpen; },
    get vaultPickerOpen() { return vaultPickerOpen; },
    get vaultPickerPage() { return vaultPickerPage; },
    get paletteOpen() { return paletteOpen; },
    get docsOpen() { return docsOpen; },
    get docsSection() { return docsSection; },
    get anyModalOpen() { return anyModalOpen; },

    /**
     * Show a section — or collapse the sidebar when the section already showing
     * is clicked again. The IntelliJ behaviour: the rail button is a toggle for
     * its own panel, not a radio that can only ever open something.
     */
    showSection(section: SidebarSection) {
      if (sidebarOpen && sidebarSection === section) {
        sidebarOpen = false;
        return;
      }
      sidebarSection = section;
      sidebarOpen = true;
    },

    /**
     * Show a section without the collapse-on-repeat of `showSection`.
     *
     * The rail button is a toggle for its own panel; a palette verb or a
     * shortcut is not — "Search the vault" must open Search, never close it.
     */
    selectSection(section: SidebarSection) {
      sidebarSection = section;
      sidebarOpen = true;
    },

    toggleSidebar() { sidebarOpen = !sidebarOpen; },

    get dockOpen() { return dockOpen; },
    get dockPanel() { return dockPanel; },

    /**
     * Open the dock on a given panel — the palette passes an id, the sync
     * control passes nothing and means conflicts.
     *
     * An unknown id opens conflicts rather than throwing: the palette is the only
     * caller that names one, and a verb that silently did nothing would be worse
     * than a verb that lands one panel over.
     */
    showDock(panel: DockPanel = 'conflicts') {
      dockPanel = DOCK_PANELS.includes(panel) ? panel : 'conflicts';
      dockOpen = true;
    },
    toggleDock() { dockOpen = !dockOpen; },
    closeDock() { dockOpen = false; },

    openRemoteConfig() { remoteConfigOpen = true; },
    closeRemoteConfig() { remoteConfigOpen = false; },
    openCommitMessage() { commitMessageOpen = true; },
    closeCommitMessage() { commitMessageOpen = false; },

    /** The door to the product. `page` is which half of it opens — "create a
     *  vault" is its own verb in the palette and lands on its own page. */
    openVaultPicker(page: VaultPickerPage = 'open') {
      vaultPickerPage = page;
      vaultPickerOpen = true;
    },
    closeVaultPicker() { vaultPickerOpen = false; },

    togglePalette() { paletteOpen = !paletteOpen; },
    closePalette() { paletteOpen = false; },

    toggleDocs() {
      docsSection = '';
      docsOpen = !docsOpen;
    },
    /** `section` lands the panel on one topic — the palette addresses them by
     *  name, which is how a page nobody would guess at gets found. */
    openDocs(section = '') {
      docsSection = section;
      docsOpen = true;
    },
    closeDocs() {
      docsOpen = false;
      docsSection = '';
    },
  };
}

export const garrulusUiStore = createGarrulusUiStore();
