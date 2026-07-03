/**
 * Bennu settings store — the editable, user-facing preferences for the Java
 * editor window (font/tab size, completion behaviour, folding, Java indexing).
 *
 * Rune store — the single approved shape: private `$state`, a returned object of
 * getters + setters (see CLAUDE.md · "Store pattern").
 *
 * MOCK persistence — wire to a future [bennu] config via BE config commands
 * (rule 11). For now every value lives in-memory only: this store is the SEAM,
 * shaped 1:1 to a future typed `[bennu]` section. When that config lands, the
 * setters call `set_bennu_config`, and `loadConfig()` (added here) hydrates from
 * `get_bennu_config` at window boot — the consumer surface (getters/setters)
 * stays identical, so `BennuSettingsModal` never changes. Do NOT reach for
 * localStorage for any of these (rule 11).
 */

/** How the editor turns a Tab press into whitespace. */
export type IndentStyle = 'spaces' | 'tabs';

/** Source encodings Bennu can decode Java files as. Cp1252 / ISO-8859-1 are the
 *  common legacy declarations found in older Maven poms. */
export const SOURCE_ENCODINGS = ['UTF-8', 'Cp1252', 'ISO-8859-1'] as const;
export type SourceEncoding = (typeof SOURCE_ENCODINGS)[number];

/**
 * The full editable settings snapshot. Field names are snake_case-free on the FE
 * (camelCase getters), but the shape maps directly onto a future `[bennu]` TOML
 * table — keep the grouping (editor / completion / folding / java) stable so the
 * BE mapping is mechanical.
 */
export interface BennuSettingsSnapshot {
  // Editor
  fontSize: number;
  tabSize: number;
  indentStyle: IndentStyle;
  wordWrap: boolean;
  showWhitespace: boolean;
  highlightCurrentLine: boolean;
  showLineNumbers: boolean;
  /** Vertical margin guide column (IntelliJ's hard-wrap ruler). 0 = hidden. */
  rightMargin: number;
  // Completion
  autoPopup: boolean;
  popupDelayMs: number;
  caseSensitive: boolean;
  autoImport: boolean;
  // Folding
  foldingEnabled: boolean;
  foldBlockComments: boolean;
  // Java Style (drives the Generate flow's output formatting)
  finalParams: boolean;
  useLombokVal: boolean;
  switchWithReturn: boolean;
  spaceInBraces: boolean;
  blankLineBetweenMembers: boolean;
  // Java
  defaultEncoding: SourceEncoding;
  rebuildIndexOnOpen: boolean;
  excludedDirs: string;
}

/** Sensible defaults — IntelliJ-flavoured (4-space indent, auto-popup on, index
 *  rebuild on open, target/.git excluded). */
const DEFAULTS: BennuSettingsSnapshot = {
  fontSize: 13,
  tabSize: 4,
  indentStyle: 'spaces',
  wordWrap: false,
  showWhitespace: false,
  highlightCurrentLine: true,
  showLineNumbers: true,
  rightMargin: 120,
  autoPopup: true,
  popupDelayMs: 150,
  caseSensitive: false,
  autoImport: true,
  foldingEnabled: true,
  foldBlockComments: false,
  finalParams: false,
  useLombokVal: false,
  switchWithReturn: true,
  spaceInBraces: false,
  blankLineBetweenMembers: true,
  defaultEncoding: 'UTF-8',
  rebuildIndexOnOpen: true,
  excludedDirs: 'target, .git, .idea, build',
};

