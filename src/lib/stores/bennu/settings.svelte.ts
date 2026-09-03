/**
 * Bennu settings store — the editable, user-facing preferences for the Java
 * editor window (font/tab size, completion behaviour, folding, Java indexing).
 *
 * Rune store — the single approved shape: private `$state`, a returned object of
 * getters + setters (see CLAUDE.md · "Store pattern").
 *
 * PARTLY config-backed — the fields that reach `…/bennu/config.toml` (everything the editing
 * surface and the completion popup read, the Java sources block, autosave, SQL dialect, local
 * history) persist for real; the rest — the folding-independent Java style flags, the popup's
 * remaining knobs — live in memory until the whole typed `[bennu]` section lands. This store is
 * the SEAM:
 * a field graduates by having its setter call `persistConfigBacked()` and `loadConfig()`
 * hydrate it, and the consumer surface (getters/setters) never moves, so
 * `BennuSettingsModal` never changes. Do NOT reach for localStorage for any of these
 * (rule 11).
 */

import { getBennuConfig, setBennuConfig } from '$lib/ipc/bennu/config';

/** How the editor turns a Tab press into whitespace. */
export type IndentStyle = 'spaces' | 'tabs';

/** Source encodings Bennu can decode Java files as. Cp1252 / ISO-8859-1 are the
 *  common legacy declarations found in older Maven poms. */
export const SOURCE_ENCODINGS = ['UTF-8', 'Cp1252', 'ISO-8859-1'] as const;
export type SourceEncoding = (typeof SOURCE_ENCODINGS)[number];

/** SQL dialects a `.sql` buffer can be **highlighted** as. `portable` (the default) uses the
 *  rules valid on both engines — the honest answer for a script nobody has classified, and the
 *  reason this is a setting rather than a detection: nothing in a `.sql` file under a Java
 *  project's resources says which engine it targets. */
