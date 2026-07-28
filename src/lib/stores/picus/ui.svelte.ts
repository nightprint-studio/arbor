/**
 * Picus window UI state — which sidebar section is up, which bottom tab is
 * showing, and the modal/overlay flags. Pure view state: nothing here is
 * persisted and nothing here touches IPC.
 *
 * Panel visibility follows Arbor's convention (Bennu / Corvus): clicking the
 * active rail icon collapses the sidebar, clicking another switches to it.
 */

/** The four left-rail sections. */
export type SidebarSection = 'connections' | 'scripts' | 'generate' | 'inventory';

/** The bottom dock's tabs. Consistency is the default — it is the panel the
 *  product is judged on. */
export type BottomTab = 'consistency' | 'output' | 'changes';

/** Sub-view of a table tab: its rows, its columns, or its DDL. */
export type TableSubview = 'data' | 'structure' | 'ddl';

function createPicusUiStore() {
  let sidebarSection = $state<SidebarSection>('connections');
  let sidebarOpen = $state(true);
  let bottomOpen = $state(true);
  let bottomTab = $state<BottomTab>('consistency');

  let tableSubview = $state<TableSubview>('data');

  let paletteOpen = $state(false);
  let settingsOpen = $state(false);
  let shortcutsOpen = $state(false);
  let aboutOpen = $state(false);
  let docsOpen = $state(false);
  let connectionEditorOpen = $state(false);
  /** Connection being edited; `null` means "new connection". */
  let connectionEditorId = $state<string | null>(null);
  /** Connection whose read-only inspector is up; `null` means closed. */
  let connectionDetailsId = $state<string | null>(null);
  /**
   * Connection the user asked to delete, waiting on the confirmation.
   *
   * The dialog is owned here rather than by the sidebar because deleting is
   * reachable from the command palette too, and a destructive confirmation must
   * not depend on which panel happens to be on screen.
   */
  let connectionDeleteId = $state<string | null>(null);
  /** The destination picker — reachable from the generator AND the sidebar. */
  let addDestinationOpen = $state(false);
  /**
   * Connection whose script repository is being attached; `null` means closed.
   *
   * Owned here rather than by the scripts panel because attaching is offered from
   * that panel, from the connection editor and from the palette, and the folder
   * picker must not depend on which of the three is on screen.
   */
  let scriptRootPickerId = $state<string | null>(null);

  return {
    get sidebarSection() { return sidebarSection; },
    get sidebarOpen() { return sidebarOpen; },
    get bottomOpen() { return bottomOpen; },
    get bottomTab() { return bottomTab; },
    get tableSubview() { return tableSubview; },
    get paletteOpen() { return paletteOpen; },
    get settingsOpen() { return settingsOpen; },
    get shortcutsOpen() { return shortcutsOpen; },
    get aboutOpen() { return aboutOpen; },
    get docsOpen() { return docsOpen; },
    get connectionEditorOpen() { return connectionEditorOpen; },
    get connectionEditorId() { return connectionEditorId; },
    get connectionDetailsId() { return connectionDetailsId; },
    get connectionDeleteId() { return connectionDeleteId; },
    get addDestinationOpen() { return addDestinationOpen; },
    get scriptRootPickerId() { return scriptRootPickerId; },

    /** True while any dialog owns the keyboard — the shell's shortcuts stand down. */
    get anyModalOpen() {
      return settingsOpen || shortcutsOpen || aboutOpen || connectionEditorOpen
        || connectionDetailsId !== null || connectionDeleteId !== null
        || addDestinationOpen || paletteOpen || scriptRootPickerId !== null;
    },

    /** Rail click: same section → collapse; different section → switch + open. */
    selectSection(section: SidebarSection) {
      if (sidebarOpen && sidebarSection === section) { sidebarOpen = false; return; }
      sidebarSection = section;
      sidebarOpen = true;
    },
    /** Open a section without the collapse-on-repeat behaviour (palette, deep links). */
    showSection(section: SidebarSection) {
      sidebarSection = section;
      sidebarOpen = true;
    },
    toggleSidebar() { sidebarOpen = !sidebarOpen; },
    closeSidebar() { sidebarOpen = false; },

    toggleBottom() { bottomOpen = !bottomOpen; },
    closeBottom() { bottomOpen = false; },
    /** Rail/status-bar entry point: reveal the dock on a given tab. */
    showBottom(tab: BottomTab) {
      bottomTab = tab;
      bottomOpen = true;
    },
    setBottomTab(tab: BottomTab) { bottomTab = tab; },
    setTableSubview(v: TableSubview) { tableSubview = v; },

    togglePalette() { paletteOpen = !paletteOpen; },
    closePalette() { paletteOpen = false; },
    openSettings() { settingsOpen = true; },
    closeSettings() { settingsOpen = false; },
    openShortcuts() { shortcutsOpen = true; },
    closeShortcuts() { shortcutsOpen = false; },
    openAbout() { aboutOpen = true; },
    closeAbout() { aboutOpen = false; },
    toggleDocs() { docsOpen = !docsOpen; },
    closeDocs() { docsOpen = false; },

    openConnectionEditor(id: string | null = null) {
      // Opening the editor from the inspector replaces it rather than stacking
      // two dialogs about the same connection on top of each other.
      connectionDetailsId = null;
      connectionEditorId = id;
      connectionEditorOpen = true;
    },
    closeConnectionEditor() {
      connectionEditorOpen = false;
      connectionEditorId = null;
    },

    openConnectionDetails(id: string) { connectionDetailsId = id; },
    closeConnectionDetails() { connectionDetailsId = null; },

    /** Ask for a connection to be deleted — the shell puts the confirmation up. */
    requestConnectionDelete(id: string) {
      connectionDetailsId = null;
      connectionDeleteId = id;
    },
    cancelConnectionDelete() { connectionDeleteId = null; },

    openAddDestination() { addDestinationOpen = true; },
    closeAddDestination() { addDestinationOpen = false; },

    /** Attach (or re-point) the script repository of one connection. */
    openScriptRootPicker(connectionId: string) { scriptRootPickerId = connectionId; },
    closeScriptRootPicker() { scriptRootPickerId = null; },
  };
}

export const picusUiStore = createPicusUiStore();