function createSettingsStore() {
  // Editor
  let fontSize = $state(DEFAULTS.fontSize);
  let tabSize = $state(DEFAULTS.tabSize);
  let indentStyle = $state<IndentStyle>(DEFAULTS.indentStyle);
  let wordWrap = $state(DEFAULTS.wordWrap);
  let showWhitespace = $state(DEFAULTS.showWhitespace);
  let highlightCurrentLine = $state(DEFAULTS.highlightCurrentLine);
  let showLineNumbers = $state(DEFAULTS.showLineNumbers);
  let rightMargin = $state(DEFAULTS.rightMargin);
  // Completion
  let autoPopup = $state(DEFAULTS.autoPopup);
  let popupDelayMs = $state(DEFAULTS.popupDelayMs);
  let caseSensitive = $state(DEFAULTS.caseSensitive);
  let autoImport = $state(DEFAULTS.autoImport);
  // Folding
  let foldingEnabled = $state(DEFAULTS.foldingEnabled);
  let foldBlockComments = $state(DEFAULTS.foldBlockComments);
  // Java Style
  let finalParams = $state(DEFAULTS.finalParams);
  let useLombokVal = $state(DEFAULTS.useLombokVal);
  let switchWithReturn = $state(DEFAULTS.switchWithReturn);
  let spaceInBraces = $state(DEFAULTS.spaceInBraces);
  let blankLineBetweenMembers = $state(DEFAULTS.blankLineBetweenMembers);
  // Java
  let defaultEncoding = $state<SourceEncoding>(DEFAULTS.defaultEncoding);
  let rebuildIndexOnOpen = $state(DEFAULTS.rebuildIndexOnOpen);
  let excludedDirs = $state(DEFAULTS.excludedDirs);

  /** Full snapshot — the shape a future `set_bennu_config` would persist. */
  function snapshot(): BennuSettingsSnapshot {
    return {
      fontSize, tabSize, indentStyle, wordWrap, showWhitespace,
      highlightCurrentLine, showLineNumbers, rightMargin,
      autoPopup, popupDelayMs, caseSensitive, autoImport,
      foldingEnabled, foldBlockComments,
      finalParams, useLombokVal, switchWithReturn, spaceInBraces, blankLineBetweenMembers,
      defaultEncoding, rebuildIndexOnOpen, excludedDirs,
    };
  }

  /** MOCK persistence — no-op today. Wire to `set_bennu_config(snapshot())` when
   *  the typed `[bennu]` config lands (rule 11). Every setter funnels here so the
   *  wiring is a one-line change. */
  function persist() {
    // MOCK — in-memory only. Future: void setBennuConfig(snapshot()).catch(() => {});
    void snapshot();
  }

  /** Parsed excluded-directory list (trimmed, empties dropped) — what a future
   *  indexer would consume. Kept as a derived helper so the consumer doesn't
   *  re-implement the comma split. */
  function excludedDirList(): string[] {
    return excludedDirs.split(',').map((s) => s.trim()).filter((s) => s.length > 0);
  }

  return {
    // ── Editor ────────────────────────────────────────────────────────────
    get fontSize() { return fontSize; },
    setFontSize(v: number) { fontSize = v; persist(); },
    get tabSize() { return tabSize; },
    setTabSize(v: number) { tabSize = v; persist(); },
    get indentStyle() { return indentStyle; },
    setIndentStyle(v: IndentStyle) { indentStyle = v; persist(); },
    get wordWrap() { return wordWrap; },
    setWordWrap(v: boolean) { wordWrap = v; persist(); },
    get showWhitespace() { return showWhitespace; },
    setShowWhitespace(v: boolean) { showWhitespace = v; persist(); },
    get highlightCurrentLine() { return highlightCurrentLine; },
    setHighlightCurrentLine(v: boolean) { highlightCurrentLine = v; persist(); },
    get showLineNumbers() { return showLineNumbers; },
    setShowLineNumbers(v: boolean) { showLineNumbers = v; persist(); },
    get rightMargin() { return rightMargin; },
    setRightMargin(v: number) { rightMargin = v; persist(); },

    // ── Completion ────────────────────────────────────────────────────────
    get autoPopup() { return autoPopup; },
    setAutoPopup(v: boolean) { autoPopup = v; persist(); },
    get popupDelayMs() { return popupDelayMs; },
    setPopupDelayMs(v: number) { popupDelayMs = v; persist(); },
    get caseSensitive() { return caseSensitive; },
    setCaseSensitive(v: boolean) { caseSensitive = v; persist(); },
    get autoImport() { return autoImport; },
    setAutoImport(v: boolean) { autoImport = v; persist(); },

    // ── Folding ───────────────────────────────────────────────────────────
    get foldingEnabled() { return foldingEnabled; },
    setFoldingEnabled(v: boolean) { foldingEnabled = v; persist(); },
    get foldBlockComments() { return foldBlockComments; },
    setFoldBlockComments(v: boolean) { foldBlockComments = v; persist(); },

    // ── Java Style (Generate output formatting) ────────────────────────────
    get finalParams() { return finalParams; },
    setFinalParams(v: boolean) { finalParams = v; persist(); },
    get useLombokVal() { return useLombokVal; },
    setUseLombokVal(v: boolean) { useLombokVal = v; persist(); },
    get switchWithReturn() { return switchWithReturn; },
    setSwitchWithReturn(v: boolean) { switchWithReturn = v; persist(); },
    get spaceInBraces() { return spaceInBraces; },
    setSpaceInBraces(v: boolean) { spaceInBraces = v; persist(); },
    get blankLineBetweenMembers() { return blankLineBetweenMembers; },
    setBlankLineBetweenMembers(v: boolean) { blankLineBetweenMembers = v; persist(); },

    // ── Java ──────────────────────────────────────────────────────────────
    get defaultEncoding() { return defaultEncoding; },
    setDefaultEncoding(v: SourceEncoding) { defaultEncoding = v; persist(); },
    get rebuildIndexOnOpen() { return rebuildIndexOnOpen; },
    setRebuildIndexOnOpen(v: boolean) { rebuildIndexOnOpen = v; persist(); },
    get excludedDirs() { return excludedDirs; },
    setExcludedDirs(v: string) { excludedDirs = v; persist(); },
    /** Parsed excluded-directory names (a future indexer's exclusion list). */
    get excludedDirList() { return excludedDirList(); },

    /** Full snapshot (future `set_bennu_config` payload). */
    snapshot,

    /** MOCK — restore every setting to its default. */
    resetToDefaults() {
      fontSize = DEFAULTS.fontSize;
      tabSize = DEFAULTS.tabSize;
      indentStyle = DEFAULTS.indentStyle;
      wordWrap = DEFAULTS.wordWrap;
      showWhitespace = DEFAULTS.showWhitespace;
      highlightCurrentLine = DEFAULTS.highlightCurrentLine;
      showLineNumbers = DEFAULTS.showLineNumbers;
      rightMargin = DEFAULTS.rightMargin;
      autoPopup = DEFAULTS.autoPopup;
      popupDelayMs = DEFAULTS.popupDelayMs;
      caseSensitive = DEFAULTS.caseSensitive;
      autoImport = DEFAULTS.autoImport;
      foldingEnabled = DEFAULTS.foldingEnabled;
      foldBlockComments = DEFAULTS.foldBlockComments;
      finalParams = DEFAULTS.finalParams;
      useLombokVal = DEFAULTS.useLombokVal;
      switchWithReturn = DEFAULTS.switchWithReturn;
      spaceInBraces = DEFAULTS.spaceInBraces;
      blankLineBetweenMembers = DEFAULTS.blankLineBetweenMembers;
      defaultEncoding = DEFAULTS.defaultEncoding;
      rebuildIndexOnOpen = DEFAULTS.rebuildIndexOnOpen;
      excludedDirs = DEFAULTS.excludedDirs;
      persist();
    },
  };
}

export const bennuSettingsStore = createSettingsStore();
