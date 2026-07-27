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
  /** The destination picker — reachable from the generator AND the sidebar. */
  let addDestinationOpen = $state(false);

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
    get addDestinationOpen() { return addDestinationOpen; },

    /** True while any dialog owns the keyboard — the shell's shortcuts stand down. */
    get anyModalOpen() {
      return settingsOpen || shortcutsOpen || aboutOpen || connectionEditorOpen
        || addDestinationOpen || paletteOpen;
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
      connectionEditorId = id;
      connectionEditorOpen = true;
    },
    closeConnectionEditor() {
      connectionEditorOpen = false;
      connectionEditorId = null;
    },

    openAddDestination() { addDestinationOpen = true; },
    closeAddDestination() { addDestinationOpen = false; },
  };
}

export const picusUiStore = createPicusUiStore();
