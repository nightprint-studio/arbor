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

import { getBennuConfig, setBennuConfig } from '$lib/ipc/bennu/config';

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
  /** Show the right-gutter overview strip (diagnostic marks + hover preview) in place of the
   *  native scrollbar — the IntelliJ error stripe that replaces the old minimap. */
  minimap: boolean;
  /** Draw indentation guides (faint vertical lines per level, active block brightened). */
  indentGuides: boolean;
  /** Pin the enclosing declaration lines (class › method) to the top while scrolling. */
  stickyScroll: boolean;
  /** Vertical margin guide column (IntelliJ's hard-wrap ruler). 0 = hidden. */
  rightMargin: number;
  /** Autosave a modified buffer to disk (after a short idle, on tab switch, on window blur).
   *  Config-backed (persists to `…/bennu/config.toml`). */
  autosave: boolean;
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
  minimap: true,
  indentGuides: true,
  stickyScroll: true,
  rightMargin: 120,
  autosave: true,
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
  let minimap = $state(DEFAULTS.minimap);
  let indentGuides = $state(DEFAULTS.indentGuides);
  let stickyScroll = $state(DEFAULTS.stickyScroll);
  let rightMargin = $state(DEFAULTS.rightMargin);
  let autosave = $state(DEFAULTS.autosave);
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
      highlightCurrentLine, showLineNumbers, minimap, indentGuides, stickyScroll, rightMargin, autosave,
      autoPopup, popupDelayMs, caseSensitive, autoImport,
      foldingEnabled, foldBlockComments,
      finalParams, useLombokVal, switchWithReturn, spaceInBraces, blankLineBetweenMembers,
      defaultEncoding, rebuildIndexOnOpen, excludedDirs,
    };
  }

  /** MOCK persistence — no-op today for the in-memory-only fields (font, folding, java-style, …).
   *  Wire to `set_bennu_config(snapshot())` when the whole typed `[bennu]` config lands (rule 11).
   *  The config-backed toggles (autosave / auto-import) DON'T use this — see `persistConfigToggles`. */
  function persist() {
    // MOCK — in-memory only. Future: void setBennuConfig(snapshot()).catch(() => {});
    void snapshot();
  }

  /** Persist the config-backed toggles (autosave / auto-import) to `…/bennu/config.toml`. These two
   *  are genuinely persisted (not mock). Read-modify-WRITE against the freshest config so a field
   *  another flow owns (build type, encoding, JDK paths, per-project overrides) is never clobbered.
   *  Fire-and-forget: a persistence hiccup must never block the UI. */
  async function persistConfigToggles() {
    try {
      const cur = await getBennuConfig();
      await setBennuConfig({ ...cur, autosave, auto_import: autoImport });
    } catch {
      /* non-critical — the in-memory value still applies for this session */
    }
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
    get minimap() { return minimap; },
    setMinimap(v: boolean) { minimap = v; persist(); },
    get indentGuides() { return indentGuides; },
    setIndentGuides(v: boolean) { indentGuides = v; persist(); },
    get stickyScroll() { return stickyScroll; },
    setStickyScroll(v: boolean) { stickyScroll = v; persist(); },
    get rightMargin() { return rightMargin; },
    setRightMargin(v: number) { rightMargin = v; persist(); },
    get autosave() { return autosave; },
    setAutosave(v: boolean) { autosave = v; void persistConfigToggles(); },

    // ── Completion ────────────────────────────────────────────────────────
    get autoPopup() { return autoPopup; },
    setAutoPopup(v: boolean) { autoPopup = v; persist(); },
    get popupDelayMs() { return popupDelayMs; },
    setPopupDelayMs(v: number) { popupDelayMs = v; persist(); },
    get caseSensitive() { return caseSensitive; },
    setCaseSensitive(v: boolean) { caseSensitive = v; persist(); },
    get autoImport() { return autoImport; },
    setAutoImport(v: boolean) { autoImport = v; void persistConfigToggles(); },

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

    /** Hydrate the config-backed toggles (autosave / auto-import) from `…/bennu/config.toml`. Call
     *  once at window boot. The mock in-memory fields keep their defaults until the full store is
     *  wired to the config. */
    async loadConfig() {
      try {
        const cfg = await getBennuConfig();
        autosave = cfg.autosave;
        autoImport = cfg.auto_import;
      } catch {
        /* keep defaults — BE absent / not ready */
      }
    },

    /** Restore every setting to its default. The two config-backed toggles are also persisted. */
    resetToDefaults() {
      fontSize = DEFAULTS.fontSize;
      tabSize = DEFAULTS.tabSize;
      indentStyle = DEFAULTS.indentStyle;
      wordWrap = DEFAULTS.wordWrap;
      showWhitespace = DEFAULTS.showWhitespace;
      highlightCurrentLine = DEFAULTS.highlightCurrentLine;
      showLineNumbers = DEFAULTS.showLineNumbers;
      minimap = DEFAULTS.minimap;
      indentGuides = DEFAULTS.indentGuides;
      stickyScroll = DEFAULTS.stickyScroll;
      rightMargin = DEFAULTS.rightMargin;
      autosave = DEFAULTS.autosave;
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
      void persistConfigToggles(); // autosave / auto-import are config-backed
    },
  };
}

export const bennuSettingsStore = createSettingsStore();