export const SQL_DIALECTS = ['portable', 'oracle', 'postgres'] as const;
export type SqlDialectSetting = (typeof SQL_DIALECTS)[number];

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
  /** Draw inlay hints — argument names at call sites, and the type a `var` was inferred as. */
  inlayHints: boolean;
  /** Vertical margin guide column (IntelliJ's hard-wrap ruler). 0 = hidden. */
  rightMargin: number;
  /** Which SQL dialect `.sql` buffers are highlighted as. Config-backed. */
  sqlDialect: SqlDialectSetting;
  /** HTML files whose own scripts may run in the editor's preview — absolute paths, remembered
   *  across launches because the alternative is answering the same question about the same file
   *  every morning. Config-backed. */
  htmlScriptsAllowed: string[];
  /** Open `.md` files in the live-preview markdown editor rather than in the code editor.
   *  Config-backed; its control is the toggle in the editor's toolbar. */
  markdownLivePreview: boolean;
  /** Show Local History's diff as two columns rather than as a unified patch. Config-backed;
   *  its control is the toggle in that window, not a settings row. */
  historyDiffSplit: boolean;
  /** Autosave a modified buffer to disk (after a short idle, on tab switch, on window blur).
   *  Config-backed (persists to `…/bennu/config.toml`). */
  autosave: boolean;
  /** Keep a private record of what every project file used to be, so a save, a refactor or a
   *  delete can be undone long after the editor's own undo stack has moved on. Config-backed. */
  localHistory: boolean;
  /** Days of history to keep. Labelled revisions, and each file's newest one, are kept
   *  regardless. Config-backed. */
  localHistoryDays: number;
  /** Ceiling on one project's history, in megabytes. Config-backed. */
  localHistoryMaxMb: number;
  /** Files bigger than this (megabytes) are not recorded at all. Config-backed. */
  localHistoryMaxFileMb: number;
  /** Fold runs of library frames in the debugger's call stack into one expandable row.
   *  Config-backed — a preference about how you read a stack, not about a project. */
  collapseLibraryFrames: boolean;
  /** Offer the classes and files inside the DEPENDENCY jars in the Go-to navigator, as two
   *  extra categories. Config-backed. Off by default: it changes what the box is, and on a
   *  small project the classpath is noise around the answer. */
  searchDependencies: boolean;
  // Completion
  autoPopup: boolean;
  popupDelayMs: number;
  caseSensitive: boolean;
  autoImport: boolean;
  // Folding
  foldingEnabled: boolean;
  foldBlockComments: boolean;
  // Java Style (drives the Generate flow's output formatting)
  /** Java formatter: most consecutive blank lines kept between members. */
  javaBlankLines: number;
  /** Java formatter: indent the statements under a `case` label one level in from it. */
  javaIndentCaseBody: boolean;
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
  inlayHints: true,
  rightMargin: 120,
  sqlDialect: 'portable',
  htmlScriptsAllowed: [],
  markdownLivePreview: true,
  historyDiffSplit: true,
  autosave: true,
  localHistory: true,
  localHistoryDays: 7,
  localHistoryMaxMb: 256,
  localHistoryMaxFileMb: 4,
  collapseLibraryFrames: true,
  searchDependencies: false,
  autoPopup: true,
  popupDelayMs: 150,
  caseSensitive: false,
  autoImport: true,
  foldingEnabled: true,
  foldBlockComments: false,
  javaBlankLines: 1,
  javaIndentCaseBody: true,
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
  let inlayHints = $state(DEFAULTS.inlayHints);
  let rightMargin = $state(DEFAULTS.rightMargin);
  let sqlDialect = $state<SqlDialectSetting>(DEFAULTS.sqlDialect);
  let autosave = $state(DEFAULTS.autosave);
  let localHistory = $state(DEFAULTS.localHistory);
  let localHistoryDays = $state(DEFAULTS.localHistoryDays);
  let localHistoryMaxMb = $state(DEFAULTS.localHistoryMaxMb);
  let localHistoryMaxFileMb = $state(DEFAULTS.localHistoryMaxFileMb);
  let collapseLibraryFrames = $state(DEFAULTS.collapseLibraryFrames);
  let searchDependencies = $state(DEFAULTS.searchDependencies);
  // Completion
  let autoPopup = $state(DEFAULTS.autoPopup);
  let popupDelayMs = $state(DEFAULTS.popupDelayMs);
  let caseSensitive = $state(DEFAULTS.caseSensitive);
  let autoImport = $state(DEFAULTS.autoImport);
  // Folding
  let foldingEnabled = $state(DEFAULTS.foldingEnabled);
  let foldBlockComments = $state(DEFAULTS.foldBlockComments);
  // Java Style
  let javaBlankLines = $state(DEFAULTS.javaBlankLines);
  let javaIndentCaseBody = $state(DEFAULTS.javaIndentCaseBody);
  let finalParams = $state(DEFAULTS.finalParams);
  let useLombokVal = $state(DEFAULTS.useLombokVal);
  let switchWithReturn = $state(DEFAULTS.switchWithReturn);
  let spaceInBraces = $state(DEFAULTS.spaceInBraces);
  let blankLineBetweenMembers = $state(DEFAULTS.blankLineBetweenMembers);
  // Java
  let defaultEncoding = $state<SourceEncoding>(DEFAULTS.defaultEncoding);
  let rebuildIndexOnOpen = $state(DEFAULTS.rebuildIndexOnOpen);
  let excludedDirs = $state(DEFAULTS.excludedDirs);
  // Markdown + Local History
  let htmlScriptsAllowed = $state<string[]>([...DEFAULTS.htmlScriptsAllowed]);
  let markdownLivePreview = $state(DEFAULTS.markdownLivePreview);
  let historyDiffSplit = $state(DEFAULTS.historyDiffSplit);

  /** Keep the font size inside the range the settings stepper offers. A hand-edited
   *  `font_size = 2` in the config would otherwise render the settings modal itself
   *  unreadable, which is the one setting with no way back. */
  function clampFontSize(v: number): number {
    return Math.min(32, Math.max(8, Math.round(v) || DEFAULTS.fontSize));
  }

  /** Full snapshot — the shape a future `set_bennu_config` would persist. */
  function snapshot(): BennuSettingsSnapshot {
    return {
      fontSize, tabSize, indentStyle, wordWrap, showWhitespace,
      highlightCurrentLine, showLineNumbers, minimap, indentGuides, stickyScroll, inlayHints, rightMargin,
      sqlDialect, htmlScriptsAllowed, markdownLivePreview, historyDiffSplit, autosave,
      collapseLibraryFrames, searchDependencies,
      localHistory, localHistoryDays, localHistoryMaxMb, localHistoryMaxFileMb,
      autoPopup, popupDelayMs, caseSensitive, autoImport,
      foldingEnabled, foldBlockComments,
      javaBlankLines, javaIndentCaseBody,
      finalParams, useLombokVal, switchWithReturn, spaceInBraces, blankLineBetweenMembers,
      defaultEncoding, rebuildIndexOnOpen, excludedDirs,
    };
  }

  /** MOCK persistence — no-op today for the in-memory-only fields (folding, java-style, …).
   *  Wire to `set_bennu_config(snapshot())` when the whole typed `[bennu]` config lands (rule 11).
   *  The config-backed fields — every editor and completion preference, autosave, auto-import,
   *  SQL dialect, local history, the Java sources block — DON'T use this; see
   *  `persistConfigBacked`. */
  function persist() {
    // MOCK — in-memory only. Future: void setBennuConfig(snapshot()).catch(() => {});
    void snapshot();
  }

  /** Persist the config-backed fields — the editor and completion preferences, autosave,
   *  auto-import, SQL dialect, local history, the Java sources block — to
   *  `…/bennu/config.toml`. These are genuinely persisted (not mock). Read-modify-WRITE against
   *  the freshest config so a field another flow owns (build type, encoding, JDK paths, per-project
   *  overrides) is never clobbered. Fire-and-forget: a persistence hiccup must never block the UI. */
  async function persistConfigBacked() {
    try {
      const cur = await getBennuConfig();
      await setBennuConfig({
        ...cur,
        font_size: fontSize,
        word_wrap: wordWrap,
        show_whitespace: showWhitespace,
        show_line_numbers: showLineNumbers,
        highlight_current_line: highlightCurrentLine,
        folding_enabled: foldingEnabled,
        fold_block_comments: foldBlockComments,
        completion_auto_popup: autoPopup,
        completion_delay_ms: popupDelayMs,
        completion_case_sensitive: caseSensitive,
        default_encoding: defaultEncoding,
        excluded_dirs: excludedDirList(),
        html_scripts_allowed: htmlScriptsAllowed,
        markdown_live_preview: markdownLivePreview,
        history_diff_split: historyDiffSplit,
        autosave,
        auto_import: autoImport,
        // ⚠️ The indentation pair persists HERE and not through `persist()`: it is what the Java
        // formatter indents by, so a value that lived only in memory meant reformatting a file
        // with four spaces on Monday and with whatever the default was on Tuesday.
        indent_width: tabSize,
        indent_with_tabs: indentStyle === 'tabs',
        java_max_blank_lines: javaBlankLines,
        java_indent_case_body: javaIndentCaseBody,
        sql_dialect: sqlDialect,
        collapse_library_frames: collapseLibraryFrames,
        search_dependencies: searchDependencies,
        local_history: localHistory,
        local_history_days: localHistoryDays,
        local_history_max_mb: localHistoryMaxMb,
        local_history_max_file_mb: localHistoryMaxFileMb,
      });
    } catch {
      /* non-critical — the in-memory value still applies for this session */
    }
  }

  /** Parsed excluded-directory list (trimmed, empties dropped) — the shape the config keeps
   *  and the indexer's walk consumes. Kept as a helper so the box's comma-separated text is
   *  split in one place. */
  function excludedDirList(): string[] {
    return excludedDirs.split(',').map((s) => s.trim()).filter((s) => s.length > 0);
  }

  return {
    // ── Editor ────────────────────────────────────────────────────────────
    get fontSize() { return fontSize; },
    setFontSize(v: number) { fontSize = clampFontSize(v); void persistConfigBacked(); },
    get tabSize() { return tabSize; },
    setTabSize(v: number) { tabSize = v; void persistConfigBacked(); },
    get indentStyle() { return indentStyle; },
    setIndentStyle(v: IndentStyle) { indentStyle = v; void persistConfigBacked(); },
    get wordWrap() { return wordWrap; },
    setWordWrap(v: boolean) { wordWrap = v; void persistConfigBacked(); },
    get showWhitespace() { return showWhitespace; },
    setShowWhitespace(v: boolean) { showWhitespace = v; void persistConfigBacked(); },
    get highlightCurrentLine() { return highlightCurrentLine; },
    setHighlightCurrentLine(v: boolean) { highlightCurrentLine = v; void persistConfigBacked(); },
    get showLineNumbers() { return showLineNumbers; },
    setShowLineNumbers(v: boolean) { showLineNumbers = v; void persistConfigBacked(); },
    get minimap() { return minimap; },
    setMinimap(v: boolean) { minimap = v; persist(); },
    get indentGuides() { return indentGuides; },
    setIndentGuides(v: boolean) { indentGuides = v; persist(); },
    get stickyScroll() { return stickyScroll; },
    setStickyScroll(v: boolean) { stickyScroll = v; persist(); },
    get inlayHints() { return inlayHints; },
    setInlayHints(v: boolean) { inlayHints = v; persist(); },
    get rightMargin() { return rightMargin; },
    setRightMargin(v: number) { rightMargin = v; persist(); },
    get sqlDialect() { return sqlDialect; },
    setSqlDialect(v: SqlDialectSetting) { sqlDialect = v; void persistConfigBacked(); },
    get autosave() { return autosave; },
    setAutosave(v: boolean) { autosave = v; void persistConfigBacked(); },
    get localHistory() { return localHistory; },
    setLocalHistory(v: boolean) { localHistory = v; void persistConfigBacked(); },
    get localHistoryDays() { return localHistoryDays; },
    setLocalHistoryDays(v: number) { localHistoryDays = Math.max(1, v); void persistConfigBacked(); },
    get localHistoryMaxMb() { return localHistoryMaxMb; },
    setLocalHistoryMaxMb(v: number) { localHistoryMaxMb = Math.max(16, v); void persistConfigBacked(); },
    get localHistoryMaxFileMb() { return localHistoryMaxFileMb; },
    setLocalHistoryMaxFileMb(v: number) { localHistoryMaxFileMb = Math.max(1, v); void persistConfigBacked(); },
    get collapseLibraryFrames() { return collapseLibraryFrames; },
    get searchDependencies() { return searchDependencies; },
    async setSearchDependencies(v: boolean) {
      searchDependencies = v;
      await persistConfigBacked();
    },
    /** Awaited by its one caller, so a failed write shows up as the toggle not sticking rather
     *  than as a preference that quietly forgets itself on the next launch. */
    async setCollapseLibraryFrames(v: boolean) {
      collapseLibraryFrames = v;
      await persistConfigBacked();
    },

    // ── Completion ────────────────────────────────────────────────────────
    get autoPopup() { return autoPopup; },
    setAutoPopup(v: boolean) { autoPopup = v; void persistConfigBacked(); },
    get popupDelayMs() { return popupDelayMs; },
    setPopupDelayMs(v: number) { popupDelayMs = v; void persistConfigBacked(); },
    get caseSensitive() { return caseSensitive; },
    setCaseSensitive(v: boolean) { caseSensitive = v; void persistConfigBacked(); },
    get autoImport() { return autoImport; },
    setAutoImport(v: boolean) { autoImport = v; void persistConfigBacked(); },

    // ── Folding ───────────────────────────────────────────────────────────
    get foldingEnabled() { return foldingEnabled; },
    setFoldingEnabled(v: boolean) { foldingEnabled = v; void persistConfigBacked(); },
    get foldBlockComments() { return foldBlockComments; },
    setFoldBlockComments(v: boolean) { foldBlockComments = v; void persistConfigBacked(); },

    // ── Java Style (Generate output formatting) ────────────────────────────
    get javaBlankLines() { return javaBlankLines; },
    setJavaBlankLines(v: number) {
      javaBlankLines = Math.max(0, Math.min(5, Math.round(v) || 0));
      void persistConfigBacked();
    },
    get javaIndentCaseBody() { return javaIndentCaseBody; },
    setJavaIndentCaseBody(v: boolean) { javaIndentCaseBody = v; void persistConfigBacked(); },
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
    setDefaultEncoding(v: SourceEncoding) { defaultEncoding = v; void persistConfigBacked(); },
    get rebuildIndexOnOpen() { return rebuildIndexOnOpen; },
    setRebuildIndexOnOpen(v: boolean) { rebuildIndexOnOpen = v; persist(); },
    get excludedDirs() { return excludedDirs; },
    setExcludedDirs(v: string) { excludedDirs = v; void persistConfigBacked(); },
    /** Parsed excluded-directory names — what the indexer's walk is given. */
    get excludedDirList() { return excludedDirList(); },

    // ── Markdown / Local History ──────────────────────────────────────────
    // Neither is on a settings page: each one's control is the toggle in the surface it belongs
    // to. Kept here because that is where a config-backed preference lives, wherever it is set.
    /** Whether this file's scripts were allowed and remembered. */
    htmlScriptsRemembered(path: string): boolean {
      return htmlScriptsAllowed.includes(path);
    },
    /** Remember (or forget) this file's scripts. Forgetting is the half that has to exist:
     *  a permission you cannot take back is not a permission, it is a one-way door. */
    setHtmlScriptsRemembered(path: string, remember: boolean) {
      const has = htmlScriptsAllowed.includes(path);
      if (remember === has) return;
      htmlScriptsAllowed = remember
        ? [...htmlScriptsAllowed, path]
        : htmlScriptsAllowed.filter((p) => p !== path);
      void persistConfigBacked();
    },
    get markdownLivePreview() { return markdownLivePreview; },
    setMarkdownLivePreview(v: boolean) { markdownLivePreview = v; void persistConfigBacked(); },
    get historyDiffSplit() { return historyDiffSplit; },
    setHistoryDiffSplit(v: boolean) { historyDiffSplit = v; void persistConfigBacked(); },

    /** Full snapshot (future `set_bennu_config` payload). */
    snapshot,

    /** Hydrate the config-backed fields — the editor and completion preferences, autosave,
     *  auto-import, SQL dialect, local history, the Java sources block — from
     *  `…/bennu/config.toml`. Call once at window boot. The mock in-memory fields keep their
     *  defaults until the full store is wired to the config. */
    async loadConfig() {
      try {
        const cfg = await getBennuConfig();
        fontSize = clampFontSize(cfg.font_size ?? DEFAULTS.fontSize);
        wordWrap = cfg.word_wrap ?? DEFAULTS.wordWrap;
        showWhitespace = cfg.show_whitespace ?? DEFAULTS.showWhitespace;
        showLineNumbers = cfg.show_line_numbers ?? DEFAULTS.showLineNumbers;
        highlightCurrentLine = cfg.highlight_current_line ?? DEFAULTS.highlightCurrentLine;
        foldingEnabled = cfg.folding_enabled ?? DEFAULTS.foldingEnabled;
        foldBlockComments = cfg.fold_block_comments ?? DEFAULTS.foldBlockComments;
        autoPopup = cfg.completion_auto_popup ?? DEFAULTS.autoPopup;
        popupDelayMs = cfg.completion_delay_ms ?? DEFAULTS.popupDelayMs;
        caseSensitive = cfg.completion_case_sensitive ?? DEFAULTS.caseSensitive;
        // An unknown label from a hand-edited config would reach the BE as an encoding nothing
        // can decode with; the Select offers exactly these three.
        defaultEncoding = (SOURCE_ENCODINGS as readonly string[]).includes(cfg.default_encoding)
          ? (cfg.default_encoding as SourceEncoding)
          : DEFAULTS.defaultEncoding;
        // The box holds the list as the user typed it; the config holds it parsed.
        excludedDirs = (cfg.excluded_dirs ?? []).join(', ');
        htmlScriptsAllowed = cfg.html_scripts_allowed ?? [];
        markdownLivePreview = cfg.markdown_live_preview ?? DEFAULTS.markdownLivePreview;
        historyDiffSplit = cfg.history_diff_split ?? DEFAULTS.historyDiffSplit;
        autosave = cfg.autosave;
        localHistory = cfg.local_history ?? DEFAULTS.localHistory;
        localHistoryDays = cfg.local_history_days ?? DEFAULTS.localHistoryDays;
        localHistoryMaxMb = cfg.local_history_max_mb ?? DEFAULTS.localHistoryMaxMb;
        localHistoryMaxFileMb = cfg.local_history_max_file_mb ?? DEFAULTS.localHistoryMaxFileMb;
        collapseLibraryFrames = cfg.collapse_library_frames ?? DEFAULTS.collapseLibraryFrames;
        searchDependencies = cfg.search_dependencies ?? DEFAULTS.searchDependencies;
        autoImport = cfg.auto_import;
        tabSize = cfg.indent_width || DEFAULTS.tabSize;
        indentStyle = (cfg.indent_with_tabs ?? false) ? 'tabs' : 'spaces';
        javaBlankLines = cfg.java_max_blank_lines ?? DEFAULTS.javaBlankLines;
        javaIndentCaseBody = cfg.java_indent_case_body ?? DEFAULTS.javaIndentCaseBody;
        // An unknown / empty label from a hand-edited config falls back to the default rather
        // than reaching the editor as an undefined dialect.
        sqlDialect = (SQL_DIALECTS as readonly string[]).includes(cfg.sql_dialect)
          ? (cfg.sql_dialect as SqlDialectSetting)
          : DEFAULTS.sqlDialect;
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
      inlayHints = DEFAULTS.inlayHints;
      rightMargin = DEFAULTS.rightMargin;
      sqlDialect = DEFAULTS.sqlDialect;
      htmlScriptsAllowed = [];
      markdownLivePreview = DEFAULTS.markdownLivePreview;
      historyDiffSplit = DEFAULTS.historyDiffSplit;
      autosave = DEFAULTS.autosave;
      localHistory = DEFAULTS.localHistory;
      localHistoryDays = DEFAULTS.localHistoryDays;
      localHistoryMaxMb = DEFAULTS.localHistoryMaxMb;
      localHistoryMaxFileMb = DEFAULTS.localHistoryMaxFileMb;
      autoPopup = DEFAULTS.autoPopup;
      popupDelayMs = DEFAULTS.popupDelayMs;
      caseSensitive = DEFAULTS.caseSensitive;
      autoImport = DEFAULTS.autoImport;
      foldingEnabled = DEFAULTS.foldingEnabled;
      foldBlockComments = DEFAULTS.foldBlockComments;
      javaBlankLines = DEFAULTS.javaBlankLines;
      javaIndentCaseBody = DEFAULTS.javaIndentCaseBody;
      finalParams = DEFAULTS.finalParams;
      useLombokVal = DEFAULTS.useLombokVal;
      switchWithReturn = DEFAULTS.switchWithReturn;
      spaceInBraces = DEFAULTS.spaceInBraces;
      blankLineBetweenMembers = DEFAULTS.blankLineBetweenMembers;
      defaultEncoding = DEFAULTS.defaultEncoding;
      rebuildIndexOnOpen = DEFAULTS.rebuildIndexOnOpen;
      excludedDirs = DEFAULTS.excludedDirs;
      collapseLibraryFrames = DEFAULTS.collapseLibraryFrames;
      searchDependencies = DEFAULTS.searchDependencies;
      persist();
      void persistConfigBacked(); // the config-backed half also has to be written
    },
  };
}

export const bennuSettingsStore = createSettingsStore();
