/**
 * Hirundo window chrome state — which sidebar section and bottom panel show, which overlay is in front.
 *
 * Session-shaped (what is on screen right now), so nothing here is persisted: the real preferences —
 * workspaces, the project to reopen on — live in `profiles/<p>/hirundo/config.toml` through `hirundo-be`.
 */

/** The left sidebar: things you pick from. */
export type SidebarSection = 'collections' | 'detected' | 'environments';

/** The right sidebar: what ran and what is planned — wide lists that are read beside the tree you pick from. */
export type RightSection = 'runner' | 'history' | 'schedules' | 'cookies' | 'stream';

/** The sub-tabs of the request editor. */
export type EditorTab = 'params' | 'headers' | 'auth' | 'body' | 'tests' | 'docs';

/** A dialog in front. One slot: only one can be, and a dialog that opens another replaces it. */
export type Dialog =
  | { kind: 'curl' }
  | { kind: 'environments' }
  | { kind: 'variables' }
  | { kind: 'request-settings' }
  | { kind: 'save-as' }
  | { kind: 'new-collection' }
  | { kind: 'new-folder'; parent: string }
  | { kind: 'rename'; path: string; name: string }
  | { kind: 'meta'; path: string }
  | { kind: 'delete'; path: string; name: string }
  | { kind: 'project-picker'; workspaceId?: string }
  | { kind: 'workspaces' }
  | { kind: 'schedule'; id: string | null }
  | { kind: 'run'; path: string }
  | { kind: 'soap-import' }
  | { kind: 'openapi-import' }
  | { kind: 'http-import' }
  | { kind: 'http-export'; path: string }
  | { kind: 'diff' }
  | { kind: 'generate-code' };

function createHirundoUiStore() {
  let sidebarOpen = $state(true);
  let section = $state<SidebarSection>('collections');
  /** The bottom panel holds one thing: the answer to the open request. */
  let bottomOpen = $state(true);
  let rightOpen = $state(false);
  let rightSection = $state<RightSection>('runner');
  let editorTab = $state<EditorTab>('params');
  let dialog = $state<Dialog | null>(null);
  let paletteOpen = $state(false);
  let searchOpen = $state(false);
  let docsOpen = $state(false);
  let docsSection = $state('');
  /** Show the bulk-edit text instead of the table on the current editor tab. */
  let bulk = $state(false);

  const anyModalOpen = $derived(dialog != null || paletteOpen || searchOpen);

  return {
    get sidebarOpen() { return sidebarOpen; },
    get section() { return section; },
    get bottomOpen() { return bottomOpen; },
    get rightOpen() { return rightOpen; },
    get rightSection() { return rightSection; },
    get editorTab() { return editorTab; },
    get dialog() { return dialog; },
    get paletteOpen() { return paletteOpen; },
    get searchOpen() { return searchOpen; },
    get docsOpen() { return docsOpen; },
    get docsSection() { return docsSection; },
    get bulk() { return bulk; },
    get anyModalOpen() { return anyModalOpen; },

    toggleSidebar() { sidebarOpen = !sidebarOpen; },
    /** Rail click: the open section collapses the sidebar, another opens and shows it. */
    showSection(s: SidebarSection) {
      if (sidebarOpen && section === s) sidebarOpen = false;
      else { section = s; sidebarOpen = true; }
    },
    selectSection(s: SidebarSection) { section = s; sidebarOpen = true; },

    toggleBottom() { bottomOpen = !bottomOpen; },
    closeBottom() { bottomOpen = false; },
    /** Rail click on the right: the open section collapses the sidebar, another opens and shows it. */
    toggleRight(s: RightSection) {
      if (rightOpen && rightSection === s) rightOpen = false;
      else { rightSection = s; rightOpen = true; }
    },
    showRight(s: RightSection) { rightSection = s; rightOpen = true; },
    closeRight() { rightOpen = false; },

    setEditorTab(t: EditorTab) { editorTab = t; bulk = false; },
    toggleBulk() { bulk = !bulk; },
    open(d: Dialog) { dialog = d; },
    closeDialog() { dialog = null; },
    togglePalette() { paletteOpen = !paletteOpen; },
    closePalette() { paletteOpen = false; },
    openSearch() { searchOpen = true; },
    closeSearch() { searchOpen = false; },
    toggleDocs() { docsOpen = !docsOpen; },
    openDocs(s = '') { docsSection = s; docsOpen = true; },
    closeDocs() { docsOpen = false; },
  };
}

export const hirundoUiStore = createHirundoUiStore();
