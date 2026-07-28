/**
 * Picus window UI state — which sidebar section is up, which bottom tab is
 * showing, and the modal/overlay flags. Pure view state: nothing here is
 * persisted and nothing here touches IPC.
 *
 * Panel visibility follows Arbor's convention (Bennu / Corvus): clicking the
 * active rail icon collapses the sidebar, clicking another switches to it.
 */

import type { FolderEngine, FolderRole } from '$lib/types/picus';

/** The four left-rail sections. */
export type SidebarSection = 'connections' | 'scripts' | 'generate' | 'inventory';

/** The bottom dock's tabs. Consistency is the default — it is the panel the
 *  product is judged on. */
export type BottomTab = 'results' | 'consistency' | 'output' | 'changes';

/** Sub-view of a table tab: its rows, its columns, or its DDL. */
export type TableSubview = 'data' | 'structure' | 'ddl';

/** Did the offer come from classifying a folder, or one file? */
export type AliasOfferKind = 'folder' | 'file';

/**
 * An offer to turn one classification into a project-wide rule about a **name**.
 *
 * Raised the moment the user classifies something whose name repeats, because
 * that is the moment they have the knowledge: they just said what `POS` is, and
 * there are ten more folders — or thirty more files — with `POS` in the name. It
 * carries the paths rather than only a count so the offer can name what it would
 * touch, and both lists rather than one so the choice between "folders", "files"
 * and "both" is made with the numbers in view.
 */
export interface AliasOffer {
  /** Which classification raised it — it decides what the rule defaults to. */
  kind: AliasOfferKind;
  /** The name the rule would be about. */
  name: string;
  /** The engine the user just declared, when that is what they declared. */
  engine: FolderEngine | null;
  /** The role they just declared. Folders only — a file has no role. */
  role: FolderRole | null;
  /** Every folder of that name, in tree order. The backend's own answer. */
  folderPaths: string[];
  /** Every file with that word in its name; absent for a folder-born offer. */
  filePaths?: string[];
  /** The one they classified, so the offer can talk about "the other ten". */
  origin: string;
  /**
   * Other words of the file's name.
   *
   * A folder's name is the whole name and there is nothing to choose. A file's
   * is a sentence, so the word carrying the meaning is a **guess**, and a guess
   * the user cannot correct is one they have to decline.
   */
  alternatives?: string[];
}

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
  /**
   * Folder whose engine and role are being set; `null` means closed, `''` means
   * "open, nothing picked yet".
   *
   * Owned here rather than by the scripts tree because classifying is reachable
   * from the tree row, from the command palette and from the destination picker
   * — and the dialog must not depend on which of the three is on screen. This is
   * the same reasoning as the connection editor above it.
   */
  let folderClassifyPath = $state<string | null>(null);
  /**
   * File whose own engine is being set; `null` means closed, `''` means "open,
   * nothing picked yet".
   *
   * A separate dialog from the folder one rather than a mode of it: they offer
   * different answers (a file has no role), they are reached from different
   * rows, and the file one exists for a case the folder one cannot express. They
   * are owned here for the same reason, though — both are reachable from the
   * tree, the palette and a keystroke.
   */
  let fileClassifyPath = $state<string | null>(null);
  /**
   * The "…and every folder named POS" offer, waiting on an answer.
   *
   * A **second, distinct** action rather than a side effect of the classification
   * that preceded it: declaring what one folder is and declaring what a name
   * means project-wide are different decisions with different blast radii, and
   * folding the second into the first would mean the user reaches eleven folders
   * by pressing a button that named one.
   */
  let aliasOffer = $state<AliasOffer | null>(null);
  /**
   * Names the user has already declined this session.
   *
   * Without this, saying "no" to the offer on the first `POS` folder means being
   * asked again on the second, and the tenth. One refusal is an answer.
   */
  let declinedAliases = $state<string[]>([]);
  /** Settings page to open on; `''` means "wherever it was". */
  let settingsSection = $state('');
  /** Docs topic to open on; `''` means "the panel's own first page". */
  let docsSection = $state('');

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
    get folderClassifyPath() { return folderClassifyPath; },
    get fileClassifyPath() { return fileClassifyPath; },
    get aliasOffer() { return aliasOffer; },
    get settingsSection() { return settingsSection; },
    get docsSection() { return docsSection; },

    /** True while any dialog owns the keyboard — the shell's shortcuts stand down. */
    get anyModalOpen() {
      return settingsOpen || shortcutsOpen || aboutOpen || connectionEditorOpen
        || connectionDetailsId !== null || connectionDeleteId !== null
        || addDestinationOpen || paletteOpen || scriptRootPickerId !== null
        || folderClassifyPath !== null || fileClassifyPath !== null || aliasOffer !== null;
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
    /** `section` lands the dialog on one page — the palette addresses them by name. */
    openSettings(section = '') {
      settingsSection = section;
      settingsOpen = true;
    },
    closeSettings() {
      settingsOpen = false;
      settingsSection = '';
    },
    openShortcuts() { shortcutsOpen = true; },
    closeShortcuts() { shortcutsOpen = false; },
    openAbout() { aboutOpen = true; },
    closeAbout() { aboutOpen = false; },
    toggleDocs() {
      docsSection = '';
      docsOpen = !docsOpen;
    },
    /** `section` lands the panel on one topic — the palette addresses them by name,
     *  the way a feature nobody would guess at gets found. */
    openDocs(section = '') {
      docsSection = section;
      docsOpen = true;
    },
    closeDocs() {
      docsOpen = false;
      docsSection = '';
    },

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

    /** Say what a folder is. `''` opens the dialog on its own folder picker. */
    openFolderClassify(path = '') { folderClassifyPath = path; },
    closeFolderClassify() { folderClassifyPath = null; },

    /** Say what one file is. `''` opens the dialog on its own file picker. */
    openFileClassify(path = '') { fileClassifyPath = path; },
    closeFileClassify() { fileClassifyPath = null; },

    /**
     * Offer to turn what was just decided about one folder — or one file — into
     * a rule about a name. Ignored when the user already declined that name this
     * session.
     */
    offerAlias(offer: AliasOffer) {
      if (declinedAliases.includes(offer.name.toLowerCase())) return;
      aliasOffer = offer;
    },
    /** They said no. Do not ask about this name again while the window is open. */
    declineAlias() {
      if (aliasOffer) declinedAliases = [...declinedAliases, aliasOffer.name.toLowerCase()];
      aliasOffer = null;
    },
    /** They said yes, or the offer is otherwise finished with. */
    closeAlias() { aliasOffer = null; },
  };
}

export const picusUiStore = createPicusUiStore();
