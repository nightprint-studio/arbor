<script lang="ts">
  /**
   * BennuEditor — the tabbed editor area.
   *
   * A JetBrains-style tab strip (shared `Tabs`) over the open files, a goto-line
   * overlay (Ctrl+G), a footer Ln/Col, and the shared `CodeEditor` with the Java
   * `LanguageDescriptor`. Files are read through the project store (which respects the
   * `bennu_read_file` encoding); diagnostics come from `bennu_diagnostics` (Phase 0
   * stub returns []).
   *
   * Imports only shared/ui + the shared code-editor core + bennu-local store/lang.
   */
  import {
    Hash, FileCode2, MapPin, Scissors, Copy, ClipboardPaste, Target, SearchCode,
    PenLine, Wand2, Save, Eye, X, ArrowRightToLine, LocateFixed, ShieldCheck, Plus, BookOpen,
    History,
    Braces, ArrowLeftRight, Package, FolderInput, CircleAlert, TriangleAlert, Check,
    DownloadCloud, FileDown, Variable, Database, Clock, Columns3, ListPlus, SquarePen,
    Languages, CaseSensitive,
    // The gutter's ▶ and the two other things pressing it might have meant.
    Play, Bug, SlidersHorizontal,
  } from 'lucide-svelte';
  import { tick, untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuImageView from './BennuImageView.svelte';
  import BennuDocxView from './BennuDocxView.svelte';
  import BennuFontView from './BennuFontView.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
  import { editorToolbarButtons, EDITOR_TOOLBAR_POINT } from '$lib/contributions/editor-toolbar';
  import { firePluginAction } from '$lib/ipc/plugin';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import { languageForPath } from './languages';
  import {
    isImageFile, isJavaFile as isJavaFileOf, isJspFile as isJspFileOf,
    isLspFile as isLspFileOf, isRustFile as isRustFileOf, isMarkdownFile,
    isRunnableScript, isHtmlFile, hasPushedDiagnostics, supportsCodeNav, supportsDiagnostics,
  } from './file-kind';
  import BennuHtmlPreview from './BennuHtmlPreview.svelte';
  import BennuTableInsert from './BennuTableInsert.svelte';
  import BennuHtmlScriptsModal from './BennuHtmlScriptsModal.svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import MarkdownEditor from '$lib/components/shared/ui/MarkdownEditor.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import {
    lspSemanticTokens, lspCodeActions, lspCodeLenses, lspExecuteCommand, lspExpandMacro,
    lspFolding, lspHighlights, lspLensLocations, lspSelectionRanges,
    lspSignatureHelp,
    type LspAction, type LspLens, type LspMacroExpansion, type LspSignature,
  } from '$lib/ipc/bennu/lsp';
  import { formatBuffer, optimizeImports } from '$lib/ipc/bennu/format';
  import {
    signatureHelp as ipcSignatureHelp, inlayHints as ipcInlayHints, type SignatureHelp,
  } from '$lib/ipc/bennu/hints';
  import type { DiagnosticSeverity, SourceEdit, TreeNode } from '$lib/types/bennu';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { ChevronDown } from 'lucide-svelte';
  // The tab strip draws the same icons the project tree does — the shared file-type set, and
  // the lettered ring for a Java type's kind.
  import IconifyIconView from '@iconify/svelte';
  import { getFileIcon } from '$lib/utils/file-icons';
  import SymbolKindIcon from './SymbolKindIcon.svelte';
  import { javaKindStore } from '$lib/stores/bennu/java-kinds.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { completionNoteStore } from '$lib/stores/bennu/completion-note.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';
  import { bennuMainClassStore } from '$lib/stores/bennu/main-classes.svelte';
  import { emptyInvocation as emptyCargoInvocation } from '$lib/ipc/bennu/cargo';
  import { bennuHistoryStore } from '$lib/stores/bennu/history.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuI18nStore } from '$lib/stores/bennu/i18n.svelte';
  import { isI18nBundle } from './i18n/bundle-path';
  import { markupEdit } from './i18n/markup-edit';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { diagnostics as ipcDiagnostics, projectTree as ipcProjectTree } from '$lib/ipc/bennu';
  import {
    definition as ipcDefinition, references as ipcReferences,
    actionUsages as ipcActionUsages,
    declaration as ipcDeclaration,
    jspNav as ipcJspNav,
    jspIncludeTarget as ipcJspIncludeTarget,
    beanClass as ipcBeanClass,
    mybatisNav as ipcMybatisNav,
    actionPropertyTarget as ipcActionPropertyTarget,
    actionPropertyLint as ipcActionPropertyLint,
    strutsResultTarget as ipcStrutsResultTarget,
    strutsResultLint as ipcStrutsResultLint,
    decompiledSource as ipcDecompiledSource,
    downloadSources as ipcDownloadSources,
    libraryDeclaration as ipcLibraryDeclaration,
    jspActions as ipcJspActions, setJspAction as ipcSetJspAction, type JspActionBinding,
    renameApply as ipcRenameApply,
  } from '$lib/ipc/bennu/nav';
  import {
    extNavigate, extHighlights, extGutter, extActions, extRefresh, springEnvVar, xmlFetchSchema,
    type ExtHighlight, type ExtGutterMark, type ExtTarget, type ExtAction, type EnvVarView,
  } from '$lib/ipc/bennu/ext';
  import { isSpringPropertyFile } from './spring-props-lang';
  import { isCargoManifest } from './cargo-toml-lang';
  import { cargoVersionHints, type CargoVersionHint } from '$lib/ipc/bennu/cargo';
  import {
    npmManifest, npmRunScript, npmVersionHints,
    type NpmScript, type NpmVersionHint,
  } from '$lib/ipc/bennu/npm';
  import { isPackageManifest } from './package-json-lang';
  import BennuEnvVarModal from './BennuEnvVarModal.svelte';
  import BennuMacroExpandModal from './BennuMacroExpandModal.svelte';
  import { makeByteToU16, makeU16ToByte } from '$lib/components/shared/ui/code-editor';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { decompiledStore } from '$lib/stores/bennu/decompiled.svelte';
  // The gutter's breakpoints and the paused line — both are the debugger's state seen from
  // the editor, and both are read-only here: the store owns them and persists them.
  import { bennuDebugStore, canonFile } from '$lib/stores/bennu/debug.svelte';
  // Which lines compile to bytecode — the gutter offers a breakpoint only on those.
  import { isFontFile, isWordFile, opensAsPreview } from '$lib/utils/preview-files';
  import { breakpointableLines } from './breakpoint-lines';
  import { buildDiagnosticsFor } from './build-diags';
  import { spellcheck as ipcSpellcheck, type SpellHit } from '$lib/ipc/bennu/spell';
  import { mojibakeCheck as ipcMojibakeCheck } from '$lib/ipc/bennu/mojibake';
  import {
    intentionsAt as ipcIntentionsAt, type IntentionOffer, type DiagRef,
  } from '$lib/ipc/bennu/intentions';
  import { createClass, refactorings, refactorPlan, type RefactorPlan } from '$lib/ipc/bennu/refactor';
  import { validationTarget as ipcValidationTarget } from '$lib/ipc/bennu/validation';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import type {
    EditorDiagnostic, EditorViewSnapshot, SemanticToken,
  } from '$lib/components/shared/ui/code-editor';
  import type { EditorView } from '@codemirror/view';
  import { bennuIntentionsStore } from '$lib/stores/bennu/intentions.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import { bennuHierarchyStore } from '$lib/stores/bennu/hierarchy.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuNavStore } from '$lib/stores/bennu/nav-history.svelte';
  import { bennuAstStore } from '$lib/stores/bennu/ast.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { collectIntentions, type GenerateMode, type IntentionItem } from './bennu-intentions';
  import { javaOutline } from './java-outline';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  // Path identity is one function, not one per component: the BE speaks forward slashes and the
  // FE carries native separators, so a `===` between them is quietly false on Windows — and the
  // bug it causes is always a silent fallback rather than a visible error.
  import { baseName, isSamePath } from '$lib/utils/paths';

  let {
    /** Open the Generate modal in `mode` (routed to the window's BennuGenerateModal).
     *  Passed down so the Alt+Enter "Generate…" intentions can trigger it. */
    onGenerate,
    onOverride,
  }: {
    onGenerate?: (mode: GenerateMode) => void;
    /** Alt+Enter → "Implement / override methods": the window hosts the picker, so the offer is
     *  relayed rather than handled here. */
    onOverride?: () => void;
  } = $props();

  type EditorController = {
    focus: () => void;
    /** Ask for completions at the caret — the explicit request. */
    requestCompletion: () => boolean;
    getValue: () => string;
    getSelectionText: () => string;
    openSearch: () => void;
    scrollToLineCol: (line: number, col?: number) => void;
    scrollToByteOffset: (byteOffset: number) => void;
    replaceByteRange: (startByte: number, endByte: number, text: string) => void;
    coordsAtCaret: () => { x: number; y: number } | null;
    coordsAtByteOffset: (byteOffset: number) => { x: number; y: number } | null;
    setCaretAtCoords: (x: number, y: number) => boolean;
    wordAtCaret: () => string | null;
    refAtCaret: () => string | null;
    caretByteOffset: () => number;
    /** Select a byte range — what a click in the syntax-tree panel does over here. */
    selectByteRange: (startByte: number, endByte: number) => void;
    /** The primary selection, in UTF-16 document offsets. */
    selectionRange: () => { from: number; to: number; head: number; empty: boolean };
    /** The primary selection in UTF-8 bytes — the frame every backend span is in. */
    selectionByteRange: () => { start: number; end: number; empty: boolean };
    /** Several byte-range replacements as ONE undo step — a formatter's edit list. */
    replaceByteRanges: (
      edits: readonly { startByte: number; endByte: number; text: string }[],
    ) => number;
    /** Replace the semantic-highlight layer (byte spans → token marks). */
    setSemanticTokens: (tokens: SemanticToken[]) => void;
    /** Replace the occurrence-highlight layer — where else the symbol under the caret appears. */
    setDocumentHighlights: (
      spans: readonly { start: number; end: number; kind: string }[],
    ) => void;
    /** Replace the fold ranges a provider supplied. */
    setFoldRanges: (
      ranges: readonly { start: number; end: number; placeholder?: string }[],
    ) => void;
    /** Replace the inlay hints — the text drawn between the code, never in it. */
    setInlayHints: (
      hints: readonly { offset: number; label: string; before?: boolean }[],
    ) => void;
    /** Show the parameter-hint strip for the call the caret is inside, or clear it with `null`. */
    setSignatureHint: (
      info: {
        label: string;
        params: readonly { start: number; end: number }[];
        active: number;
        anchor: number;
        doc?: string | null;
        overload?: { index: number; count: number } | null;
      } | null,
    ) => void;
    /** Replace the code lenses drawn above the items of the buffer. */
    setCodeLenses: (
      lenses: readonly {
        start: number;
        title: string;
        actionable: boolean;
        key: number;
        tone?: 'muted' | 'accent';
      }[],
    ) => void;
    /** Select a byte range WITHOUT scrolling — what expand-selection needs, since it grows a range
     *  the caret is already inside. */
    setSelectionBytes: (startByte: number, endByte: number) => void;
    insertAtCursor: (text: string) => void;
    copySelection: () => void;
    cutSelection: () => void;
    pasteClipboard: () => void;
  };
  let editorComp = $state<EditorController | null>(null);

  const activePath = $derived(projectStore.activeFilePath);
  const openPaths = $derived(projectStore.openFilePaths);
  /**
   * The open tab is an image, so this area shows a preview instead of an editor.
   *
   * Checked here rather than deeper down because it changes what the whole area *is*: there is no
   * document, so the toolbar's actions, the breadcrumb, the diagnostics badge and the Ln/Col footer
   * all have nothing to say. A `CodeEditor` mounted on an image would be an empty buffer whose
   * every keystroke is an edit to a file that has no text.
   */
  const isImageTab = $derived(isImageFile(activePath));
  const isDocxTab = $derived(isWordFile(activePath));
  const isFontTab = $derived(isFontFile(activePath));
  /**
   * A markdown document, which opens **rendered**.
   *
   * Unlike the three above it still has a buffer — it is edited, saved and made dirty like any
   * other file, and the live preview edits it in place (the markup is revealed on the line the
   * caret is on). What changes is only which editor is mounted, so the toolbar, the tab strip
   * and Ctrl+S all keep working. The toggle beside them mounts the code editor instead, for
   * when the markup itself is the thing being worked on.
   */
  const isMarkdownTab = $derived(isMarkdownFile(activePath));
  const markdownLive = $derived(isMarkdownTab && bennuSettingsStore.markdownLivePreview);
  /** A tab with no buffer behind it — a viewer, not an editor. The toolbar and the caret
   *  footer are both about a document, so neither belongs above or below one. Keyed on the
   *  shared predicate rather than on the kinds listed above, because the last kind added
   *  (`.docx`) got its viewer and kept the toolbar — two rows naming the same file. */
  const isPreviewTab = $derived(opensAsPreview(activePath));

  // Per-tab cursor + scroll, so switching away and back restores where you left off.
  // The editor remounts on tab switch ({#key activePath}); it emits `onViewState` while
  // a tab is active and reads `initialState` for the returning tab from this map. The snapshot
  // carries the tab's undo/redo HISTORY as well as its cursor and scroll — a remount builds a
  // fresh CodeMirror state, so without it everything you had typed in a file you came back to
  // was still there and none of it was undoable.
  const viewStates = new Map<string, EditorViewSnapshot>();
  // A closed tab's snapshot goes with it. The history is the largest thing in there, and it is
  // bounded by the tabs you have open rather than by everything you have opened this session.
  $effect(() => {
    const open = new Set(projectStore.openFilePaths);
    for (const path of [...viewStates.keys()]) {
      if (!open.has(path)) viewStates.delete(path);
    }
  });
  // The editor language for the active file — Java (tree-sitter) or a CodeMirror
  // built-in / legacy mode (XML, JSP, YAML, JSON, …) picked by extension.
  const editorLanguage = $derived(languageForPath(activePath));
  // Struts validation files get a dedicated editor toolbar (the "New validator" flow).
  const isValidationFile = $derived(activePath?.toLowerCase().endsWith('-validation.xml') ?? false);
  // Emmet Tab-expansion is markup-only: JSP + HTML (where the abbreviations pay off).
  const emmetEnabled = $derived(!!activePath && /\.(jsp|jspf|tag|html?|xhtml)$/i.test(activePath));


  /** The workspace project a path belongs to (longest-prefix root match), or undefined when
   *  it's outside every workspace project — for badging a "foreign" tab with its owner. */
  function owningName(path: string): string | undefined {
    const canon = path.replace(/\\/g, '/').toLowerCase();
    let best: { root: string; name: string } | undefined;
    for (const p of projectStore.workspaceProjects) {
      const r = p.root.toLowerCase();
      if (canon === r || canon.startsWith(r.endsWith('/') ? r : `${r}/`)) {
        if (!best || p.root.length > best.root.length) best = p;
      }
    }
    return best?.name;
  }

  // A tab whose file isn't under the active project's root is "foreign" (opened from another
  // workspace project): badge it with the owning project's name so it's clear where it lives.
  //
  // An **external change** to a file with unsaved edits outranks that badge. It is the only
  // tab state that means "this file needs a decision from you": autosave is paused for it, so
  // without a mark on the tab the file would just quietly stop saving. The tooltip says so
  // rather than leaving the badge to be guessed at.
  const tabs = $derived<TabItem[]>(
    openPaths.map((p) => {
      const foreign = projectStore.isForeign(p);
      const conflicted = projectStore.isConflicted(p);
      const from = foreign ? `  ·  from ${owningName(p) ?? 'another project'}` : '';
      // The same icon the tree shows for this file — a Java type's kind as a lettered ring,
      // anything else as its file-type icon. A tab strip where every tab wore the identical
      // generic glyph was spending the space without answering anything with it.
      const java = p.toLowerCase().endsWith('.java');
      return {
        id: p,
        label: baseName(p),
        icon: java ? SymbolKindIcon : IconifyIconView,
        iconProps: java
          ? { kind: javaKindStore.kindOf(p) }
          : { icon: getFileIcon(baseName(p)), width: 14, height: 14 },
        iconSize: 13,
        // Not your file: a dependency's source or a file owned by another project. Tinted in
        // both states — the badge is read once and then stops being noticed, while "edits
        // here go nowhere" is true every time you glance at the strip.
        tone: foreign ? ('external' as const) : undefined,
        title: conflicted
          ? `${p}${from}  ·  changed on disk — autosave paused until you choose a version`
          : `${p}${from}`,
        badge: conflicted ? 'disk' : foreign ? (owningName(p) ?? 'ext') : undefined,
      };
    }),
  );

  // ── Caret position (footer, via the UI store) ────────────────────────────────
  let caretLine = $state(1);
  let caretCol = $state(1);

  // ── Navigation history (Ctrl+Alt+←/→) ─────────────────────────────────────────
  //
  // Record a "place" when the caret makes a real JUMP — a different file, or a big in-file hop
  // (a go-to / structure / find click) — not on every arrow keystroke.
  //
  // The hard part is that **one navigation is several caret events**. Every cross-file jump in
  // Bennu is `openFile(…)` and then `requestGoto(line)`: the open lands the buffer wherever it
  // starts, the scroll follows. Treating those as two places is what put stops in the ring
  // nobody ever visited — Back from a go-to took you to line 1 of the file you had just arrived
  // in, and a Back that crossed files recorded its own landing, truncating the branch and
  // killing Forward. So two pieces of state below: which jump we are still waiting to land, and
  // whether the entry we last pushed was a file opening that the next hop should refine.
  let lastNav: { file: string; line: number } | null = null;
  /** The landing of a programmatic Back/Forward, with a budget of buffer events to ignore
   *  before giving up on it — so a jump that can never land (a shorter file, a navigation the
   *  user superseded) cannot wedge the history shut. */
  let pendingJump: { file: string; line: number; budget: number } | null = null;
  /** The file whose OPENING we just recorded. The next jump inside it refines that entry
   *  instead of pushing a second one. Consumed by the first caret event either way. */
  let justOpened: string | null = null;
  const NAV_JUMP_LINES = 3; // an in-file move larger than this counts as a jump
  const NAV_SETTLE_EVENTS = 4; // buffer events a pending jump may swallow before it gives up

  function onCaret(line: number, col: number) {
    caretLine = line; caretCol = col;
    bennuUiStore.setCaret(line, col);

    // Occurrence highlighting keys off this, through a `$state` rather than a direct call so the
    // effect owns the debounce and the cancellation in one place.
    if (editorComp) highlightCaret = editorComp.caretByteOffset();

    // An expand-selection run describes one document state and one starting caret. Moving the caret
    // any other way ends it — otherwise the next press would apply a link computed for a position
    // the caret has left, and select the wrong text.
    if (!steppingSelection) forgetSelectionChain();

    // The other direction of the syntax-tree panel: open it down to whatever the caret is in.
    // This is what makes it a reading tool — you point at the construct you do not understand
    // and it says what the grammar called it.
    if (bennuUiStore.rightPanel === 'ast' && editorComp) {
      void bennuAstStore.revealAt(editorComp.caretByteOffset());
    }

    const path = activePath;
    if (!path) return;

    // Remembered across restarts (debounced hard in the store — this runs on every arrow key).
    // The live `viewStates` snapshot below is finer while the window is open; this is the part
    // that survives closing it.
    projectStore.rememberCaret(path, line, col);

    if (pendingJump) {
      const arrived =
        isSamePath(pendingJump.file, path) && Math.abs(pendingJump.line - line) <= 1;
      if (!arrived && pendingJump.budget > 0) {
        // On the way: the buffer being swapped, the target file opening at wherever it starts.
        // The old file can report a last caret position too, before `activePath` catches up —
        // hence swallowing by budget rather than by which file this event is in.
        pendingJump.budget -= 1;
        return;
      }
      pendingJump = null;
      if (arrived) {
        // Already in the ring at the index we just stepped to. Recording it again would
        // truncate the branch this step moved into — which is why Forward stopped working
        // after any Back that crossed a file.
        lastNav = { file: path, line };
        justOpened = null;
        return;
      }
      // Never landed, or the user went somewhere else meanwhile: fall through and treat this
      // as an ordinary event rather than blocking the history for the rest of the session.
    }

    const opened = justOpened;
    justOpened = null;
    const changedFile = !lastNav || !isSamePath(lastNav.file, path);
    const movedFar = !!lastNav && Math.abs(lastNav.line - line) > NAV_JUMP_LINES;
    const jumped = changedFile || movedFar;
    if (jumped) {
      const place = { file: path, line, col };
      if (opened && isSamePath(opened, path)) {
        bennuNavStore.replace(place); // the scroll that the opening was for
      } else {
        bennuNavStore.record(place);
        // Only a file OPENING is provisional. Two deliberate hops inside one file are two
        // stops, and collapsing them would lose the one you meant to come back to.
        justOpened = changedFile ? path : null;
      }
    }
    lastNav = { file: path, line };
  }

  /** Navigate to a recorded place (cross-file via the goto relay so the remounted editor
   *  picks it up on mount; same-file directly). `pendingJump` keeps every caret event this
   *  causes — the opening as much as the landing — out of the history. */
  async function navGo(place: { file: string; line: number; col: number } | null) {
    if (!place) return;
    pendingJump = { file: place.file, line: place.line, budget: NAV_SETTLE_EVENTS };
    if (!isSamePath(place.file, projectStore.activeFilePath)) {
      await projectStore.openFile(place.file);
      bennuUiStore.requestGoto(place.line);
    } else {
      editorComp?.scrollToLineCol(place.line, place.col);
    }
  }
  /** Ctrl+Alt+← — jump back to the previous place in the navigation history. */
  export function navBack() { void navGo(bennuNavStore.back()); }
  /** Ctrl+Alt+→ — jump forward again after a Back. */
  export function navForward() { void navGo(bennuNavStore.forward()); }

  // ── Restore the caret a restart lost ─────────────────────────────────────────
  //
  // `viewStates` restores a tab you switched away from, but it dies with the window: reopening
  // Bennu brought the tabs back and put every one of them at the top of the file. The store
  // remembers a line/col per tab in the persisted session, and this places it.
  //
  // It waits for the BUFFER, not just for the tab. On a restored session the active file's text
  // is fetched *after* `activeFilePath` is set, so the editor mounts empty — and a jump to line
  // 400 of an empty document lands on line 1 and stays there once the text arrives. Reading
  // `sourceOf` makes this re-run when the text lands, which is the moment the line exists.
  //
  // Once per tab activation: a live view state (a tab you have already been in) is the finer
  // answer, and a go-to that asked for a specific line outranks both — hence the relays below
  // claim the tab through this same flag.
  let restoredCaretFor: string | null = null;
  $effect(() => {
    const path = activePath;
    const source = path ? projectStore.sourceOf(path) : '';
    if (!path || !source || isPreviewTab) return;
    if (restoredCaretFor === path || viewStates.has(path)) return;
    restoredCaretFor = path;
    const caret = projectStore.caretOf(path);
    if (!caret || (caret.line === 1 && caret.col === 1)) return;
    void tick().then(() => {
      if (projectStore.activeFilePath !== path) return;
      editorComp?.scrollToLineCol(caret.line, caret.col);
    });
  });

  // ── Goto relays: Structure / Outline / Problems / Find request a jump; scroll there. ──
  //
  // **One request, one jump.** These used to read `editorComp` inside the tracked scope, and
  // `editorComp` is `$state` that changes on every tab switch (the editor remounts under
  // `{#key activePath}`). So the LAST go-to re-fired on every switch and dragged the freshly
  // activated tab to that stale line — which is what "switching tabs loses the cursor position"
  // actually was: the restored caret was not lost, it was overwritten a frame later.
  //
  // The fix is to depend on the request alone and consume its nonce, then perform the jump after
  // `tick()`. The wait is load-bearing rather than cosmetic: a CROSS-FILE go-to is
  // `openFile(f)` followed by `requestGoto(line)`, so at the instant the effect runs the mounted
  // editor may still be the OLD file's. After the flush, `editorComp` is the one for the file the
  // jump was asked about.
  let consumedGotoNonce = 0;
  $effect(() => {
    const t = bennuUiStore.gotoTarget;
    if (!t || t.nonce === consumedGotoNonce) return;
    consumedGotoNonce = t.nonce;
    // A go-to names the line; the remembered caret must not overrule it when the buffer lands.
    restoredCaretFor = projectStore.activeFilePath;
    void tick().then(() => editorComp?.scrollToLineCol(t.line, 1));
  });

  // ── Goto-by-byte-offset relay: the Forms tool window requests a jump to a `<form>`
  //    tag / field-name byte span; move the caret there and reveal it. ──
  let consumedGotoOffsetNonce = 0;
  $effect(() => {
    const t = bennuUiStore.gotoOffsetTarget;
    if (!t || t.nonce === consumedGotoOffsetNonce) return;
    consumedGotoOffsetNonce = t.nonce;
    restoredCaretFor = projectStore.activeFilePath;
    void tick().then(() => editorComp?.scrollToByteOffset(t.offset));
  });

  // ── Edits → store ────────────────────────────────────────────────────────────
  /** Bumped on every edit — what the document-keyed effects (inlay hints) depend on.
   *  A counter rather than the text: they re-read the buffer themselves, and depending on a
   *  megabyte string would re-run them on a change that produced the same text. */
  let docRevision = $state(0);

  function onInput(text: string) {
    if (activePath) projectStore.setSource(activePath, text);
    docRevision += 1;
  }

  // ── The markdown mount ───────────────────────────────────────────────────────
  //
  // The live-preview editor takes its document once, at mount, and keys off `docKey` — it has
  // no controlled `value` the way `CodeEditor` does, because a preview that re-read the buffer
  // on every keystroke would re-render the document under the caret. So the two ways the text
  // can change are told apart here: an edit *from* the editor is already in the store and must
  // not remount it; a change from anywhere else (a reload from disk, a revert, a plugin writing
  // the file) must.
  let mdRemounts = $state(0);
  /** The text the mounted markdown editor is known to hold — what it last emitted, or what it
   *  was mounted with. */
  let mdMountedText: string | null = null;
  /** The file that mount belongs to, so the first render of a tab is not read as a change. */
  let mdMountedPath: string | null = null;
  const mdDocKey = $derived(`${activePath ?? ''}#${mdRemounts}`);
  /** The live-preview editor, for the one thing only it can do: land on a heading in a file
   *  that a link has just opened. Typed by what is asked of it rather than by the component,
   *  so this stays a contract and not a handle to poke at. */
  let mdComp = $state<{ goToAnchor(slug: string): boolean } | null>(null);

  /**
   * Every file in the project, for the link completion behind `[…](`.
   *
   * Read once per project rather than from `projectStore.tree`: that one is the sidebar's, and
   * the sidebar expands folders lazily — completing only inside the folders you happened to
   * click open is the kind of half-answer that teaches people the feature doesn't work.
   */
  let mdFiles = $state<string[]>([]);
  const projectFileIndex = () => mdFiles;

  $effect(() => {
    const root = markdownLive ? projectStore.project?.root : null;
    if (!root) return;
    let live = true;
    void ipcProjectTree(root)
      .then((tree) => {
        if (live) mdFiles = flattenFiles(tree);
      })
      .catch(() => {
        // A completion list is not worth a toast: the headings still complete.
      });
    return () => {
      live = false;
    };
  });

  function flattenFiles(node: TreeNode, into: string[] = []): string[] {
    if (node.is_dir) for (const child of node.children) flattenFiles(child, into);
    else into.push(node.path);
    return into;
  }

  $effect(() => {
    if (!markdownLive || !activePath) return;
    const path = activePath;
    const text = projectStore.sourceOf(path); // tracked: the store's copy
    // Writes to state from inside an effect — untracked, or this re-enters itself.
    untrack(() => {
      if (mdMountedPath !== path) {
        // First render of this tab: the editor is mounting with exactly this text, so there is
        // nothing to remount for. Bumping here would tear down the mount that just happened.
        mdMountedPath = path;
        mdMountedText = text;
        return;
      }
      if (text === mdMountedText) return; // our own edit, already in the editor
      mdMountedText = text;
      mdRemounts += 1;
    });
  });

  function onMarkdownInput(text: string) {
    mdMountedText = text;
    onInput(text);
  }

  // ── HTML preview ────────────────────────────────────────────────────────────
  //
  // Rendering asks nothing. The frame is sandboxed with no origin of its own, so a page that
  // only lays itself out cannot touch anything — and a dialog in front of every preview is a
  // dialog that gets dismissed without being read, which is worse than none.
  //
  // **Running the page's own scripts** is the decision, and it is asked once per file. The answer
  // can be kept for the session or remembered across launches (`bennuSettingsStore`), because a
  // report you open every morning should not ask every morning — and it can be taken back from
  // the preview's own bar, because a permission with no way out is not a permission.
  const isHtmlTab = $derived(isHtmlFile(activePath));
  /** Files previewing right now. Per session and per file: it is a view state, not a setting. */
  const htmlPreviewOpen = new SvelteSet<string>();
  /** Scripts allowed for this session only — the "just this once" answer. The remembered ones
   *  live in the config; this set is what the two are unioned from. */
  const htmlScriptsOnce = new SvelteSet<string>();
  /** The file whose scripts dialog is open. */
  let htmlAsk = $state<string | null>(null);
  let htmlFullscreen = $state(false);

  const htmlPreviewing = $derived(!!activePath && isHtmlTab && htmlPreviewOpen.has(activePath));
  const htmlScriptsOn = $derived(
    !!activePath
      && (htmlScriptsOnce.has(activePath) || bennuSettingsStore.htmlScriptsRemembered(activePath)),
  );

  // A tab switch leaves the enlarged preview behind: it belongs to the page you were looking at,
  // and the next file opening full-window because the last one did would be a surprise.
  $effect(() => {
    void activePath;
    untrack(() => { htmlFullscreen = false; });
  });

  /** The bar's one switch. On asks; off is immediate and forgets both answers — taking a
   *  permission back has to be as cheap as it was to give, or nobody does it. */
  function toggleHtmlScripts(next: boolean) {
    const path = activePath;
    if (!path) return;
    if (next) { htmlAsk = path; return; }
    htmlScriptsOnce.delete(path);
    bennuSettingsStore.setHtmlScriptsRemembered(path, false);
  }

  function allowHtmlScripts(remember: boolean) {
    const path = htmlAsk;
    htmlAsk = null;
    if (!path) return;
    if (remember) bennuSettingsStore.setHtmlScriptsRemembered(path, true);
    else htmlScriptsOnce.add(path);
  }

  /**
   * Write an empty `rows × cols` table into the buffer, under the caret's line.
   *
   * Through the store rather than through the editor: the markdown mount takes its document once
   * and is re-mounted when the store's text changes from outside (see `mdMountedText`), which is
   * exactly what an insertion from the toolbar is. The table lands on its own paragraph — a
   * table glued to the line above it is not a table to the parser.
   */
  function insertMarkdownTable(rows: number, cols: number) {
    const path = activePath;
    if (!path) return;
    const line = (cells: string) => `| ${Array(cols).fill(cells).join(' | ')} |`;
    const table = [
      line('   '),
      `| ${Array(cols).fill('---').join(' | ')} |`,
      ...Array.from({ length: rows }, () => line('   ')),
    ].join('\n');
    const source = projectStore.sourceOf(path);
    const sep = source.length === 0 || source.endsWith('\n\n') ? '' : source.endsWith('\n') ? '\n' : '\n\n';
    onMarkdownInput(`${source}${sep}${table}\n`);
  }

  /**
   * A link in a rendered markdown document pointed at a file: open it in a tab.
   *
   * The editor has already resolved it against the document's own directory, so what arrives is
   * an absolute path. A file outside the project opens all the same — a README linking to a
   * sibling repository's notes is a normal thing to write, and refusing it would only send the
   * reader to a file manager.
   */
  function openMarkdownLink(path: string, anchor: string | null) {
    void projectStore
      .openFile(path)
      .then(() => {
        if (anchor) jumpToAnchorWhenReady(path, anchor);
      })
      .catch(() => {
        toastStore.show(`Could not open ${path.split(/[\\/]/).pop() ?? path}`, 'info');
      });
  }

  /**
   * The `#uso` half of a `guida.md#uso` link, once the file it names is the open one.
   *
   * The editor that saw the click is about to be replaced by the one for the file being opened,
   * so there is nothing to ask until that one exists. Rather than guess how many frames a mount
   * takes, ask every frame until the jump lands — it stops on the first success, and gives up
   * after about a third of a second, which is the honest answer for a heading that isn't there.
   */
  function jumpToAnchorWhenReady(path: string, slug: string, tries = 20) {
    const want = path.replace(/\\/g, '/');
    requestAnimationFrame(() => {
      if ((activePath ?? '').replace(/\\/g, '/') === want && mdComp?.goToAnchor(slug)) return;
      if (tries > 0) jumpToAnchorWhenReady(path, slug, tries - 1);
    });
  }

  /** The caret, for the footer. Deliberately not `onCaret`: that one drives occurrence
   *  highlighting, the navigation history and the expand-selection run, all of which read
   *  `editorComp` — which is the code editor, and is not mounted here. */
  function onMarkdownCaret(line: number, col: number) {
    caretLine = line;
    caretCol = col;
    bennuUiStore.setCaret(line, col);
  }

  // ── The syntax-tree panel, both directions ───────────────────────────────────
  /** Only while the panel is open. A tree costs a round trip per pause in typing, and one
   *  computed for a panel nobody is looking at is a round trip spent on nothing. */
  const astOpen = $derived(bennuUiStore.rightPanel === 'ast');

  // Fed from the STORE's copy of the buffer rather than from `onInput`, so switching files
  // re-parses too — an editor that only reacted to typing would keep showing the previous
  // file's tree until you touched this one.
  $effect(() => {
    const path = activePath;
    if (!astOpen || !path) {
      if (!astOpen) bennuAstStore.clear();
      return;
    }
    bennuAstStore.follow(projectStore.sourceOf(path) ?? '', path);
  });

  // A node was clicked over there: select its bytes here. Keyed on the request's timestamp so
  // clicking the same node twice still re-selects — the second click means "show me again".
  let lastAstSelect = 0;
  $effect(() => {
    const req = bennuAstStore.selectRequest;
    if (!req || req.at === lastAstSelect) return;
    lastAstSelect = req.at;
    // After the flush, like the go-to relays above — and for the same reason: the editor that
    // has to perform the selection may not be mounted yet at the moment the request lands.
    void tick().then(() => editorComp?.selectByteRange(req.start, req.end));
  });

  // ── Diagnostics (byte spans) from the backend, re-fetched per active file ─────
  // For a JSP the backend extracts + checks the `action="…"` refs (unknown → warning
  // squiggle). Re-fetched when the index rebuilds too (`buildRevision`), so squiggles
  // appear once the config graph finishes building after a fresh open.
  let diags = $state<EditorDiagnostic[]>([]);

  /**
   * The backend's severity as CodeMirror understands it.
   *
   * `weak` — a style finding, which is what a naming-convention violation is — has no CodeMirror
   * equivalent, so it is drawn with the softest level the lint gutter has. The distinction is not
   * lost: the wire value reaches the Problems panel unchanged, which groups it on its own. Mapping
   * here rather than widening `shared/ui/code-editor` keeps that widget's severities 1:1 with
   * CodeMirror's, which is the promise it makes to every other product.
   */
  function cmSeverity(s: DiagnosticSeverity): EditorDiagnostic['severity'] {
    return s === 'weak' ? 'hint' : s;
  }

  $effect(() => {
    const path = activePath;
    void bennuIndexStore.buildRevision; // re-run when the index (config graph) rebuilds
    // …and when the project's naming conventions are saved, so a rule change repaints on the next
    // debounce rather than on the next time the file is reopened.
    void bennuNamingStore.revision;
    if (!path) { diags = []; return; }
    // Only the files an analyzer understands are validated. Asking about a `.rs` / `.dig` /
    // `.toml` buffer would hand it to the Java validator once per keystroke, for an answer
    // that can only ever be empty.
    if (!supportsDiagnostics(path)) { diags = []; return; }
    // Java files validate the LIVE buffer — track the source so the check re-runs on edit,
    // debounced so a burst of keystrokes coalesces. JSP checks read the file on the backend, so
    // they don't depend on the buffer.
    const isJava = /\.java$/i.test(path);
    // The live buffer goes with the request for Java AND for XML: a bean XML's framework
    // diagnostics are computed from the text, so reading the stale file from disk would
    // squiggle the version you already fixed. JSP checks resolve against the project
    // config on the backend and genuinely don't need it.
    const src = isJava || /\.xml$/i.test(path) ? projectStore.sourceOf(path) : undefined;
    let cancelled = false;
    let fullDone = false;
    // The FULL (resolver-backed) pass — the authoritative set: drives the editor squiggles AND the
    // shared Problems-panel store (so the active-file section updates live and stays correct after
    // you switch away).
    const runFull = () => {
      void ipcDiagnostics(path, src, true)
        .then((ds) => {
          if (cancelled) return;
          fullDone = true;
          diags = ds.map((d) => ({ from: d.start, to: d.end, severity: cmSeverity(d.severity), message: d.message, code: d.code }));
          bennuDiagnosticsStore.setActiveFileDiagnostics(path, ds);
        })
        // A full-pass FAILURE (backend error/panic) must NOT blank the editor: keep whatever the
        // fast pure-AST pass already painted, so a single failing resolver check can't make all
        // validation — syntax included — appear to vanish. An empty *success* still clears via the
        // `.then` above.
        .catch(() => {});
    };
    if (hasPushedDiagnostics(path)) {
      // A language server's diagnostics are PUSHED — they already exist by the time they are
      // asked for, so the two-tier schedule below would buy nothing. One short-debounced read,
      // plus a re-read whenever the server publishes for this file (which for Rust is when
      // `cargo check` finishes, seconds after a save, long after any keystroke debounce).
      const read = () => {
        void ipcDiagnostics(path, src, true)
          .then((ds) => {
            if (cancelled) return;
            diags = ds.map((d) => ({ from: d.start, to: d.end, severity: cmSeverity(d.severity), message: d.message, code: d.code }));
            bennuDiagnosticsStore.setActiveFileDiagnostics(path, ds);
          })
          .catch(() => {});
      };
      const t = setTimeout(read, 150);
      const detach = bennuLspStore.onDiagnosticsPublished((file) => {
        if (!cancelled && isSamePath(file, path)) read();
      });
      return () => { cancelled = true; clearTimeout(t); detach(); };
    }
    if (isJava) {
      // Two-tier validation (IntelliJ's fast-syntax-then-semantic model) so a big file stays
      // responsive while typing: a FAST pure-AST pass (syntax / structure / unused imports) paints
      // squiggles almost immediately (~120ms), then the FULL resolver-backed pass (unknown members,
      // types, inheritance — the ~0.7s one on a large class) replaces it with the complete set
      // (~600ms idle). The full set is a superset, so it simply supersedes; the fast pass touches
      // ONLY the editor (not the Problems store) so the panel never shows a briefly-incomplete set,
      // and `fullDone` stops a late fast response from clobbering an already-applied full set.
      const tFast = setTimeout(() => {
        void ipcDiagnostics(path, src, false)
          .then((ds) => {
            if (cancelled || fullDone) return;
            diags = ds.map((d) => ({ from: d.start, to: d.end, severity: cmSeverity(d.severity), message: d.message, code: d.code }));
          })
          .catch(() => {});
      }, 120);
      const tFull = setTimeout(runFull, 600);
      return () => { cancelled = true; clearTimeout(tFast); clearTimeout(tFull); };
    }
    runFull();
    return () => { cancelled = true; };
  });

  // ── Spell-check (opt-in per project) — merged into the editor as hint squiggles ──
  // Runs against the live buffer (debounced), only when spell-check is enabled for
  // the project and dictionaries are installed. Each misspelled word carries quick-fix
  // actions: replace-with-suggestion + add-to-dictionary (project / global).
  let spellDiags = $state<EditorDiagnostic[]>([]);
  // Mojibake squiggles are an on-demand check (palette command), not auto-run — populated by
  // `checkMojibake()`, cleared when the active file changes so they never leak across files.
  let mojibakeDiags = $state<EditorDiagnostic[]>([]);
  // "Unknown property on action" warnings for a JSP form field / OGNL root or a
  // `*-validation.xml` `<field>` whose name isn't a property of the resolved action class. Runs
  // against the LIVE buffer (offsets match the editor), debounced, only on JSP / validation files;
  // re-runs when the index (config graph) rebuilds so the action resolves once it's ready.
  let propertyDiags = $state<EditorDiagnostic[]>([]);
  // "JSP not found" / "not a property of action" on a Struts config XML's `<result>` targets (live
  // buffer, debounced, re-run on index rebuild).
  let strutsDiags = $state<EditorDiagnostic[]>([]);
  // The last build's compiler errors/warnings for THIS file, placed in the buffer. Derived
  // rather than fetched: the run store already holds them, and re-deriving on every build
  // means a rebuild that fixes an error clears its mark without anyone clearing anything.
  //
  // Dropped the moment the buffer moves past the build. A compiler diagnostic describes the text
  // the compiler read, and its line/column are re-mapped against the CURRENT buffer — so an edit
  // that fixes the error does not remove the mark, it slides it onto whatever line now sits there.
  // One `cannot find symbol` rode an edit onto an unrelated method and read as a false positive on
  // code that compiles. Live validation covers the file from here until the next build.
  const buildDiags = $derived(
    activePath
      && projectStore.project
      && projectStore.editedAt(activePath) <= bennuRunStore.diagnosticsAt
      ? buildDiagnosticsFor(
          projectStore.project.root,
          activePath,
          projectStore.sourceOf(activePath),
          bennuRunStore.diagnostics,
        )
      : [],
  );
  const allDiags = $derived([
    ...diags, ...buildDiags, ...spellDiags, ...mojibakeDiags, ...propertyDiags, ...strutsDiags,
  ]);

  /**
   * The diagnostics a quick-fix could act on, in the shape the backend wants.
   *
   * Only the ones with a `code`: a fix is keyed by kind, so a diagnostic without one has no fix to
   * look up. That drops the build output and the spell hits, which is right — the first is a
   * compiler's word about a file on disk, and the second already carries its own actions.
   */
  function diagRefsForFixes(): DiagRef[] {
    const out: DiagRef[] = [];
    for (const d of allDiags) {
      if (d.code) out.push({ code: d.code, start: d.from, end: d.to });
    }
    return out;
  }

  // ── Semantic highlight (language-server backed languages) ───────────────────────
  //
  // A LAYER over the base highlight, not a replacement: the CodeMirror mode colours the file the
  // instant it opens, and this refines it once the server answers — a struct told apart from a
  // trait, a macro from a function, a `mut` binding from an immutable one.
  //
  // Debounced longer than the diagnostics read because the payload is every token in the file,
  // and re-requested when the server *becomes* ready so a file opened during startup does not
  // stay coarsely coloured until the next keystroke.
  //
  // Deliberately NOT gated on the server's advertised feature list. The request is cheap and the
  // backend answers `[]` for a file no server serves — whereas gating meant this one feature
  // depended on a chain (status → root match → feature list) that nothing else in the editor
  // touches, so a break anywhere in it showed up as "everything is white" while go-to and
  // completion carried on working. Ask, and tolerate an empty answer: the same shape as the rest.
  //
  // The re-fetch trigger is the server's `state` as a `$derived` **string**, not the statuses
  // object: a server reports progress several times a second while it indexes, and an effect
  // reading the statuses directly would re-request every token in the file on each of those ticks.
  const lspState = $derived(bennuLspStore.statusFor(activePath)?.state ?? null);
  $effect(() => {
    const path = activePath;
    if (!path || !isLspFileOf(path)) {
      editorComp?.setSemanticTokens([]);
      bennuLspStore.setTokenCount(0);
      return;
    }
    void lspState; // re-request when the server becomes ready (or dies)
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void lspSemanticTokens(path, src)
        .then((tokens) => {
          // A late answer for a tab the user has already left must not paint over the new one.
          if (cancelled || projectStore.activeFilePath !== path) return;
          editorComp?.setSemanticTokens(tokens);
          // Recorded so the footer can say how many landed. "All white" has two causes — none
          // arrived, or they arrived and lost the colour fight — and they look identical.
          bennuLspStore.setTokenCount(tokens.length);
        })
        .catch(() => {});
    }, 250);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── Folding, from the server ────────────────────────────────────────────────────
  //
  // Same shape as the semantic tokens above and for the same reasons: pushed rather than pulled,
  // debounced, keyed on the server's readiness, and dropped when a late answer belongs to a tab the
  // user has left. Longer debounce than the tokens — a fold arrow appearing a beat late costs
  // nothing, where a colour does.
  $effect(() => {
    const path = activePath;
    if (!path || !isLspFileOf(path)) {
      editorComp?.setFoldRanges([]);
      return;
    }
    void lspState;
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void lspFolding(path, src)
        .then((ranges) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          editorComp?.setFoldRanges(ranges);
        })
        .catch(() => {});
    }, 400);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── Code lenses ─────────────────────────────────────────────────────────────────
  //
  // The counts a server draws above an item — "3 implementations", "12 references".
  //
  // The longest debounce of the pushed layers, because it is the most expensive answer: the backend
  // resolves every lens individually (a server sends them with no title at all), so a file of a
  // hundred items is a hundred round-trips inside one request. A count that lags a keystroke is
  // worth having; one that re-queries per keystroke is a server permanently busy counting.
  //
  // Which lenses arrive is decided in the server's init options, not here — Bennu asks only for the
  // ones it can honour when pressed (see `catalogue.rs`'s `lens_options`).
  //
  // Two kinds share the layer, because a lens is a *place plus a label plus an action* and where it
  // came from is nobody's business but the press handler's:
  //
  //   * a **language server's** — the counts above an item;
  //   * a **manifest version hint** — "1.0.219 available" above an outdated dependency, which is the
  //     same shape and wants the same surface. A `Cargo.toml` has no language server at all, which is
  //     exactly why the layer is gated on the host's press handler rather than on one.
  type EditorLens =
    | { kind: 'lsp'; lens: LspLens }
    | { kind: 'version'; hint: CargoVersionHint }
    // The npm hint is the same offer with one difference that matters: its span excludes the
    // quotes, so the replacement is the bare version rather than a quoted one. Kept as its own
    // variant rather than normalised into the Cargo one, because a span that means two things
    // depending on where it came from is the kind of detail that eats a quote six months later.
    | { kind: 'npm-version'; hint: NpmVersionHint }
    | { kind: 'script'; script: NpmScript; manager: string };
  let lenses: EditorLens[] = [];

  /** Push the current list, keyed by index — the key only has to identify a lens within the list the
   *  editor is showing right now, which is what comes back on a press. */
  function pushLenses(next: EditorLens[]) {
    lenses = next;
    editorComp?.setCodeLenses(
      next.map((entry, key) => {
        if (entry.kind === 'lsp') {
          return {
            start: entry.lens.start,
            title: entry.lens.title,
            actionable: !!entry.lens.command,
            key,
          };
        }
        if (entry.kind === 'script') {
          // The manager's own name, not a generic "Run": which of npm / yarn / pnpm / bun a
          // repository uses is a thing people get wrong, and a control that says what it will
          // actually type is a control you can trust without checking.
          return {
            start: entry.script.offset,
            title: `▶ ${entry.manager} ${entry.script.name}`,
            actionable: true,
            tone: 'accent' as const,
            key,
          };
        }
        return {
          start: entry.hint.offset,
          // An arrow and the accent tone, because this one is an OFFER rather than a count: it
          // has to survive being glanced past, and a grey line above a line of code reads as a
          // comment. The word stays so the first one is unambiguous.
          title: `↑ ${entry.hint.latest} available`,
          actionable: true,
          tone: 'accent' as const,
          key,
        };
      }),
    );
  }

  $effect(() => {
    const path = activePath;
    if (!path || !isLspFileOf(path)) {
      // Not cleared for a manifest: that buffer's lenses come from the effect below, and clearing
      // here would race it into an empty layer on every keystroke.
      if (!isCargoManifest(path) && !isPackageManifest(path)) pushLenses([]);
      return;
    }
    void lspState;
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void lspCodeLenses(path, src)
        .then((found) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          pushLenses(found.map((lens) => ({ kind: 'lsp' as const, lens })));
        })
        .catch(() => {});
    }, 600);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── Version hints (Cargo.toml) ──────────────────────────────────────────────────
  //
  // "There is a newer release of this crate", above the dependency it is about.
  //
  // The longest debounce in the editor by some margin. Behind it is the crates.io index: cached on
  // disk with a TTL of a day, so in the steady state this is a file read, but on a cold cache it is
  // one small HTTP request per dependency. A dependency being one minor version behind does not
  // become more true by being checked while you type.
  //
  // Only what is *certainly* outdated is drawn — a pin, a range, a `path` or an inherited dependency
  // says nothing. See `requirement_admits` in `bennu-cargo` for why the test errs towards silence.
  $effect(() => {
    const path = activePath;
    if (!path || !isCargoManifest(path)) return;
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void cargoVersionHints(path, src)
        .then((hints) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          pushLenses(hints.map((hint) => ({ kind: 'version' as const, hint })));
        })
        .catch(() => {});
    }, 900);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── package.json: run controls, and version hints ───────────────────────────────
  //
  // Two sources in one layer, because they belong on the same buffer and the layer is per-buffer:
  // the scripts arrive at once (a parse of the text, no network) and the hints arrive later (one
  // registry request per dependency, cached for a day). Pushed together on each pass so a slow
  // hints answer cannot wipe out the run controls that were already there — which is what
  // pushing them separately did, and it read as controls that flickered.
  //
  // The run controls are drawn from the BUFFER, so a script added a second ago has one before the
  // file is saved.
  let npmScripts = $state<NpmScript[]>([]);
  let npmManager = $state('npm');
  let npmHints = $state<NpmVersionHint[]>([]);

  $effect(() => {
    const path = activePath;
    if (!path || !isPackageManifest(path)) return;
    const src = projectStore.sourceOf(path);
    let cancelled = false;

    // Short: this is a parse of text the editor already has, and a run control that appears half a
    // second after you finish typing a script name looks broken.
    const scripts = setTimeout(() => {
      void npmManifest(path, src)
        .then((m) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          npmScripts = m.scripts;
          npmManager = m.package_manager;
        })
        .catch(() => {});
    }, 250);

    // Long, for the same reason the Cargo one is: behind it is a request per dependency. A package
    // being one minor version behind does not become more true by being checked while you type.
    const hints = setTimeout(() => {
      void npmVersionHints(path, src)
        .then((found) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          npmHints = found;
        })
        .catch(() => {});
    }, 900);

    return () => { cancelled = true; clearTimeout(scripts); clearTimeout(hints); };
  });

  // The two halves, merged. A `$effect` rather than a call at each arrival so the layer is written
  // once per change of either.
  $effect(() => {
    if (!isPackageManifest(activePath)) return;
    const manager = npmManager;
    pushLenses([
      ...npmScripts.map((script) => ({ kind: 'script' as const, script, manager })),
      ...npmHints.map((hint) => ({ kind: 'npm-version' as const, hint })),
    ]);
  });

  // Leaving a manifest drops what was read for it: the next one's scripts arrive a moment after it
  // opens, and showing the previous file's in the meantime is worse than showing none.
  $effect(() => {
    const path = activePath;
    if (isPackageManifest(path)) return;
    untrack(() => {
      npmScripts = [];
      npmHints = [];
    });
  });

  /**
   * A lens was pressed.
   *
   * Every lens rust-analyzer draws is a **client** command, so there is nothing to execute on the
   * server: `showReferences` arrives with the locations it counted already in its arguments, because
   * counting them meant querying them. So the first thing tried is reading that list — no request,
   * and exactly the places the lens promised.
   *
   * The fallbacks are in order of how much they assume. A single location is a jump, because a list
   * of one is a click for nothing. Several go to the usages popover, the same surface Alt+F7 fills.
   * A command carrying no locations at all is something else entirely (a runnable), so it is handed
   * to the server — and if the server does not own it either, the press says so rather than being
   * swallowed, which is the difference between a control that failed and one that looks broken.
   */
  /**
   * The `runnable` behind a rust-analyzer Run / Debug lens, or `null` for any other lens.
   *
   * Shaped defensively: the argument is whatever the server sent, and a version that changes the
   * payload should make the lens do nothing rather than launch `cargo undefined`. Only the cargo
   * kind is honoured — rust-analyzer can also emit a `shell` runnable, which is a different
   * program with a different working directory and not something to guess at.
   */
  function rustRunnableOf(lens: LspLens): {
    label: string;
    cargoArgs: string[];
    cargoExtraArgs: string[];
    executableArgs: string[];
    workspaceRoot: string;
  } | null {
    if (lens.command !== 'rust-analyzer.runSingle' && lens.command !== 'rust-analyzer.debugSingle') {
      return null;
    }
    const first = lens.arguments?.[0] as
      | { label?: unknown; kind?: unknown; args?: Record<string, unknown> }
      | undefined;
    const args = first?.args;
    if (!args || (typeof first?.kind === 'string' && first.kind !== 'cargo')) return null;
    const strings = (v: unknown): string[] =>
      Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
    const cargoArgs = strings(args.cargoArgs);
    if (cargoArgs.length === 0) return null;
    return {
      label: typeof first?.label === 'string' ? first.label : lens.title,
      cargoArgs,
      cargoExtraArgs: strings(args.cargoExtraArgs),
      executableArgs: strings(args.executableArgs),
      workspaceRoot: typeof args.workspaceRoot === 'string' ? args.workspaceRoot : '',
    };
  }

  async function onLensPress(key: number) {
    const entry = lenses[key];
    if (!entry) return;
    if (entry.kind === 'version') {
      updateDependencyVersion(entry.hint);
      return;
    }
    if (entry.kind === 'npm-version') {
      if (!editorComp) return;
      // The bare version: this span is the string's CONTENTS, quotes excluded — see the variant's
      // comment. Writing a quoted value here would produce `""6.0.0""`.
      editorComp.replaceByteRange(entry.hint.start, entry.hint.end, entry.hint.latest);
      toastStore.show(`${entry.hint.name} → ${entry.hint.latest}`, 'success');
      return;
    }
    if (entry.kind === 'script') {
      const root = projectStore.project?.root;
      const file = activePath;
      if (!root || !file) return;
      void npmRunScript(root, file, entry.script.name)
        .then(() => bennuUiStore.showBottom('run'))
        .catch((e) => toastStore.show(`${entry.manager} ${entry.script.name}: ${e}`, 'error'));
      return;
    }
    const path = activePath;
    const lens = entry.lens;
    if (!path || !lens.command || !editorComp) return;

    // rust-analyzer's own ▶ Run / Debug. The server has already worked out the exact cargo
    // invocation — which package, which binary, which test and with `--exact` — so the runner is
    // handed those arguments **verbatim** rather than re-deriving them from the file. Re-deriving
    // is how the ▶ above a `#[test]` ends up running the whole suite.
    const runnable = rustRunnableOf(lens);
    if (runnable) {
      const root = projectStore.project?.root;
      if (!root) return;
      const [command, ...rest] = runnable.cargoArgs;
      // ⚠️ rust-analyzer ends a test runnable's `cargoArgs` with a bare `--`, because in its own
      // client the two lists are concatenated. Here the separator is added by the invocation
      // builder before `args`, so leaving this one in produces `cargo test … -- -- name --exact`
      // and the second `--` reaches the test harness as a filter that matches nothing: the ▶
      // above one test would run zero.
      while (rest.length && rest[rest.length - 1] === '--') rest.pop();
      bennuUiStore.showBottom('run');
      void bennuRunStore.runCargoCommand(
        root,
        {
          ...emptyCargoInvocation(command || 'run'),
          // Everything after the subcommand, as the server wrote it. `extra` is appended to the
          // argv untouched, which is exactly what "the flags rust-analyzer chose" needs.
          extra: [...rest, ...runnable.cargoExtraArgs],
          args: runnable.executableArgs,
        },
        runnable.label,
        { debug: lens.command === 'rust-analyzer.debugSingle', workingDir: runnable.workspaceRoot },
      );
      return;
    }

    const source = editorComp.getValue();
    const anchor = editorComp.coordsAtByteOffset(lens.start);
    const result = await lspLensLocations(path, source, lens.title, lens.arguments).catch(() => null);
    const hits = result?.usages ?? [];

    if (hits.length === 1) {
      const hit = hits[0];
      void projectStore.openFile(hit.file).then(() => bennuUiStore.requestGoto(hit.line));
      return;
    }
    if (hits.length > 1) {
      bennuRefactorStore.startUsages(anchor, lens.title);
      bennuRefactorStore.setUsages(result?.target_label ?? lens.title, hits);
      return;
    }
    const ran = await lspExecuteCommand(path, lens.command, lens.arguments).catch(() => false);
    if (!ran) toastStore.show(`Nothing to show for “${lens.title}”`, 'info');
  }

  /**
   * Write the newer version into the manifest.
   *
   * Through CodeMirror, so it is one undo step and the buffer stays the authority — the alternative,
   * having the backend rewrite the file, would leave the open buffer disagreeing with the disk.
   *
   * The span the backend sent includes the quotes, so the replacement is a whole TOML value. Nothing
   * else is touched: a bare `"1.0.150"` becomes `"1.0.219"` and a `{ version = "…" }` keeps its table.
   * The lens list is left alone — the buffer changed, and the effect above is about to re-ask.
   */
  function updateDependencyVersion(hint: CargoVersionHint) {
    if (!editorComp) return;
    editorComp.replaceByteRange(hint.start, hint.end, `"${hint.latest}"`);
    toastStore.show(`${hint.name} → ${hint.latest}`, 'success');
  }

  // ── Occurrence highlighting ─────────────────────────────────────────────────────
  //
  // Where else the symbol under the caret appears. Keyed on the CARET, which makes it the most
  // frequently re-run effect in the editor — hence the shortest debounce that is still longer than a
  // keypress, and a request the backend gives a two-second timeout: a highlight that arrives after
  // the caret has moved on decorates the wrong thing, so late is worse than never.
  let highlightCaret = $state(0);
  $effect(() => {
    const path = activePath;
    const caret = highlightCaret;
    if (!path || !isLspFileOf(path)) {
      editorComp?.setDocumentHighlights([]);
      return;
    }
    const src = editorComp?.getValue() ?? '';
    let cancelled = false;
    const t = setTimeout(() => {
      void lspHighlights(path, src, caret)
        .then((spans) => {
          // Both guards matter: the tab may have changed, and the caret may have moved to a
          // position this answer says nothing about.
          if (cancelled || projectStore.activeFilePath !== path) return;
          editorComp?.setDocumentHighlights(spans);
        })
        .catch(() => {});
    }, 220);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── Parameter hints ─────────────────────────────────────────────────────────────
  //
  // The signature of the call the caret is inside, in the strip above the line. Keyed on the caret
  // like occurrence highlighting, and for the same reason: what you want to know changes with every
  // comma. Both engines answer it — Bennu's resolver for Java, the language server for everything
  // else — into the same shared widget, so the two look and behave identically.
  //
  // Cleared eagerly on a caret that is not in a call, rather than waiting for the answer: leaving
  // the last signature up while you type the next statement is worse than a flicker, because it
  // reads as a claim about the line you are on now.
  $effect(() => {
    const path = activePath;
    const caret = highlightCaret;
    if (!path || (!isJavaFileOf(path) && !isLspFileOf(path))) {
      editorComp?.setSignatureHint(null);
      return;
    }
    const src = editorComp?.getValue() ?? '';
    let cancelled = false;
    const t = setTimeout(() => {
      const fetch = isJavaFileOf(path)
        ? ipcSignatureHelp(path, src, caret).then(javaSignatureToHint)
        : lspSignatureHelp(path, src, caret).then(lspSignatureToHint);
      void fetch
        .then((hint) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          editorComp?.setSignatureHint(hint);
        })
        .catch(() => {});
    }, 160);
    return () => { cancelled = true; clearTimeout(t); };
  });

  /** Bennu's own answer, which already carries a span per parameter. */
  function javaSignatureToHint(s: SignatureHelp | null) {
    if (!s) return null;
    return {
      label: s.label,
      params: s.params.map(([start, end]) => ({ start, end })),
      active: s.active,
      anchor: s.anchor,
      overload: s.overload ? { index: s.overload[0], count: s.overload[1] } : null,
    };
  }

  /**
   * A language server's answer, which marks only the ACTIVE parameter's span.
   *
   * The widget takes a span per parameter and an index, so the server's single span becomes a
   * one-element list with index 0 — the same thing said in the shape the widget speaks. Rebuilding
   * the other spans by searching the label for each parameter's text is what the previous,
   * never-wired version did, and it goes wrong the moment two parameters share a type.
   */
  function lspSignatureToHint(s: LspSignature | null) {
    if (!s) return null;
    const start = s.active_start ?? null;
    const end = s.active_end ?? null;
    const params = start !== null && end !== null ? [{ start, end }] : [];
    return {
      label: s.label,
      params,
      active: 0,
      // The server reports no anchor; the caret is inside the call, which is close enough to put
      // the strip over the right line.
      anchor: editorComp?.caretByteOffset() ?? 0,
      doc: s.doc ?? null,
      overload: null,
    };
  }

  // ── Inlay hints ─────────────────────────────────────────────────────────────────
  //
  // Argument names and inferred types, drawn between the code. Keyed on the DOCUMENT rather than
  // the caret — they annotate the file, not the position — with a longer debounce, because a hint
  // that is one keystroke stale is invisible while a request per keystroke is not.
  //
  // Off unless the setting is on, and cleared when it is turned off, so the toggle is immediate
  // rather than effective from the next edit.
  $effect(() => {
    const path = activePath;
    const revision = docRevision;
    const on = bennuSettingsStore.inlayHints;
    if (!path || !on || !isJavaFileOf(path)) {
      editorComp?.setInlayHints([]);
      return;
    }
    void revision;
    const src = editorComp?.getValue() ?? '';
    let cancelled = false;
    const t = setTimeout(() => {
      void ipcInlayHints(path, src)
        .then((hints) => {
          if (cancelled || projectStore.activeFilePath !== path) return;
          editorComp?.setInlayHints(hints);
        })
        .catch(() => {});
    }, 400);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── Expand / shrink selection ───────────────────────────────────────────────────
  //
  // The server answers with the WHOLE chain from the token under the caret out to the file, so
  // expanding walks a list already in hand and shrinking walks back down it. One request per run,
  // not one per keypress — which is the difference between this feeling instant and feeling remote.
  //
  // The chain is dropped as soon as the buffer changes or the caret is moved by anything other than
  // an expand: it describes a document state, and applying a stale link would select the wrong text.
  let selectionChain: [number, number][] = [];
  let selectionDepth = 0;
  /** Set while a chain step is dispatching, so the caret listener does not read its own selection
   *  change as the user moving away and discard the chain. */
  let steppingSelection = false;

  function forgetSelectionChain() {
    selectionChain = [];
    selectionDepth = 0;
  }

  /** Grow or shrink the selection by one syntactic step. Returns false when there is nothing to do,
   *  so the key can fall through. */
  async function stepSelection(direction: 1 | -1): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || !isLspFileOf(path)) return false;

    if (selectionChain.length === 0) {
      if (direction < 0) return false; // nothing to shrink back to
      const src = editorComp.getValue();
      const chain = await lspSelectionRanges(path, src, editorComp.caretByteOffset()).catch(
        () => [] as [number, number][],
      );
      if (chain.length === 0 || projectStore.activeFilePath !== path) return false;
      selectionChain = chain;
      // The first link is the token the caret is in. Selecting it IS the first expansion, so the
      // depth starts below it and the step below moves onto it.
      selectionDepth = -1;
    }

    const next = selectionDepth + direction;
    if (next < 0) {
      // Shrunk past the innermost link: the run is over and the caret keeps what it has.
      forgetSelectionChain();
      return false;
    }
    if (next >= selectionChain.length) return false;

    selectionDepth = next;
    const [start, end] = selectionChain[next];
    steppingSelection = true;
    editorComp.setSelectionBytes(start, end);
    // Cleared after the dispatch has been observed, not synchronously: the selection listener runs
    // from CodeMirror's update cycle, which is a microtask away.
    setTimeout(() => { steppingSelection = false; }, 0);
    return true;
  }

  // ── Server-initiated edits (`workspace/applyEdit`) ──────────────────────────────
  //
  // Some refactorings are delivered this way: the server computes the edit only when its command
  // runs, then asks the client to apply it. Applied through CodeMirror like every other edit, so
  // it lands in the undo history — the backend never writes a buffer.
  $effect(() => {
    const detachEdit = bennuLspStore.onServerEdit((edits, fileOps) => {
      if (fileOps.length) {
        // Bennu edits buffers; it does not create, move or delete files for a server. Saying so
        // is the point: a silently half-applied refactoring leaves a project that does not build
        // with nothing on screen to explain why.
        toastStore.show(
          `This refactoring also needs: ${fileOps.join(', ')} — do it yourself and re-run`,
          'warning',
        );
      }
      if (!edits.length) return;
      void applyServerEdits(edits);
    });
    const detachMessage = bennuLspStore.onServerMessage((level, text) => {
      toastStore.show(text, level === 'error' ? 'error' : level === 'warning' ? 'warning' : 'info');
    });
    return () => { detachEdit(); detachMessage(); };
  });

  /** Apply a server's edits and say so when some file would not take them.
   *
   *  The splicing itself lives in the project store (`applyEdits`) — a file rename's implied edits
   *  were the third caller of the same six lines, and the step they all need is "read what is there
   *  now, splice byte spans, write it back through the save guard". */
  async function applyServerEdits(edits: SourceEdit[]) {
    const failed = await projectStore.applyEdits(edits);
    if (failed) {
      toastStore.show(`Could not apply the change in ${failed} file(s)`, 'error');
    }
  }

  // ── Framework syntax marks (Spring `${…}` / `#{…}` / `{pathVar}`) ────────────────
  // A property placeholder inside a Java string literal is, to the Java grammar, one
  // undifferentiated string — which is exactly why a typo in it is invisible. These marks
  // colour the parts the framework actually reads: the key, the default, the SpEL bean
  // reference, the path variable. Debounced with the buffer, and byte→UTF-16 mapped
  // because the backend speaks bytes and the editor speaks code units.
  let springMarks = $state<{ from: number; to: number; className: string }[]>([]);
  let springGutter = $state<ExtGutterMark[]>([]);
  /** What the active frameworks offer to write into this buffer — the toolbar's contents.
   *  Contributed rather than enumerated here: whether this file is an entity or a repository is
   *  a question about Java, and it is answered on the same debounce as the marks, off the LIVE
   *  buffer, so a class you have just annotated grows its buttons without waiting for a
   *  reindex. */
  let fwActions = $state<ExtAction[]>([]);
  // The environment-override card, when the right-click menu asked for one. Local to the
  // editor because that menu is the only thing that opens it.
  let envVarView = $state<EnvVarView | null>(null);
  $effect(() => {
    const path = activePath;
    // Java / JSP / XML, plus the Spring property files — those carry no diagnostics but do
    // carry the gutter's usage counts, which is the whole point of opening one.
    const wantsFramework =
      !!path
      && (supportsDiagnostics(path)
        || /(^|[\\/])(application|bootstrap)[^\\/]*\.(ya?ml|properties)$/i.test(path)
        // A fulcrum translation bundle. TOML has one opinion about `'$red.bold{{n}} left'` — it is a
        // string — which is exactly why a mistake inside it survives review, and the marks below are
        // what make its structure visible. Asked for on every bundle, panel open or not: the
        // colouring is not a feature of the panel.
        || isI18nBundle(path)
        // A Rust source. Named explicitly rather than left to `supportsDiagnostics`, which only
        // admits a `.rs` once a language server has claimed it: the ECS gutter marks are Bennu's
        // own and must not blink out because rust-analyzer is absent, misconfigured or still
        // starting.
        || isRustFileOf(path));
    if (!path || !wantsFramework) { springMarks = []; springGutter = []; fwActions = []; return; }
    const src = projectStore.sourceOf(path);
    void bennuIndexStore.buildRevision; // new beans / new keys after a rebuild
    let cancelled = false;
    const t = setTimeout(() => {
      void extHighlights(path, src)
        .then((hs) => { if (!cancelled) springMarks = toSpringMarks(src, hs); })
        // No framework on this project (or an older backend) — no marks, no noise.
        .catch(() => { if (!cancelled) springMarks = []; });
      void extGutter(path, src)
        .then((gs) => { if (!cancelled) springGutter = gs; })
        .catch(() => { if (!cancelled) springGutter = []; });
      void extActions(path, src)
        .then((as) => { if (!cancelled) fwActions = as; })
        // Keep the buttons that were there. A failed call means "I don't know", not "this file
        // has nothing" — and blanking on it is how a backend that is merely busy reads as an
        // editor whose toolbar has broken. The marks above do clear, because a stale squiggle
        // sits at an offset the buffer may no longer have; a button carries no offset.
        .catch(() => {});
    }, 220);
    return () => { cancelled = true; clearTimeout(t); };
  });

  /*
   * ── The i18n panel, both directions ───────────────────────────────────────────────────────────
   *
   * The panel is a tool window on the right and has no handle on this component, so the store between
   * them carries traffic each way — see `bennuI18nStore` for why an *intent* travels rather than a
   * computed edit.
   */

  /** Feed the panel: the value under the caret, re-read on every move and every keystroke. */
  $effect(() => {
    const path = activePath;
    // Only while the panel is open. The markup COLOURING above is unconditional; this is the panel's
    // own data, and asking for it with nothing on screen would be a round trip per keystroke for
    // nobody.
    if (bennuUiStore.rightPanel !== 'i18n' || !path || !isI18nBundle(path)) {
      bennuI18nStore.reset();
      return;
    }
    // Both dependencies are the point: the caret moving means a different translation, the buffer
    // changing means the same one says something else. `sourceOf` is the tracked one — reading the
    // document off the component would not re-run this on a keystroke.
    void caretLine;
    void caretCol;
    void projectStore.sourceOf(path);
    void bennuIndexStore.buildRevision; // a new stylesheet / a new sibling language after a scan
    void bennuI18nStore.retry; // a rescan changed the project's capabilities, not the buffer
    if (!editorComp) return;
    // The text and the offset must come from the SAME document. `caretByteOffset()` measures against
    // the editor's own, so the text has to be the editor's own too — the store's copy is updated from
    // `oninput` and is a tick behind mid-keystroke, which is exactly when this runs. A value's span is
    // a few bytes wide, so a few bytes of drift is the difference between finding it and finding
    // nothing.
    bennuI18nStore.track(path, editorComp.getValue(), editorComp.caretByteOffset());
  });

  /**
   * Write what the panel's toolbar asked for.
   *
   * The selection is read HERE, at the moment the button was pressed, and clamped into the value —
   * without the clamp a selection spanning the closing quote would write `$bold{` inside the string
   * and `}` outside it, producing a file TOML can no longer parse from one click on a button whose
   * job is to be safe.
   */
  $effect(() => {
    const req = bennuI18nStore.request;
    if (!req) return;
    // Untracked: this reads the view and the buffer to compute an edit, and every one of those reads
    // would otherwise re-run the effect on the change the edit itself causes.
    untrack(() => {
      const view = bennuI18nStore.view;
      const base = view?.content_start;
      if (!editorComp || !view || base == null) { bennuI18nStore.consume(); return; }

      const raw = editorComp.selectionByteRange();
      const valueEnd = base + new TextEncoder().encode(view.raw).length;
      const start = Math.min(Math.max(raw.start, base), valueEnd);
      const end = Math.min(Math.max(raw.end, start), valueEnd);
      // A caret outside the value entirely (the panel was open, then the caret moved to the key)
      // collapses to a clamped point, which is still inside the value — so the construct lands in the
      // text rather than being written nowhere.
      const text = start === raw.start && end === raw.end ? editorComp.getSelectionText() : '';

      const edit = markupEdit(req.insert, { start, end, text });
      if (edit) {
        editorComp.replaceByteRange(edit.start, edit.end, edit.text);
        editorComp.setSelectionBytes(edit.selectStart, edit.selectEnd);
        editorComp.focus();
      }
      bennuI18nStore.consume();
    });
  });

  /** Glyph per gutter-mark kind. Text, not an icon set: the shared editor draws whatever
   *  string it is given, so a new kind costs a character here and nothing anywhere else. */
  const GUTTER_GLYPHS: Record<string, string> = {
    bean: '◆',       // ◆ a bean is declared on this line
    inject: '→',     // → something is injected here
    endpoint: '»',   // » a route enters here
    entity: '▤',     // ▤ a persistent entity — points at the repositories that manage it
    repository: '◇', // ◇ a repository — points at the entity it manages
    // The ECS declarations. Distinct glyphs rather than one for all four, because the question a
    // mark answers is different for each: a component is data on an entity, a resource is a
    // singleton, an event is a buffer, a bundle is a recipe.
    component: '◈',  // ◈ a component — points at the systems that read and write it
    resource: '▣',   // ▣ a resource — points at the systems that read and write it
    message: '✉',    // ✉ a buffered message — points at its readers and writers
    event: '✳',      // ✳ an observer event — points at the observers that handle it
    bundle: '▦',     // ▦ a bundle — points at what it inserts
    states: '⬡',     // ⬡ a States enum
  };

  const springGutterMarks = $derived(
    springGutter.map((g) => ({
      line: g.line,
      // For a usage mark the COUNT is the glyph: beside a property key, `2` says more in one
      // character than any icon could, and an unmarked line is the signal that nothing reads it.
      glyph: g.kind === 'usage' ? String(g.targets.length) : (GUTTER_GLYPHS[g.kind] ?? '•'),
      tooltip: g.targets.length > 0 ? `${g.tooltip} — click to open` : g.tooltip,
      className: `cm-fw-gutter cm-fw-gutter-${g.kind}`,
    })),
  );

  // ── The ▶ beside a `main` ────────────────────────────────────────────────────
  //
  // IntelliJ's green arrow, and the reason it is worth copying is not that it saves a menu: it is
  // that it says *this class is a way in*. On a legacy project with four modules and eleven
  // entry points, which file starts the thing is a question people answer by grepping.
  //
  // Two facts have to meet, and they come from different places on purpose. WHICH line is a
  // question about the buffer in front of you — including the one you are typing right now, which
  // no index has seen. WHAT to run is a question about the project, and it is answered by the
  // backend's own entry-point scan (`bennuMainClassStore`), so the class that gets launched is the
  // one the compiler will agree exists rather than a name assembled out of a `package` line that
  // may be wrong. No entry, no arrow — a ▶ that fails when pressed is worse than none.

  /** `public static void main(String[] args)`, in any of its spellings. */
  const MAIN_METHOD = /^[^\S\r\n]*(?:public\s+)?(?:static\s+final\s+|final\s+static\s+|static\s+)void\s+main\s*\(\s*(?:final\s+)?String\s*(?:\[\s*\]|\.{3})/;

  /** The entry points the backend found in THIS file. Loaded once per project (the store caches
   *  and joins concurrent callers), and only for a Java project — a Cargo one has no such scan. */
  const fileEntryPoints = $derived.by(() => {
    const root = projectStore.project?.root;
    const path = activePath;
    if (!root || !path || projectStore.isCargo || !isJavaFile) return [];
    const here = path.replace(/\\/g, '/');
    return bennuMainClassStore.forRoot(root).filter((e) => e.source_file === here);
  });

  $effect(() => {
    const root = projectStore.project?.root;
    if (!root || projectStore.isCargo || !isJavaFile) return;
    // Cached per project: this is a no-op after the first Java file of the session.
    void bennuMainClassStore.load(root);
  });

  /** The 1-based lines of this buffer that declare a `main`. Read off the live text, so the
   *  arrow appears with the method rather than after the next index build. */
  const mainLines = $derived.by(() => {
    if (!activePath || fileEntryPoints.length === 0) return [] as number[];
    const out: number[] = [];
    const lines = projectStore.sourceOf(activePath).split('\n');
    for (let i = 0; i < lines.length; i++) if (MAIN_METHOD.test(lines[i])) out.push(i + 1);
    return out;
  });

  /**
   * A script is runnable as a whole, so its arrow goes on its first meaningful line — the shebang
   * when it has one, because that line IS the statement "this file is a program", and line 1
   * otherwise.
   *
   * Nothing is checked about the machine here. A `.bat` on a Mac still gets an arrow, and pressing
   * it prints the reason in the console: a greyed-out control teaches nothing, and the refusal
   * names the interpreter and where it was looked for.
   */
  const scriptRunLine = $derived.by(() => {
    if (!activePath || !isRunnableScript(activePath)) return 0;
    const first = projectStore.sourceOf(activePath).split('\n', 1)[0] ?? '';
    return first.startsWith('#!') ? 1 : 1;
  });

  const runGutterMarks = $derived(
    scriptRunLine
      ? [{
          line: scriptRunLine,
          glyph: '▶',
          tooltip: `Run ${activePath?.split(/[\\/]/).pop() ?? 'this script'}`,
          className: 'cm-run-gutter',
        }]
      : mainLines.map((line) => ({
          line,
          glyph: '▶',
          tooltip: fileEntryPoints[0]?.spring_boot
            ? `Run ${fileEntryPoints[0].fqcn} (Spring Boot)`
            : `Run ${fileEntryPoints[0]?.fqcn ?? 'this class'}`,
          className: 'cm-run-gutter',
        })),
  );

  /**
   * The two gutters are one column, so they are merged here — the run arrow first, because a line
   * that is both an entry point and a framework mark is a line you press to RUN. Nothing in the
   * project marks a `main` line today; the order is stated so that the day something does, the
   * answer is decided rather than incidental.
   */
  /** The lines the ▶ owns — a script's one line, or every `main` in a Java file. */
  const runLines = $derived(runGutterMarks.map((m) => m.line));

  const allGutterMarks = $derived([
    ...runGutterMarks,
    ...springGutterMarks.filter((g) => !runLines.includes(g.line)),
  ]);

  /**
   * ▶ pressed: run it, or offer the other things you might have meant.
   *
   * A menu rather than an immediate launch, because Debug is the other half of the gesture and
   * IntelliJ's own arrow opens one too. A file declaring **two** entry points (a secondary class
   * with its own `main`) lists both: picking the first silently would be a launch of something
   * you did not press.
   */
  function onRunGutterClick(line: number, event: MouseEvent) {
    const root = projectStore.project?.root;
    if (!root) return;
    // A script has one way to be run and no debugger behind it, so the arrow just runs it —
    // a menu with a single entry is a click spent on nothing.
    if (activePath && isRunnableScript(activePath)) {
      const path = activePath;
      // Whatever is in the buffer is what should run: a script is edited and run in the same
      // breath, and launching the version on disk would run the line you just fixed. A refused
      // save is a conflict — the store raises it, and running the old text on top of that would
      // be the second wrong thing to do about it.
      void projectStore.saveActive().then((ok) => {
        if (ok || !projectStore.isDirty(path)) void bennuRunStore.runScript(root, path);
      });
      return;
    }
    const entries = fileEntryPoints;
    if (entries.length === 0) return;
    void line;
    const simple = (fqcn: string) => fqcn.split('.').pop() ?? fqcn;
    const items: MenuItem[] = entries.flatMap((e, i) => [
      { id: `run:${i}`, label: `Run ${simple(e.fqcn)}`, icon: Play,
        shortcut: i === 0 && entries.length === 1 ? 'Shift+F10' : undefined },
      { id: `debug:${i}`, label: `Debug ${entries.length > 1 ? simple(e.fqcn) : ''}`.trim(), icon: Bug },
    ]);
    items.push({ separator: true, id: 'sep-run', label: '' });
    // The escape hatch from an ad-hoc launch: arguments, VM flags, a working directory. The
    // arrow deliberately runs with none of those, so this is where you go when you need them.
    items.push({ id: 'edit', label: 'Edit configurations…', icon: SlidersHorizontal });

    bennuContextMenuStore.show(event.clientX, event.clientY, items, (id) => {
      if (id === 'edit') { bennuUiStore.openRunConfig(); return; }
      const [what, at] = id.split(':');
      const entry = entries[Number(at)];
      if (!entry) return;
      void bennuRunStore.runMainClass(root, {
        mainClass: entry.fqcn,
        module: entry.module ?? '',
        label: simple(entry.fqcn),
        debug: what === 'debug',
      });
    });
  }

  /**
   * Clicking a gutter icon opens what it points at — and when it points at more than one
   * thing, it asks.
   *
   * Silently picking one is the wrong answer twice over: it hides that there were others, and
   * the one it picks is the one *we* ranked rather than the one you meant. A bean injected in
   * six places has six real answers, so the menu is anchored at the pointer and lists them all.
   */
  function onGutterClick(line: number, event: MouseEvent) {
    if (runLines.includes(line)) {
      onRunGutterClick(line, event);
      return;
    }
    onSpringGutterClick(line, event);
  }

  function onSpringGutterClick(line: number, event: MouseEvent) {
    const mark = springGutter.find((g) => g.line === line);
    if (!mark || mark.targets.length === 0) return;
    if (mark.targets.length === 1) {
      openDefinitionFile(mark.targets[0].file, mark.targets[0].offset);
      return;
    }
    showTargetPicker(mark.targets, event.clientX, event.clientY);
  }

  /** A menu over `targets`, anchored at a point. Each row names the target and, under it, what
   *  kind of site it is — which is how you tell two injections of the same bean apart. */
  function showTargetPicker(targets: ExtTarget[], x: number, y: number) {
    const items: MenuItem[] = targets.map((t, i) => ({
      id: String(i),
      label: t.detail ? `${t.label} — ${t.detail}` : t.label,
      icon: Target,
    }));
    bennuContextMenuStore.show(x, y, items, (id) => {
      const t = targets[Number(id)];
      if (t) openDefinitionFile(t.file, t.offset);
    });
  }

  // ── Breakpoints (the flag gutter) ─────────────────────────────────────────────

  /**
   * Whether this file can hold a breakpoint at all.
   *
   * Java **and Rust**: both have a debugger behind them now, and the two are the same gesture in the
   * same margin — see `debug_backend` on the backend side. A decompiled view is excluded because its
   * line numbers mean nothing to the VM, and everything else (a `.properties`, a `.xml`) compiles to
   * nothing at all: offering a gutter there is offering a click that can only ever be pending.
   */
  const breakLanguage: 'java' | 'rust' | null = $derived.by(() => {
    if (!activePath || isDecompiledView) return null;
    if (isJavaFileOf(activePath)) return 'java';
    if (isRustFileOf(activePath)) return 'rust';
    return null;
  });
  const canBreak = $derived(breakLanguage !== null);

  /**
   * The gutter's dots: solid where the VM accepted the breakpoint, hollow where it is waiting
   * for a class to load, muted where it is disabled.
   *
   * The distinction is the whole value of showing verification: "waiting for the class to
   * load" resolves itself the moment the program touches it, and "that line has no code"
   * never will — and staring at a breakpoint that will never be hit is the single most
   * expensive way to misread a debugger.
   */
  const breakpointMarks = $derived.by(() => {
    const root = projectStore.project?.root;
    if (!root || !activePath || !canBreak) return [];
    return bennuDebugStore.breakpointsIn(root, activePath).map((b) => {
      const status = bennuDebugStore.statusOf(b.file, b.line);
      const restricted = !!b.condition.trim() || b.hit_count > 1;
      const classes = ['cm-bp'];
      if (!b.enabled) classes.push('cm-bp-off');
      else if (status && !status.verified) classes.push('cm-bp-pending');
      // Marked distinctly whatever else it is: a breakpoint that does not stop is the single most
      // expensive thing to misread in a debugger, and "it has a condition on it" is the answer
      // ninety per cent of the time. The dot has to say so without being clicked.
      if (restricted) classes.push('cm-bp-cond');
      return {
        line: b.line,
        className: classes.join(' '),
        tooltip: [
          b.enabled ? 'Breakpoint' : 'Breakpoint (disabled)',
          b.condition.trim() ? `stops when ${b.condition.trim()}` : '',
          b.hit_count > 1 ? `every ${b.hit_count} hits` : '',
          status?.hits ? `hit ${status.hits}×` : '',
          status?.condition_error || status?.message || '',
        ]
          .filter(Boolean)
          .join(' — '),
      };
    });
  });

  /**
   * The lines a breakpoint may be set on — those that compile to bytecode.
   *
   * Recomputed off the live buffer on a debounce, like the framework marks above: it is a scan
   * of one file, but it is not worth doing per keystroke, and being a beat stale only means the
   * line you are halfway through typing does not offer a dot yet.
   */
  let breakpointable = $state<Set<number>>(new Set());
  $effect(() => {
    if (!canBreak || !activePath) {
      breakpointable = new Set();
      return;
    }
    const src = projectStore.sourceOf(activePath);
    // The language decides the reading: a Rust `fn` line takes a breakpoint (LLDB binds it to the
    // prologue) where a Java method signature does not, and Rust's lifetimes and raw strings would
    // desync a scanner written for Java literals.
    const language = breakLanguage ?? 'java';
    const t = setTimeout(() => { breakpointable = breakpointableLines(src, language); }, 200);
    return () => clearTimeout(t);
  });

  /** A new identity per recomputation, which is what tells the editor its column has changed. */
  const canFlagLine = $derived.by(() => {
    const lines = breakpointable;
    return (line: number) => lines.has(line);
  });

  function onBreakpointClick(line: number) {
    const root = projectStore.project?.root;
    if (root && activePath && canBreak) bennuDebugStore.toggleBreakpoint(root, activePath, line);
  }

  /**
   * Set or clear a breakpoint on the caret's line (Ctrl+F8).
   *
   * Here rather than in the window's key handler because the rule about *where* a breakpoint
   * may go is a property of the buffer, and the buffer is here — routing the shortcut through
   * the store directly would be a second answer to the same question.
   */
  export function toggleBreakpointAtCaret(): boolean {
    const root = projectStore.project?.root;
    if (!root || !activePath || !canBreak) return false;
    if (!breakpointable.has(caretLine)) {
      const existing = bennuDebugStore
        .breakpointsIn(root, activePath)
        .some((b) => b.line === caretLine);
      if (!existing) return false;
    }
    bennuDebugStore.toggleBreakpoint(root, activePath, caretLine);
    return true;
  }

  /** Right-click a breakpoint: enable/disable it, or drop it. On a line with none it just
   *  offers to set one, so the menu is never empty and never lies about what is there. */
  function onBreakpointContext(line: number, e: MouseEvent) {
    const root = projectStore.project?.root;
    if (!root || !activePath || !canBreak) return;
    const path = activePath;
    const existing = bennuDebugStore.breakpointsIn(root, path).find((b) => b.line === line);
    const conditional = !!existing?.condition.trim() || (existing?.hit_count ?? 0) > 1;
    const items: MenuItem[] = existing
      ? [
          // First, because it is the reason to open this menu on a breakpoint that already exists
          // — enabling and removing are both one click away in the gutter itself.
          { id: 'condition', label: conditional ? 'Edit condition…' : 'Add condition…' },
          { id: 'toggle', label: existing.enabled ? 'Disable breakpoint' : 'Enable breakpoint' },
          { id: 'remove', label: 'Remove breakpoint' },
          { id: 'sep', label: '', separator: true },
          { id: 'clear', label: 'Remove all breakpoints in this project' },
        ]
      : [{ id: 'add', label: 'Set breakpoint' }];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      if (id === 'add') bennuDebugStore.toggleBreakpoint(root, path, line);
      else if (id === 'remove') bennuDebugStore.removeBreakpoint(root, path, line);
      else if (id === 'clear') bennuDebugStore.clearBreakpoints(root);
      // The list, focused on this one. A popup hanging off the gutter would be a second place to
      // edit the same thing, and one the keyboard could not reach.
      else if (id === 'condition') bennuUiStore.openBreakpoints({ file: path, line });
      else if (id === 'toggle' && existing) {
        bennuDebugStore.setBreakpointEnabled(root, path, line, !existing.enabled);
      }
    });
  }

  /** Editing moved lines the gutter had dots on — follow them (see the store). */
  function onBreakpointsMoved(moves: readonly { from: number; to: number }[]) {
    const root = projectStore.project?.root;
    if (root && activePath) bennuDebugStore.moveBreakpoints(root, activePath, moves);
  }

  /**
   * The line the debugger is stopped on — a whole-line band, not a mark over the text.
   *
   * Only when the *selected frame* is in this file: the panel drives which frame you are
   * looking at, and highlighting frame 0's line in a file you are not looking at would put a
   * marker where nothing is happening.
   */
  const pausedLine = $derived.by(() => {
    const frame = bennuDebugStore.currentFrame;
    if (!frame?.file || !frame.line || !activePath) return [];
    if (canonFile(frame.file).toLowerCase() !== canonFile(activePath).toLowerCase()) return [];
    return [{ line: frame.line, className: 'cm-paused-line' }];
  });

  /** Map backend byte spans to the editor's UTF-16 offsets + a CSS class per kind. */
  function toSpringMarks(src: string, hs: ExtHighlight[]) {
    const toU16 = makeByteToU16(src);
    return hs.map((h) => ({
      from: toU16(h.start),
      to: toU16(h.end),
      // `spring.placeholder.key` → `cm-fw-spring-placeholder-key`. An unknown kind still
      // gets a class, so a backend that adds one degrades to "styled neutrally" rather
      // than to nothing.
      className: `cm-fw cm-fw-${h.kind.replace(/\./g, '-')}`,
    }));
  }
  // Error / warning counts for the editor's top-right status badge (IntelliJ-style).
  const diagCounts = $derived.by(() => {
    let errors = 0, warnings = 0;
    for (const d of allDiags) {
      if (d.severity === 'error') errors++;
      else if (d.severity === 'warning') warnings++;
    }
    return { errors, warnings };
  });
  $effect(() => { void activePath; mojibakeDiags = []; });

  // Action binding for a JSP view (the reverse view→action picker). Fetched when a JSP is open;
  // `bindingRev` bumps after the user pins/clears an action so the lint + this both re-run.
  let jspBinding = $state<JspActionBinding | null>(null);
  let bindingRev = $state(0);
  $effect(() => {
    const path = activePath;
    void bennuIndexStore.buildRevision;
    void bindingRev;
    if (!path || !isJspFileOf(path)) { jspBinding = null; return; }
    let cancelled = false;
    void ipcJspActions(path).then((b) => { if (!cancelled) jspBinding = b; }).catch(() => { if (!cancelled) jspBinding = null; });
    return () => { cancelled = true; };
  });
  /** Pin (qname) or clear (null) the JSP's bound action, then re-fetch + re-lint. */
  async function selectJspAction(qname: string | null) {
    const path = activePath;
    if (!path) return;
    await ipcSetJspAction(path, qname).catch(() => {});
    bindingRev += 1;
  }
  // The toolbar label for the action picker (the effective action's simple name, or a prompt).
  const jspActionLabel: string | null = $derived.by(() => {
    const b = jspBinding;
    if (!b || !b.candidates.length) return null;
    if (!b.effective) return 'Pick action';
    const opt = b.candidates.find((c) => c.qname === b.effective);
    return opt ? opt.simple : (b.effective.split('/').pop() ?? b.effective);
  });
  // The picker menu: each reverse-lookup candidate (pin it), plus "Auto" to clear the pin. A pinned
  // action shows a check; the sole auto-resolved candidate is effective without a pin.
  const jspActionMenu: DropdownItem[] = $derived.by(() => {
    const b = jspBinding;
    if (!b || !b.candidates.length) return [];
    // Several actions routinely share one implementation class — a legacy config declares the
    // same class under three namespaces — and for THIS question they are one answer: the
    // properties OGNL is checked against come from the class. So one row per class. Three rows
    // reading `DettaglioComunicazioniAction` are not duplicates, they are a choice with no
    // difference, and the label was showing the one thing they have in common while hiding the
    // one thing that separates them.
    const groups = new Map<string, { simple: string; fqcn: string | null; qnames: string[] }>();
    for (const c of b.candidates) {
      const key = c.class_fqcn ?? c.qname;
      const group = groups.get(key);
      if (group) group.qnames.push(c.qname);
      else groups.set(key, { simple: c.simple, fqcn: c.class_fqcn ?? null, qnames: [c.qname] });
    }
    // Two classes of the same simple name in different packages would still read alike; there
    // the package IS the distinguishing fact, so it goes on the row.
    const seen = new Set<string>();
    const ambiguous = new Set<string>();
    for (const g of groups.values()) {
      if (seen.has(g.simple)) ambiguous.add(g.simple);
      seen.add(g.simple);
    }
    const items: DropdownItem[] = [{ kind: 'separator', label: 'Check OGNL against action' }];
    for (const g of groups.values()) {
      items.push({
        kind: 'item',
        id: g.qnames[0],
        label: ambiguous.has(g.simple) ? (g.fqcn ?? g.qnames[0]) : g.simple,
        // Which action(s) the row stands for — the row is otherwise a class name with no
        // route attached, and on a page reached three ways that is the interesting part.
        subtitle: g.qnames.join(' · '),
        active: g.qnames.some((q) => q === b.effective),
        // Any of them pins the same class, so the first is as good an answer as the others.
        onclick: () => void selectJspAction(g.qnames[0]),
      });
    }
    items.push({ kind: 'separator' });
    items.push({ kind: 'item', id: '__auto', label: 'Auto (clear pin)', active: !b.bound, onclick: () => void selectJspAction(null) });
    return items;
  });

  $effect(() => {
    const path = activePath;
    void bennuIndexStore.buildRevision;
    void bindingRev;
    if (!path || isJavaFile) { propertyDiags = []; return; }
    const isValidation = path.toLowerCase().endsWith('-validation.xml');
    if (!isJspFileOf(path) && !isValidation) { propertyDiags = []; return; }
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void ipcActionPropertyLint(path, src)
        .then((hits) => {
          if (cancelled) return;
          propertyDiags = hits.map((h) => ({
            from: h.start,
            to: h.end,
            severity: 'warning' as const,
            message: `“${h.name}” is not a property of action ${h.action}`,
          }));
        })
        .catch(() => { if (!cancelled) propertyDiags = []; });
    }, 350);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // Struts config XML: lint the `<result>` targets (JSP-not-found + OGNL-not-a-property). Only on a
  // non-validation `.xml` (validation.xml is covered by the property lint above).
  $effect(() => {
    const path = activePath;
    void bennuIndexStore.buildRevision;
    const isXml = !!path && path.toLowerCase().endsWith('.xml');
    const isValidation = !!path && path.toLowerCase().endsWith('-validation.xml');
    if (!path || !isXml || isValidation) { strutsDiags = []; return; }
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void ipcStrutsResultLint(path, src)
        .then((hits) => {
          if (cancelled) return;
          strutsDiags = hits.map((d) => ({
            from: d.start,
            to: d.end,
            severity: cmSeverity(d.severity as DiagnosticSeverity),
            message: d.message,
          }));
        })
        .catch(() => { if (!cancelled) strutsDiags = []; });
    }, 350);
    return () => { cancelled = true; clearTimeout(t); };
  });

  function spellDiagOf(h: SpellHit, root: string): EditorDiagnostic {
    const actions = h.suggestions.slice(0, 4).map((s) => ({
      name: `Replace with “${s}”`,
      apply: (view: EditorView, from: number, to: number) =>
        view.dispatch({ changes: { from, to, insert: s } }),
    }));
    actions.push({
      name: 'Add to project dictionary',
      apply: () => void bennuSpellStore.addToDictionary(h.word, 'project', root),
    });
    actions.push({
      name: 'Add to global dictionary',
      apply: () => void bennuSpellStore.addToDictionary(h.word, 'global', root),
    });
    return { from: h.start, to: h.end, severity: 'hint', message: `“${h.word}” may be misspelled`, actions };
  }

  $effect(() => {
    const path = activePath;
    const root = projectStore.project?.root ?? null;
    // Re-run when the dictionaries change (download / add-word).
    void bennuSpellStore.revision;
    if (!path || !bennuSpellStore.activeFor(root)) { spellDiags = []; return; }
    const src = projectStore.sourceOf(path);
    let cancelled = false;
    const t = setTimeout(() => {
      void ipcSpellcheck(path, src)
        .then((hits) => { if (!cancelled) spellDiags = hits.map((h) => spellDiagOf(h, root!)); })
        .catch(() => { if (!cancelled) spellDiags = []; });
    }, 450);
    return () => { cancelled = true; clearTimeout(t); };
  });

  // ── Goto-line overlay (Ctrl+G) ───────────────────────────────────────────────
  let gotoOpen = $state(false);
  let gotoValue = $state('');
  let gotoInputEl = $state<HTMLInputElement | null>(null);

  export function openGoto() {
    if (!activePath) return;
    gotoOpen = true; gotoValue = '';
    queueMicrotask(() => gotoInputEl?.focus());
  }
  /** Open the editor's in-buffer search panel (Ctrl+F when the pane is focused). */
  export function openSearch() { editorComp?.openSearch(); }
  /**
   * Ask for completions at the caret — the explicit request behind `Ctrl+Shift+Space`.
   *
   * The note is cleared first so every press answers for itself: either a popup opens, or a fresh
   * line appears in the footer saying why it did not. A leftover message from the previous press
   * would be the one thing worse than silence — an answer to a question nobody just asked.
   */
  export function requestCompletion() {
    completionNoteStore.clear();
    // After a `tick`, for the same reason `focusEditor` waits: the Command Palette entry closes an
    // overlay in the same gesture, and asking while that overlay is still mounted opens a popup the
    // teardown then blurs shut. From the keyboard nothing is pending and the tick costs a frame.
    void tick().then(() => {
      // `false` means the buffer carries no completion machinery at all — the language has no
      // source, or the view is gone. It is the one outcome that never reaches a completion source,
      // so it is the one the sources cannot report: without this the press would be silent for a
      // reason none of the other messages covers.
      if (editorComp && !editorComp.requestCompletion()) {
        completionNoteStore.say('No completions available in this file');
      }
    });
  }
  /**
   * Put the caret back in the buffer.
   *
   * After a `tick`, deliberately. Every caller is closing something that has just written to the
   * file — a rename, an intention, a generated body — and that write flows back through the store
   * into the editor's controlled value. Focusing before that settles focuses a view the update then
   * takes focus away from again, which is what left the tab drawn as if the pane were inactive
   * until you clicked into the code.
   */
  export function focusEditor() { void tick().then(() => editorComp?.focus()); }
  /** The editor's current selection text ('' when nothing selected) — used by the window to
   *  seed Find-in-project / Go-to navigator fields from what the user highlighted. */
  export function getSelectedText(): string { return editorComp?.getSelectionText() ?? ''; }
  /** The 1-based line the caret is on — what "run the test at the caret" resolves against. */
  export function getCaretLine(): number { return caretLine; }

  /** Grow the selection by one syntactic step (Alt+Shift+Right). */
  export function expandSelection(): Promise<boolean> { return stepSelection(1); }
  /** Shrink it back by one (Alt+Shift+Left). */
  export function shrinkSelection(): Promise<boolean> { return stepSelection(-1); }

  /** Toolbar action on a Java action class: resolve its `<Class>-validation.xml` (naming
   *  convention), create it from a skeleton if missing, open it, and pop the validator chain
   *  builder so the user can add rules straight away. No-op (with a toast) off a Java file. */
  export async function createValidationFile() {
    const file = activePath;
    if (!file) return;
    let target;
    try { target = await ipcValidationTarget(file); } catch { target = null; }
    if (!target) { toastStore.show('Not a Java action class', 'info'); return; }
    if (!target.exists) {
      await projectStore.saveText(target.path, target.content); // write the fresh skeleton
      toastStore.show('Created validation file', 'success');
    }
    await projectStore.openFile(target.path);
    bennuUiStore.openValidationCreator();
  }

  /** Palette "Check file for mojibake": scan the active buffer for UTF-8-as-Cp1252 corruption
   *  (`Ã©` → `é`, `â€™` → `'`) and surface each hit as a warning squiggle with a one-click
   *  replace, plus a summary toast. One-shot (recomputed each run); cleared on file switch. */
  export async function checkMojibake() {
    const path = activePath;
    if (!path || !editorComp) return;
    const src = editorComp.getValue();
    let hits: Awaited<ReturnType<typeof ipcMojibakeCheck>> = [];
    try { hits = await ipcMojibakeCheck(path, src); } catch { hits = []; }
    if (projectStore.activeFilePath !== path) return; // file switched mid-scan → drop
    mojibakeDiags = hits.map((h) => ({
      from: h.start,
      to: h.end,
      severity: 'warning' as const,
      message: `Likely mojibake: “${h.bad}” should be “${h.fix}”`,
      actions: [{
        name: `Replace with “${h.fix}”`,
        apply: (view: EditorView, from: number, to: number) =>
          view.dispatch({ changes: { from, to, insert: h.fix } }),
      }],
    }));
    toastStore.show(
      hits.length
        ? `${hits.length} mojibake sequence${hits.length === 1 ? '' : 's'} found`
        : 'No mojibake found',
      hits.length ? 'warning' : 'success',
    );
  }

  // ── Intentions (Alt+Enter) ────────────────────────────────────────────────────
  /** Collect the context actions at the caret and open the intentions popup
   *  anchored there. No-op (with a toast) when no file is open or the caret has no
   *  anchor. The two "Generate…" items route through `onGenerate`. */
  /** Pick the Alt+Enter list icon for an intention offer by its stable id. */
  function intentionIcon(id: string) {
    if (id === 'log-parameterize') return Braces;
    if (id === 'np-equals') return ArrowLeftRight;
    if (id === 'change-package') return Package;
    if (id === 'move-to-package') return FolderInput;
    if (id === 'override-methods') return Wand2;
    if (id.startsWith('naming-fix:')) return CaseSensitive;
    return Wand2; // the simplification family (isEmpty / boolean / negated comparison)
  }

  /**
   * Run an intention that is an action rather than an edit.
   *
   * The two rename actions differ only in whether the user is shown the plan first, and the BE
   * decides which: a local or a parameter cannot be referred to from outside its file, so its
   * rename is exact and applying it on the spot is the whole gesture. Anything that can reach
   * another file opens the preview with the suggestion filled in — the name is pre-computed either
   * way, so the modal opens on the answer rather than on an empty field.
   */
  async function runIntentionAction(o: IntentionOffer, path: string) {
    if (o.action === 'move-to-package') { await moveFileToPackage(path); return; }
    if (o.action === 'create-class') { await createMissingClass(path, o); return; }
    if (o.action === 'override-methods') { onOverride?.(); return; }
    if (o.action !== 'rename-symbol' && o.action !== 'rename-symbol-preview') return;
    if (!editorComp) return;

    const source = editorComp.getValue();
    // The offer's offsets are bytes (the BE coordinate); the buffer is UTF-16.
    const b2u = makeByteToU16(source);
    const current = source.slice(b2u(o.start), b2u(o.end));
    if (o.action === 'rename-symbol-preview') {
      bennuRefactorStore.openRename({
        file: path,
        source,
        offset: o.start,
        initialName: current,
        suggestedName: o.replacement,
      });
      return;
    }
    try {
      const edits = await ipcRenameApply(path, source, o.start, o.replacement);
      if (!edits.length) { toastStore.show('Nothing to rename here', 'info'); return; }
      const failed = await projectStore.applyEdits(edits);
      if (failed) toastStore.show(`Renamed, but ${failed} file(s) could not be written`, 'error');
      else toastStore.show(`Renamed to “${o.replacement}”`, 'success');
    } catch {
      toastStore.show('Rename failed', 'error');
    }
  }

  /** Move the file into the folder matching its declared package (the `move-to-package` intention).
   *  Delegates to the store (save → move → re-point tab → refresh tree) and reports the outcome. */
  /**
   * Create the file for a type that does not resolve, then open it.
   *
   * Opening it is half the gesture: the file is empty on purpose, so the only reason to make it is
   * to start writing in it, and leaving the user to go and find it in the tree is the difference
   * between a repair and a chore.
   */
  async function createMissingClass(path: string, offer: IntentionOffer) {
    if (!editorComp) return;
    const source = editorComp.getValue();
    try {
      const created = await createClass(path, source, offer.start, offer.end);
      await projectStore.openFile(created);
      toastStore.show(`Created ${created.split('/').pop() ?? created}`, 'success');
    } catch (e) {
      toastStore.show(`Couldn't create the class: ${e}`, 'error');
    }
  }

  async function moveFileToPackage(path: string) {
    try {
      const newPath = await projectStore.moveFileToPackage(path);
      toastStore.show(`Moved to ${newPath}`, 'success');
    } catch (e) {
      toastStore.show(`Couldn't move file: ${e}`, 'error');
    }
  }

  /**
   * A server's code actions, in the order they should be read.
   *
   * Three tiers, and the reason is that they answer different questions. A **fix** answers "this is
   * wrong, make it right" and is why you pressed the key — it goes first, and the one the server
   * marked `preferred` goes first among those, since that is the server saying "this is the obvious
   * one". A **refactoring** answers "this is fine, change its shape". A **disabled** row is neither:
   * it is there to say why, so it sits at the bottom instead of pushing the actionable rows down.
   *
   * Stable within a tier, so the server's own ordering survives — it knows more about relevance at a
   * caret than a sort here could.
   */
  function orderedActions(actions: readonly LspAction[]): LspAction[] {
    const rank = (a: LspAction): number => {
      if (a.disabled) return 3;
      if (a.kind.startsWith('refactor') || a.kind.startsWith('source')) return 2;
      return a.preferred ? 0 : 1;
    };
    return [...actions].sort((x, y) => rank(x) - rank(y));
  }

  export async function openIntentions() {
    if (!activePath || !editorComp) return;
    const path = activePath;
    const anchor = editorComp.coordsAtCaret();
    // Context-aware, Rust-backed intentions resolved by bennu-be in one round-trip: the pure
    // `bennu-intentions` catalog returns every quick-fix applicable at the caret (parameterize
    // logging, NP-safe equals, isEmpty()/boolean/negated-comparison simplifications).
    //
    // Asked for EVERY file, not only a Java one: the naming-convention fix rides in the same
    // answer and covers the languages a server serves too. The handler itself is what knows which
    // of its offers are Java-only — a guard here would have to know that as well, and the two
    // would drift the first time a language-agnostic offer was added.
    const dynamic: IntentionItem[] = [];
    {
      const src = editorComp.getValue();
      const offset = editorComp.caretByteOffset();
      // The diagnostics travel with the request so the offers can include a FIX for the ones under
      // the caret. They are the editor's own, already computed and already drawn — revalidating the
      // file to rediscover them would run every check in it for the sake of one squiggle.
      const offers = await ipcIntentionsAt(path, src, offset, diagRefsForFixes()).catch(() => []);
      if (projectStore.activeFilePath === path) {
        for (const o of offers) {
          dynamic.push({
            id: o.id,
            label: o.label,
            icon: intentionIcon(o.id),
            // A non-edit action is dispatched by whoever owns it — a filesystem move by the
            // store, a rename by the semantic engine (never by splicing the identifier in place,
            // which would leave every use of it behind). A plain edit applies the byte-range
            // replacement.
            run: o.action ? () => void runIntentionAction(o, path) : () =>
              editorComp?.replaceByteRange(o.start, o.end, o.replacement),
          });
        }
      }
    }
    // A language server's code actions ARE this language's Alt+Enter list: for Rust that is
    // "import HashMap", "fill match arms", "add missing lifetime", each computed with full type
    // knowledge. Offered in the same popup as the Java intentions rather than in a second menu —
    // the user's gesture is "what can you do here", and which engine answers is not their problem.
    if (isLspFileOf(path)) {
      const src = editorComp.getValue();
      const sel = editorComp.selectionRange();
      const u2b = makeU16ToByte(src);
      const start = u2b(sel.from);
      const end = u2b(sel.to);
      // Both questions asked at once. Expanding a macro is not a code action — it is
      // rust-analyzer's own request — but "what can you do where I am standing" is one gesture, so
      // it belongs in the same list rather than behind a chord you have to know about.
      //
      // Asked speculatively rather than guessed from the text: the server answers `null` off a
      // macro call straight from its syntax tree, which costs a fraction of the assists request
      // running beside it, and it answers with the *name*. A `ident!`-shaped regex here would both
      // offer the row on `a != b` and miss a caret inside a nested expansion. The answer is also
      // the expansion itself, so picking the row opens the modal with nothing left to wait for.
      const [actions, macro] = await Promise.all([
        lspCodeActions(path, src, start, end).catch(() => []),
        isRustFileOf(path)
          ? lspExpandMacro(path, src, editorComp.caretByteOffset()).catch(() => null)
          : Promise.resolve(null),
      ]);
      if (projectStore.activeFilePath === path) {
        if (macro) {
          dynamic.push({
            id: 'expand-macro',
            label: `Expand ${macro.name}!`,
            icon: Wand2,
            run: () => { macroView = macro; },
          });
        }
        for (const a of orderedActions(actions)) {
          // A disabled action is shown with its reason rather than hidden: "cannot extract:
          // selection crosses a block" tells you what to change, an absent menu item does not.
          const label = a.disabled ? `${a.title} — ${a.disabled}` : a.title;
          dynamic.push({
            id: `lsp:${a.title}`,
            label,
            icon: a.kind.startsWith('refactor') ? Wand2 : Braces,
            run: a.disabled ? () => {} : () => void runCodeAction(path, a),
          });
        }
      }
    }
    // The Java refactorings — extract method / variable / constant, inline variable / method. In the
    // same list as everything else for the same reason the server's assists are: the gesture is
    // "what can you do here", and which engine answers is not the user's problem.
    //
    // Asked with the SELECTION and not the caret, because that is what half of them are about: a
    // run of statements is an extract-method and the same caret with nothing selected is not.
    if (isJavaFileOf(path)) {
      const src = editorComp.getValue();
      const sel = editorComp.selectionByteRange();
      const offers = await refactorings(path, src, sel.start, sel.end).catch(() => []);
      if (projectStore.activeFilePath === path) {
        for (const offer of offers) {
          // A refusal is a row, greyed, carrying its reason — see `bennu/refactor.ts`.
          dynamic.push({
            id: `refactor:${offer.id}`,
            label: offer.reason ? `${offer.label} — ${offer.reason}` : offer.label,
            icon: Wand2,
            run: offer.reason ? () => {} : () => void runRefactoring(path, offer.id),
          });
        }
      }
    }

    // The editor's own two entries — the Generate flows — and Java-only, because that is what they
    // write. Offering them on a `.rs` put "Generate constructor…" and "Generate getters and setters…"
    // above a Rust function, which is not a thing that exists: everything a Rust buffer can be
    // offered comes from the server, above.
    const local = isJavaFileOf(path)
      ? collectIntentions(
          {
            src: projectStore.sourceOf(path),
            wordUnderCaret: editorComp.wordAtCaret(),
            outline: javaOutline(projectStore.sourceOf(path)),
          },
          { onGenerate: (mode) => onGenerate?.(mode) },
        )
      : [];
    const items = [...dynamic, ...local];
    if (!items.length) {
      toastStore.show('No context actions here', 'info');
      return;
    }
    bennuIntentionsStore.openWith(items, anchor);
  }

  /**
   * Apply a chosen refactoring.
   *
   * The plan is asked for **again** here rather than carried from the menu: opening the list and
   * picking a row are two moments, and a keystroke between them would leave the edits describing
   * text that no longer exists. The backend recomputes against the buffer sent with this call, so
   * the offsets always belong to the document they land in.
   *
   * Everything arrives as one `replaceByteRanges`, so the whole refactoring — the call, the moved
   * body, the import — is one undo.
   */
  async function runRefactoring(path: string, id: string) {
    const src = editorComp?.getValue() ?? '';
    const sel = editorComp?.selectionByteRange() ?? { start: 0, end: 0, empty: true };
    let plan: RefactorPlan;
    try {
      plan = await refactorPlan(path, src, sel.start, sel.end, id);
    } catch (e) {
      toastStore.show(String(e), 'error');
      return;
    }
    if (projectStore.activeFilePath !== path) return;
    editorComp?.replaceByteRanges(
      plan.edits.map((e) => ({ startByte: e.start, endByte: e.end, text: e.text })),
    );
    // The introduced name is the one thing worth typing over, so the caret goes there. Nothing is
    // pre-selected: a rename is a separate, undoable gesture, and doing it for the user is how a
    // refactoring becomes something you have to undo twice.
    if (plan.caret !== null) editorComp?.selectByteRange(plan.caret, plan.caret + plan.name.length);
    if (plan.unresolved_type) {
      toastStore.show(
        `Declared with \`var\` — the type could not be resolved${
          bennuIndexStore.indexing ? ' while the index is still building' : ''
        }.`,
        'warning',
      );
    }
  }

  /**
   * Run a language server's code action.
   *
   * Two halves, and both can be present: inline `edits` are applied straight away, and a
   * `command` asks the server to compute the rest — which comes back as a `workspace/applyEdit`
   * and lands through the same path (see the `onServerEdit` effect). Some actions are edits-only,
   * some are command-only, and rust-analyzer uses both.
   */
  async function runCodeAction(path: string, action: LspAction) {
    if (action.file_ops.length) {
      toastStore.show(
        `This action also needs: ${action.file_ops.join(', ')} — Bennu won't move files for a server`,
        'warning',
      );
    }
    if (action.edits.length) {
      await applyServerEdits(action.edits);
    }
    if (action.command) {
      const ok = await lspExecuteCommand(path, action.command, action.arguments);
      if (!ok) toastStore.show(`“${action.title}” could not be run`, 'error');
    }
  }

  /**
   * Format the buffer with the language's own formatter (`rustfmt`, for Rust).
   *
   * Byte-span edits applied through CodeMirror rather than a whole-document replace: the caret
   * keeps its place, the change is one undo step, and a formatter that only touched three lines
   * does not mark the whole file dirty.
   */
  export async function formatDocument() {
    const path = activePath;
    if (!path || !editorComp) return;
    // Java is formatted by Bennu's own formatter and everything else by its server, and the backend
    // routes between them — so the only files with no formatter are the ones that are neither.
    if (!isLspFileOf(path) && !isJavaFileOf(path)) {
      toastStore.show('No formatter for this file type', 'info');
      return;
    }
    const src = editorComp.getValue();
    const edits = await formatBuffer(
      path,
      src,
      bennuSettingsStore.tabSize,
      bennuSettingsStore.indentStyle !== 'tabs',
    ).catch(() => []);
    if (!edits.length) {
      // A formatter that returns nothing has nothing to change — which is the good outcome, and
      // worth saying so the user does not press it again wondering.
      toastStore.show('Already formatted', 'info');
      return;
    }
    editorComp.replaceByteRanges(
      edits.map((e) => ({ startByte: e.start, endByte: e.end, text: e.new_text })),
    );
  }

  /**
   * Drop the imports the file does not use and put the rest in order.
   *
   * Java only — the answer comes from the same `unused-import` judgement the squiggles do, and no
   * other language here has one. One edit over the whole import block, so it is one undo step.
   */
  export async function optimizeImportsInBuffer() {
    const path = activePath;
    if (!path || !editorComp) return;
    if (!isJavaFileOf(path)) {
      toastStore.show('Imports can only be optimized in a Java file', 'info');
      return;
    }
    const edits = await optimizeImports(path, editorComp.getValue()).catch(() => []);
    if (!edits.length) {
      // Deliberately not "already in order": a file with a comment written among its imports is
      // also left alone (reordering would strand the comment above a different import), and
      // claiming it was already tidy would be a lie in that case.
      toastStore.show('No import changes', 'info');
      return;
    }
    editorComp.replaceByteRanges(
      edits.map((e) => ({ startByte: e.start, endByte: e.end, text: e.new_text })),
    );
  }

  /** Insert text at the caret (Generate modal → editor). Mirrors merula's insert. */
  export function insertAtCursor(text: string) { editorComp?.insertAtCursor(text); }

  /** The buffer and the caret in it, in the byte coordinates every backend span uses. `null` when
   *  no editor is mounted — a generator has nothing to work from. */
  export function caretContext(): { source: string; offset: number } | null {
    if (!editorComp) return null;
    return { source: editorComp.getValue(), offset: editorComp.caretByteOffset() };
  }

  /**
   * Apply a generator's byte-range edits as ONE undo step.
   *
   * One step because they are one action: the methods and the imports they need are not two things
   * the user did, and undoing half of it leaves code that does not compile. The backend already
   * ordered them highest-offset-first for callers that apply them one at a time; the editor's own
   * batch API remaps for us, so the order here does not matter.
   */
  export function applyGeneratedEdits(
    edits: readonly { start: number; end: number; replacement: string }[],
  ) {
    if (!editorComp || edits.length === 0) return;
    editorComp.replaceByteRanges(
      edits.map((e) => ({ startByte: e.start, endByte: e.end, text: e.replacement })),
    );
  }

  // ── Rename (Shift+F6) — inline ────────────────────────────────────────────────
  // An IntelliJ-style in-place rename: a small field anchored at the caret, pre-filled
  // with the symbol, that only accepts Java-identifier characters. Enter applies the
  // rename across the project (via `bennu_rename_apply`); Shift+Enter escalates to the
  // full preview modal; Esc cancels. The modal (BennuRenameModal) stays as that
  // preview surface.
  interface InlineRenameCtx { file: string; source: string; offset: number; initialName: string; }
  let renameOpen = $state(false);
  let renameName = $state('');
  let renameCtx = $state<InlineRenameCtx | null>(null);
  let renameAnchor = $state<{ x: number; y: number } | null>(null);
  let renameBusy = $state(false);
  let renameInputEl = $state<HTMLInputElement | null>(null);
  let renameBoxEl = $state<HTMLDivElement | null>(null);

  const JAVA_IDENT = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
  const renameValid = $derived(
    !!renameCtx && JAVA_IDENT.test(renameName) && renameName !== renameCtx.initialName,
  );

  /** Open the inline rename for the symbol under the caret. No-op (with a toast) when
   *  the caret isn't on an identifier. */
  export function openRename() {
    if (!activePath || !editorComp) return;
    const initial = editorComp.wordAtCaret() ?? '';
    if (!initial) { toastStore.show('Place the caret on a symbol to rename', 'info'); return; }
    renameCtx = {
      file: activePath,
      source: editorComp.getValue(),
      offset: editorComp.caretByteOffset(),
      initialName: initial,
    };
    renameAnchor = editorComp.coordsAtCaret();
    renameName = initial;
    renameOpen = true;
    queueMicrotask(() => renameInputEl?.select());
  }

  function closeInlineRename(refocus = true) {
    renameOpen = false;
    renameCtx = null;
    renameAnchor = null;
    if (refocus) focusEditor();
  }

  /** Strip anything that can't appear in a Java identifier as it's typed (spaces, `.`,
   *  `(`, …) so an invalid name can never be entered — a leading digit is still caught
   *  by `renameValid` (the commit stays disabled). */
  function onRenameInput() {
    const cleaned = renameName.replace(/[^A-Za-z0-9_$]/g, '');
    if (cleaned !== renameName) renameName = cleaned;
  }

  async function commitInlineRename() {
    const ctx = renameCtx;
    if (!ctx || !renameValid || renameBusy) return;
    const target = renameName;
    renameBusy = true;
    try {
      const edits = await ipcRenameApply(ctx.file, ctx.source, ctx.offset, target);
      if (!edits.length) { toastStore.show('Nothing to rename here', 'info'); closeInlineRename(); return; }
      // The splicing is the store's (see `applyEdits`) — this only counts what it was asked to do,
      // for a message that says how far the rename reached.
      const files = new Set(edits.map((e) => e.file)).size;
      const failed = await projectStore.applyEdits(edits);
      if (failed) {
        toastStore.show(`Renamed, but ${failed} file(s) could not be written`, 'error');
      } else {
        toastStore.show(
          `Renamed to “${target}” · ${edits.length} edit(s) in ${files} file(s)`,
          'success',
        );
      }
      closeInlineRename();
    } catch {
      toastStore.show('Rename failed', 'error');
    } finally {
      renameBusy = false;
    }
  }

  /** Escalate to the full preview modal (BennuRenameModal), carrying the caret context. */
  function openRenamePreview() {
    const ctx = renameCtx;
    closeInlineRename(false);
    if (ctx) bennuRefactorStore.openRename(ctx);
  }

  function onRenameKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && e.shiftKey) { e.preventDefault(); e.stopPropagation(); openRenamePreview(); }
    else if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); void commitInlineRename(); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeInlineRename(); }
  }

  /** Close the inline field when focus leaves it (unless it moves to the Preview
   *  button inside the same box). */
  function onRenameBlur(e: FocusEvent) {
    const box = renameBoxEl;
    if (box && e.relatedTarget instanceof Node && box.contains(e.relatedTarget)) return;
    closeInlineRename(false);
  }

  // ── Find usages (Alt+F7) ──────────────────────────────────────────────────────

  /** Core find-usages: query `bennu_references` at an explicit byte `offset` and fill the
   *  popover anchored at `anchor`, labelled `word`. Graceful: an unresolvable target shows
   *  the empty state, never throws. Shared by the caret-driven `findUsages` (Alt+F7) and
   *  the Ctrl+Click-on-a-declaration fallback (which passes the CLICK offset, not the
   *  caret). */
  async function runFindUsages(
    source: string,
    offset: number,
    anchor: { x: number; y: number } | null,
    word: string | null,
    ref: string | null = null,
  ) {
    if (!activePath) return;
    bennuRefactorStore.startUsages(anchor, word);
    try {
      // JSP/XML: the Java reference index is meaningless here. Resolve a page-scoped JSP
      // variable first (a `<c:set var>`/`${var}` under the caret → all its references),
      // then fall back to a Struts action reference (`action="…"` → every JSP that uses it).
      //
      // A server-backed buffer skips this branch: `bennu_references` is the right call for it
      // (the backend routes it to the server), and the JSP resolvers below would be asked about
      // a page that does not exist.
      if (!isJavaFile && !isLspBuffer) {
        const nav = await ipcJspNav(activePath, source, offset).catch(() => null);
        if (nav && nav.usages.length) {
          bennuRefactorStore.setUsages(nav.label, nav.usages);
          return;
        }
        if (ref) {
          const av = await ipcActionUsages(activePath, ref).catch(() => null);
          if (av && av.usages.length) {
            bennuRefactorStore.setUsages(av.target_label, av.usages);
            return;
          }
        }
        bennuRefactorStore.setUsages(nav?.label ?? null, nav?.usages ?? []);
        return;
      }

      // Inside a library source view the buffer is under no project root, so the backend
      // cannot tell from its path which index holds the use sites — it needs to be told.
      //
      // The registered context is the precise answer but not always present: a tab restored
      // from a previous session, or one reached through the library→library chain, has the
      // view without the registration. `isDecompiledView` recognises those (it also tests the
      // path), and the open project's root serves as the origin just as well — the backend
      // only uses it to pick a slot, and any path inside the project picks the same one.
      const origin = decompiledCtx?.originFile
        ?? (isDecompiledView ? projectStore.project?.root : undefined);
      const res = await ipcReferences(activePath, source, offset, origin);
      if (res && res.usages.length) {
        bennuRefactorStore.setUsages(res.target_label, res.usages);
        return;
      }
      bennuRefactorStore.setUsages(res?.target_label ?? null, res?.usages ?? []);
    } catch {
      bennuRefactorStore.setUsages(null, []);
    }
  }

  /** Find usages of the symbol under the caret (Alt+F7) — opens the popover anchored
   *  there and fills it from `bennu_references`. */
  /**
   * Usages of the **component this file is** — the answer `findUsages` cannot give.
   *
   * A `.svelte` file has no declaration inside it to put the caret on: the file *is* the
   * component, and `<Foo />` in another file refers to the module rather than to anything
   * written here. Its server bridges that with a generated TypeScript shim, and the shim's
   * declaration sits at **the very start of the file** — so asking for references at offset 0 is
   * asking about the component, and it answers with the imports and the tags.
   *
   * A separate verb rather than a fallback inside Alt+F7: an Alt+F7 that quietly changes its
   * subject when the caret is not on a name would be answering a question nobody asked.
   */
  export async function findComponentUsages() {
    if (!activePath || !editorComp) return;
    await runFindUsages(
      editorComp.getValue(),
      0,
      editorComp.coordsAtCaret(),
      activePath.split(/[\\/]/).pop()?.replace(/\.svelte$/, '') ?? null,
    );
  }

  export async function findUsages() {
    if (!activePath || !editorComp) return;
    await runFindUsages(
      editorComp.getValue(),
      editorComp.caretByteOffset(),
      editorComp.coordsAtCaret(),
      editorComp.wordAtCaret(),
      // The reference token (a JSP `action="…"` value) for the Struts-action fallback.
      editorComp.refAtCaret(),
    );
  }

  // ── Macro expansion (Alt+Shift+M) ───────────────────────────────────────────────

  /** The expansion on screen, or null. Editor-owned because it is about the caret. */
  let macroView = $state<LspMacroExpansion | null>(null);

  /**
   * Expand the macro at the caret and show it.
   *
   * `quiet` is for the modal's own Re-expand: an explicit gesture deserves to be told when the caret
   * is not in a macro, but a re-expand that finds nothing should leave what is on screen alone rather
   * than replacing an expansion you were reading with an empty modal.
   */
  async function expandMacroAtCaret(quiet = false) {
    const path = activePath;
    if (!path || !editorComp || !isLspFileOf(path)) return;
    const found = await lspExpandMacro(path, editorComp.getValue(), editorComp.caretByteOffset())
      .catch(() => null);
    if (!found) {
      if (!quiet) toastStore.show('No macro call at the caret', 'info');
      return;
    }
    macroView = found;
  }

  /** Show what the macro at the caret expands to (Alt+Shift+M). */
  export function expandMacro() { void expandMacroAtCaret(); }

  // ── Call / type hierarchy (Ctrl+Shift+H / Ctrl+H) ──────────────────────────────

  /**
   * Build a hierarchy from the caret and show it in the bottom dock.
   *
   * The panel is opened first, before the answer is in: the tree takes a round-trip to prepare and
   * another per level, and a key that appears to do nothing for a second reads as a key that does
   * nothing. It shows its own "building" state, and says so if the caret was not on something a
   * hierarchy can be built from.
   */
  async function showHierarchy(kind: 'calls' | 'types') {
    // Java is answered by Bennu's own engine over the reference index and everything else by its
    // server; the backend routes between them, so the only files with no hierarchy are the ones
    // that are neither. Same shape as `formatDocument`.
    if (!activePath || !editorComp) return;
    if (!isLspFileOf(activePath) && !isJavaFileOf(activePath)) return;
    bennuUiStore.showBottom('hierarchy');
    await bennuHierarchyStore.open(
      kind,
      activePath,
      editorComp.getValue(),
      editorComp.caretByteOffset(),
    );
  }

  /** Who calls the function at the caret (and, by direction, what it calls). */
  export function showCallHierarchy() { void showHierarchy('calls'); }
  /** What implements the type at the caret (and, by direction, what it is built on). */
  export function showTypeHierarchy() { void showHierarchy('types'); }

  // ── Go to definition (Ctrl+B / Ctrl+Click) ────────────────────────────────────
  //
  // Resolves the JSP form/link **action reference** under the caret/click to its
  // definition via `bennu_definition` (the config fragment the `<action>` is
  // declared in + the class FQCN + the view JSP). We jump to the openable file
  // target — the config fragment, falling back to the view JSP — and surface the
  // class FQCN (a name, not a path) as info when that's all we have. Graceful:
  // an unresolvable ref just toasts, never throws.

  /** Guard against overlapping resolves (a fast double Ctrl+B): only the latest
   *  wins, so a stale result never yanks the editor to the wrong file. */
  let gotoDefSeq = 0;

  /** Open a resolved target file and scroll to the definition site. When a byte `offset` is
   *  known (the `<action>` element in the config fragment) we jump there so go-to lands on
   *  the declaration line, not the top of the file; else we scroll to the top. The goto relay
   *  drives the scroll after the cross-file open settles. */
  function openDefinitionFile(path: string, offset?: number) {
    // A go-to target may be a URL rather than a file: an XML document names its schema by
    // address, and no local copy of it may exist. Download it and open it HERE rather than in a
    // browser — not for convenience, but because the cached copy joins the catalog, so the file
    // that named it stops being answered by a fallback grammar and starts being answered by the
    // real one. The browser is only the consolation prize when the fetch fails.
    if (/^https?:\/\//i.test(path)) {
      void xmlFetchSchema(path)
        .then((local) => projectStore.openFile(local).then(() => extRefresh(projectStore.project?.root ?? '')))
        .catch((e) => {
          toastStore.show(`Could not download the schema — ${String(e)}`, 'error');
          void openUrl(path).catch(() => {});
        });
      return;
    }
    void projectStore.openFile(path).then(() => {
      if (offset && offset > 0) bennuUiStore.requestGotoOffset(offset);
      else bennuUiStore.requestGoto(1);
    });
  }

  /** Try the BE go-to-declaration for the symbol at `offset`. Resolves via `bennu_declaration`
   *  — which the backend answers with whichever engine owns the file, Bennu's Java index or a
   *  language server — and jumps to the declaring file + line. When the click/caret is
   *  **already on the declaration itself** (its name token in this same file — a method
   *  signature, or a variable/class/record decl), go-to-declaration would be a no-op, so we
   *  fall back to **find usages** at that offset (IntelliJ's Ctrl+Click / Ctrl+B behaviour on a
   *  declaration). Returns true when it handled the gesture; false (gracefully) when the BE
   *  isn't attached, the symbol is JDK/dep-jar resident, or the caret isn't on a symbol. */
  async function tryGoToDeclarationBE(offset: number, word: string | null): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp) return false;
    // Only in a buffer some engine can resolve. On a JSP/XML the Java resolver would parse the
    // text as Java and could mis-fire on a coincidental symbol name (the `viewTree` inside
    // `action="viewTree"` matching a Java method), hijacking the gesture before the
    // config-graph resolver (`bennu_definition`) gets its turn — so those keep going through
    // the chain below instead.
    const resolvable = path.toLowerCase().endsWith('.java') || isLspFileOf(path);
    if (!resolvable) return false;
    const source = editorComp.getValue();
    const target = await ipcDeclaration(path, source, offset).catch(() => null);
    if (!target) return false;
    // On the declaration's own name span (same file, offset inside [start,end))? → usages.
    if (isSamePath(target.file, path) && offset >= target.start && offset < target.end) {
      await runFindUsages(source, offset, editorComp.coordsAtByteOffset(offset), word || null);
      return true;
    }
    await projectStore.openFile(target.file);
    bennuUiStore.requestGoto(target.line);
    return true;
  }

  /** Try JSP **page-scoped variable** go-to for the caret at `offset` — a `<c:set var>` /
   *  `<s:set var>` / `<c:forEach var>` declaration or an `${var}` / `%{var}` reference.
   *  Resolves via `bennu_jsp_nav`; everything is in THIS file (JSP variables are
   *  page-scoped), so we just move the caret to the declaration — no cross-file open, no
   *  project index needed. If the caret is already ON the declaration, jumping is a no-op, so
   *  fall back to find-usages (IntelliJ behaviour). Returns true when it handled the gesture. */
  async function tryGoToJspVar(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || isJavaFile) return false;
    const source = editorComp.getValue();
    const nav = await ipcJspNav(path, source, offset).catch(() => null);
    if (!nav || !nav.declaration) return false;
    const d = nav.declaration;
    // Already on the declaration's own name span → show usages instead of a no-op jump.
    if (offset >= d.start && offset < d.end) {
      await runFindUsages(source, offset, editorComp.coordsAtByteOffset(offset), null);
      return true;
    }
    editorComp.scrollToLineCol(d.line, d.col);
    return true;
  }

  /** Try go-to from a JSP form field / OGNL root (a `<s:textfield name="…">` etc.), or a
   *  `*-validation.xml` `<field name="…">`, under the caret to the **action class's** accessor for
   *  that property. Resolves via `bennu_action_property_target` (the form's action → class → its
   *  `get`/`set`/`is` for the property). Non-`.java` buffers only; `null` (→ false) when the caret
   *  isn't on a resolvable field so the include / action resolvers get their turn. */
  async function tryGoToActionProperty(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || isJavaFile) return false;
    const source = editorComp.getValue();
    const t = await ipcActionPropertyTarget(path, source, offset).catch(() => null);
    if (!t) return false;
    openDefinitionFile(t.file, t.start);
    return true;
  }

  /** Try **Struts `<result>`** go-to for the caret in a config XML — a JSP path (`/WEB-INF/x.jsp`)
   *  opens that JSP; an OGNL/EL root (`${prop}`) jumps to the owning action's property accessor.
   *  Resolves via `bennu_struts_result_target`; a non-result token resolves to null → false. */
  async function tryGoToStrutsResult(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || !path.toLowerCase().endsWith('.xml')) return false;
    const t = await ipcStrutsResultTarget(path, editorComp.getValue(), offset).catch(() => null);
    if (!t) return false;
    openDefinitionFile(t.file, t.start);
    return true;
  }

  /** Last-resort go-to into a **library/JDK type** — a class/interface with no project source. The
   *  backend generates a signature-only Java stub from the class's bytecode (with a "decompiled"
   *  header) and this opens it. `name` is the type name under the caret (simple or FQCN). */
  async function tryGoToDecompiled(name: string): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || !name) return false;
    const src = editorComp.getValue();
    const loc = await ipcDecompiledSource(path, src, name).catch(() => null);
    if (!loc) return false;
    // Remember the origin (file/buffer/type) so the tab's "Download sources" button can fetch the
    // dependency's real sources against the same imports.
    decompiledStore.register(loc.file, {
      originFile: path,
      originSource: src,
      name,
      canDownload: loc.can_download,
    });
    openDefinitionFile(loc.file, loc.offset);
    return true;
  }

  /** Go-to from a **project file** into a library/JDK **member or type** — `list.add(…)` lands on
   *  `add` in `ArrayList`'s source view, not merely on the class.
   *
   *  The same backend call as {@link tryGoToLibraryNav}; the only difference is which file names
   *  the project. `library_declaration` takes `origin_file` purely to pick the project's classpath
   *  resolver and makes no assumption that the buffer is itself a library view, so a project buffer
   *  works unchanged — the resolution was simply never offered from here. */
  async function tryGoToLibraryMember(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || !isJavaFile) return false;
    const loc = await ipcLibraryDeclaration(path, editorComp.getValue(), offset).catch(() => null);
    if (!loc) return false;
    // Tracked with the same origin project, so the opened view is read-only and itself navigable.
    // No download banner: that needs the TYPE name to re-resolve the artifact, and the word under
    // the caret here is a member (`add`) — offering a button that cannot work is worse than not
    // offering it. Reaching the type by name still gets its banner (`tryGoToDecompiled`).
    decompiledStore.register(loc.file, {
      originFile: path,
      originSource: '',
      name: '',
      canDownload: false,
    });
    openDefinitionFile(loc.file, loc.offset);
    return true;
  }

  /** Go-to WITHIN a library/JDK source view: resolve the caret against the origin project's
   *  classpath resolver and open the target's source view (member-precise). Registers the target
   *  with the SAME origin project so navigation chains library → library. */
  async function tryGoToLibraryNav(offset: number): Promise<boolean> {
    const ctx = decompiledCtx;
    if (!ctx || !editorComp) return false;
    const loc = await ipcLibraryDeclaration(ctx.originFile, editorComp.getValue(), offset).catch(() => null);
    if (!loc) return false;
    // The target is another library view — track it (same origin project) so it's read-only and
    // itself navigable. A chained target doesn't carry the type name for the download button, so its
    // banner is suppressed (canDownload: false); the initial go-to tab keeps its download banner.
    decompiledStore.register(loc.file, {
      originFile: ctx.originFile,
      originSource: '',
      name: '',
      canDownload: false,
    });
    openDefinitionFile(loc.file, loc.offset);
    return true;
  }

  /** The current tab's decompiled-view context (present only for a JDK/library source view), and
   *  whether it's a read-only decompiled path (by the data-dir `/decompiled/` segment — robust for
   *  restored tabs too). */
  const decompiledCtx = $derived(decompiledStore.ctx(activePath));
  // A decompiled view lives in the data dir (never under a project root → always "foreign"); the
  // `isForeign` guard means a real project package literally named `decompiled` isn't made read-only.
  const isDecompiledView = $derived(
    !!decompiledCtx ||
      ((activePath?.includes('/decompiled/') ?? false) && projectStore.isForeign(activePath ?? '')),
  );

  /** Fetch the dependency's real sources for the current decompiled tab (a tracked backend job). On
   *  success the backend emits `sources-ready`, which reloads the tab with the real source. */
  async function downloadTabSources() {
    const path = activePath;
    const ctx = decompiledCtx;
    if (!path || !ctx || decompiledStore.isDownloading(path)) return;
    decompiledStore.markDownloading(path);
    try {
      // `path` is the open decompiled tab — the backend echoes it back to reload / clear the spinner.
      await ipcDownloadSources(ctx.originFile, ctx.originSource, ctx.name, path);
      toastStore.show('Downloading sources…', 'info');
    } catch (e) {
      decompiledStore.clearDownloading(path);
      toastStore.show(`Couldn't download sources: ${e}`, 'error');
    }
  }

  /** Try JSP **include / view reference** go-to for the token under the caret — a path in a
   *  `<%@ include file>` directive, `<jsp:include page>`, `<s:include value>`, `<c:import url>`,
   *  … pointing at another JSP view/fragment. Resolves via `bennu_jsp_include_target` (absolute
   *  paths against the webapp root, relative against the JSP's own dir) and opens the referenced
   *  file. Only runs for non-`.java` buffers; a computed / external / non-existent path resolves
   *  to `null` and this returns false so the action resolver gets its turn. */
  async function tryGoToJspInclude(ref: string): Promise<boolean> {
    const path = activePath;
    if (!path || isJavaFile || !ref) return false;
    const target = await ipcJspIncludeTarget(path, ref).catch(() => null);
    if (!target) return false;
    await projectStore.openFile(target);
    bennuUiStore.requestGoto(1);
    return true;
  }

  /** Try to resolve `word` to a project CLASS declaration — an instant, offline fallback
   *  (from the FE class index). Accepts a **simple name** (`FooAction`) or a dotted **FQCN**
   *  (`com.x.FooAction`, as a struts/spring `class="…"` carries): a simple name matches by
   *  `simple` then by an FQCN ending in `.word`; an FQCN matches exactly then by its last
   *  segment. Jumps to the declaring file + line. Returns true when it jumped. */
  async function tryGoToClassDeclaration(word: string): Promise<boolean> {
    const root = projectStore.project?.root;
    if (!root || !word) return false;
    const isFqcn = /^[A-Za-z_$][A-Za-z0-9_$.]*$/.test(word) && word.includes('.');
    const isSimple = /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(word);
    if (!isFqcn && !isSimple) return false;
    const classes = await bennuIndexStore.classesForRoot(root).catch(() => null);
    if (!classes) return false;
    const last = word.split('.').pop() ?? word;
    const hit = isFqcn
      ? (classes.find((c) => c.fqcn === word) ?? classes.find((c) => c.simple === last))
      : (classes.find((c) => c.simple === word) ?? classes.find((c) => c.fqcn.endsWith('.' + word)));
    if (!hit) return false;
    await projectStore.openFile(hit.file);
    bennuUiStore.requestGoto(hit.line);
    return true;
  }

  /** Try go-to on a **config XML** (struts/spring/tiles `.xml`) reference under the caret —
   *  a `class="…"` value that is either an FQCN (handled by {@link tryGoToClassDeclaration})
   *  or a Spring **bean id**. Resolves the bean id to its impl FQCN via `bennu_bean_class`,
   *  then opens that class. Returns true when it jumped. Only meaningful in an `.xml` file. */
  async function tryGoToXmlClass(ref: string): Promise<boolean> {
    const path = activePath;
    if (!path || !ref || !path.toLowerCase().endsWith('.xml')) return false;
    // A dotted FQCN opens directly from the class index.
    if (await tryGoToClassDeclaration(ref)) return true;
    // Otherwise treat it as a Spring bean id → resolve to its impl class, then open it.
    const fqcn = await ipcBeanClass(path, ref).catch(() => null);
    if (fqcn) return tryGoToClassDeclaration(fqcn);
    return false;
  }

  /**
   * Try **framework-extension** navigation for the caret at `offset`.
   *
   * One call covers every framework target because the backend resolves them behind one
   * seam: a `${property}` key, a `@Qualifier` / SpEL `@bean` reference, an injected
   * field's candidate beans, — in a bean XML — `class=`, `ref=` and `<property name=>`, and
   * across the Bevy material seam: a `"shaders/x.wgsl"` inside an `impl Material` → the shader,
   * and a `.wgsl` → the materials that run it. Empty on a caret that is none of those, which is
   * most of a file.
   *
   * With several candidates it **asks**, anchoring the menu at the caret — a bean with six
   * injection points has six real answers, and picking one silently hides that there were
   * others and lands on the one we ranked rather than the one you meant.
   */
  async function tryGoToFrameworkExt(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp) return false;
    const targets = await extNavigate(path, editorComp.getValue(), offset).catch(() => []);
    if (targets.length === 0) return false;
    if (targets.length > 1) {
      const at = editorComp.coordsAtByteOffset(offset);
      if (at) {
        showTargetPicker(targets, at.x, at.y);
        return true;
      }
    }
    openDefinitionFile(targets[0].file, targets[0].offset);
    return true;
  }

  /** Try **MyBatis mapper-XML** navigation for the caret at `offset` — a statement `id` → the
   *  Java interface method (XML→Java), the mapper `namespace` → the interface type, an
   *  `<include refid>` → its `<sql>` fragment, a statement `resultMap="…"` → its `<resultMap>`.
   *  Resolves via `bennu_mybatis_nav`, which classifies purely by offset (no ref token needed).
   *  Intra-file targets move the caret here; a Java target opens the `.java` and jumps to the
   *  method/type line. Only meaningful in a mapper `.xml` (a non-mapper XML resolves to null →
   *  false, so the struts/spring `class=` resolver gets its turn). Returns true when it jumped. */
  async function tryGoToMybatis(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp || !path.toLowerCase().endsWith('.xml')) return false;
    const t = await ipcMybatisNav(path, editorComp.getValue(), offset).catch(() => null);
    if (!t) return false;
    if (!isSamePath(t.file, path)) {
      await projectStore.openFile(t.file);
      if (t.offset > 0) bennuUiStore.requestGotoOffset(t.offset);
      else bennuUiStore.requestGoto(t.line);
    } else if (t.offset > 0) {
      editorComp?.scrollToByteOffset(t.offset);
    } else if (t.line > 0) {
      editorComp?.scrollToLineCol(t.line, 1);
    }
    return true;
  }

  /** Resolve + navigate to the declaration of the symbol / `action` under the caret/click.
   *  Prefers the config fragment, then the view JSP; if only a class FQCN is known (no
   *  openable path), reports it. `silent` suppresses the "nothing found" feedback — used
   *  for **Ctrl+click**, where a click on any random token shouldn't pop a toast (IntelliJ
   *  stays quiet there); an explicit **Ctrl+B / palette** invocation keeps the feedback. */
  async function resolveDefinition(action: string, offset?: number, silent = false) {
    // The whole resolution behind one in-progress mark. It is a chain of round trips —
    // and the last of them, a library type, has to be found on the classpath and read
    // out of an archive — so it can run for seconds with nothing on screen changing.
    // A `finally` and not an end-of-function call: the chain returns from a dozen
    // places, and a mark that outlives its navigation is a spinner that never stops.
    const token = bennuUiStore.beginNavigation(action || 'declaration');
    try {
      await resolveDefinitionIn(action, offset, silent);
    } finally {
      bennuUiStore.endNavigation(token);
    }
  }

  /** The body of {@link resolveDefinition} — every resolution step, in order. */
  async function resolveDefinitionIn(action: string, offset?: number, silent = false) {
    const path = activePath;
    if (!path) return;
    // 0. Current tab IS a library/JDK source view → resolve the caret against the ORIGIN project's
    //    classpath resolver and open the target's source view (member-precise), chaining library →
    //    library. Runs BEFORE the project engine (a `/decompiled/` path ends in `.java` and would
    //    otherwise route into the project declaration and get nothing).
    if (offset != null && decompiledCtx && (await tryGoToLibraryNav(offset))) return;
    // 1. BE go-to-declaration — any Java symbol (class/method/field/local) — when we have
    //    a byte offset to classify at. Authoritative + precise (jumps to the exact line).
    if (offset != null && (await tryGoToDeclarationBE(offset, action))) return;
    // 1a. Framework extensions, BEFORE the server-backed stop below — because one of them now
    //     has something to say about a Rust file, and about a shader. A `"shaders/x.wgsl"`
    //     inside an `impl Material` is a string literal: rust-analyzer answers nothing about the
    //     inside of one by construction, and this is the only step that can. Same in the other
    //     direction, from a `.wgsl` to the materials that run it.
    //
    //     After the language's own answer, so a real symbol still wins; before the stop, because
    //     the stop is what kept this unreachable on exactly the two file kinds it is for.
    if (offset != null && (await tryGoToFrameworkExt(offset))) return;
    // 1b. A server-backed buffer stops here. Everything below is a Java-stack resolver — a JSP
    //     page variable, a Struts action, a MyBatis statement, a library class from the
    //     classpath — and none of them has anything to say about a Rust file. Falling through
    //     would spend five round-trips to reach the same "nothing", and the last of them
    //     (`tryGoToDecompiled`) would ask the classpath for a type named after whatever word the
    //     caret happened to be on.
    if (isLspBuffer) {
      if (!silent) {
        const status = bennuLspStore.statusFor(path);
        // The reason matters here: "nothing to go to" and "the server is still indexing" look
        // identical to the user and mean completely different things.
        if (status && status.state === 'starting') {
          toastStore.show(`${status.name} is still starting — try again in a moment`, 'info');
        } else if (status && status.state === 'failed') {
          toastStore.show(status.message || `${status.name} is not running`, 'warning');
        } else {
          toastStore.show('Nothing to go to here', 'info');
        }
      }
      return;
    }
    // 1b. JSP page-scoped variable (a `<c:set var>`/`${var}` under the caret) — single-file,
    //     resolved off the buffer with no project index (only runs for non-`.java` files).
    if (offset != null && (await tryGoToJspVar(offset))) return;
    // 1b-bis. JSP form field / OGNL root, or a validation `<field name>`, → the action class's
    //     property accessor (its `get`/`set`/`is`). Non-`.java` files; a page var (above) wins first.
    if (offset != null && (await tryGoToActionProperty(offset))) return;
    // 1c. JSP include / view reference (a `<%@ include file>` / `<jsp:include page>` /
    //     `<s:include value>` path under the caret) — a `.jsp`/`.jspf` path is unambiguous, so
    //     resolve it before the Struts-action ref (they're disjoint). Non-`.java` files only.
    if (action && (await tryGoToJspInclude(action))) return;
    // 1c-bis. MyBatis mapper XML: a statement `id` → the Java interface method, `namespace` →
    //     the interface, an `<include refid>` → its `<sql>`, a `resultMap="…"` → its
    //     `<resultMap>`. Offset-classified (needs no ref token); a non-mapper XML is a no-op.
    if (offset != null && (await tryGoToMybatis(offset))) return;
    // 1c-ter. Struts config XML: a `<result>` body — a JSP path (open it) or an OGNL `${prop}` (jump
    //     to the owning action's property). Offset-classified; a non-result token is a no-op.
    if (offset != null && (await tryGoToStrutsResult(offset))) return;
    // 1d. Config XML (`struts.xml`/`spring-*.xml`/`tiles.xml`): a `class="…"` value — an FQCN
    //     or a Spring bean id — go to the Java class it names.
    if (action && (await tryGoToXmlClass(action))) return;
    // 2. Instant offline class-index fallback (types) when the BE resolver is cold.
    if (action && (await tryGoToClassDeclaration(action))) return;
    // 2b. A library/JDK **member** — `list.add(…)`, `LOGGER.info(…)`, `cipher.doFinal(…)`. The
    //     caret's receiver is typed against this project's classpath and the target is served
    //     member-precise, so it lands on the method rather than at the top of the class. Runs
    //     before the name-only fallback below, which can look up a TYPE name and nothing else:
    //     on `list.add` the word under the caret is `add`, which is no type, so without this
    //     step every library member go-to quietly did nothing.
    if (offset != null && (await tryGoToLibraryMember(offset))) return;
    if (action) {
      // A library/JDK type with no project source → its decompiled-from-bytecode stub.
      if (await tryGoToDecompiled(action)) return;
    } else {
      if (!silent) toastStore.show('Nothing to go to here', 'info');
      return;
    }
    const seq = ++gotoDefSeq;
    let res;
    try {
      // Pass the live JSP buffer + caret so the BE can fold an enclosing `<s:url namespace="…">`
      // onto a relative `action` when the bare name alone is ambiguous.
      const jspSrc = isJspFileOf(path) ? projectStore.sourceOf(path) : undefined;
      res = await ipcDefinition(path, action, jspSrc, offset);
    } catch {
      if (!silent && seq === gotoDefSeq) toastStore.show('Go to declaration unavailable', 'info');
      return;
    }
    if (seq !== gotoDefSeq) return; // superseded by a newer request
    if (!res) {
      if (!silent) toastStore.show(`No declaration for “${action}”`, 'info');
      return;
    }
    // Prefer the config fragment (where the <action> is declared); fall back to the
    // resolved view JSP. Both are file paths we can open. When jumping to the config
    // fragment, land on the `<action>` element (`config_offset`), not the top of the file.
    if (res.config_file) {
      openDefinitionFile(res.config_file, res.config_offset);
      return;
    }
    if (res.view_jsp) {
      openDefinitionFile(res.view_jsp);
      return;
    }
    // Only a class FQCN resolved — try to open that class from the index, then its decompiled stub;
    // else report it.
    if (res.class_fqcn) {
      if (await tryGoToClassDeclaration(res.class_fqcn)) return;
      if (await tryGoToDecompiled(res.class_fqcn)) return;
      if (!silent) toastStore.show(`Maps to ${res.class_fqcn}`, 'info');
      return;
    }
    if (!silent) toastStore.show(`No declaration for “${action}”`, 'info');
  }

  /** Go to declaration of the symbol / action reference under the caret (Ctrl+B / palette).
   *  No-op (with a toast) when nothing reference-like is under the caret. */
  export function goToDefinition() {
    if (!activePath || !editorComp) return;
    const ref = editorComp.refAtCaret() ?? editorComp.wordAtCaret() ?? '';
    void resolveDefinition(ref, editorComp.caretByteOffset());
  }

  /** Ctrl/Cmd+Click seam from the editor: the reference token at the click position (an
   *  identifier, a string-literal's contents, or a path) + the clicked byte offset → go
   *  to declaration/definition. **Silent** on failure — a Ctrl+click that lands on nothing
   *  resolvable just does nothing, rather than popping a toast every time. */
  function onEditorGoto(word: string, _view: EditorView, byteOffset: number) {
    if (!activePath) return;
    void resolveDefinition(word, byteOffset, true);
  }

  // ── Editor context menu (right-click) ─────────────────────────────────────────
  /** Generate (constructors/getters/setters) is Java-only — its source scan is a Java
   *  outline, meaningless (and historically a freeze risk) on a `.jsp`/XML file. */
  const isJavaFile = $derived(isJavaFileOf(activePath));
  const isJspFile = $derived(isJspFileOf(activePath));
  /** A buffer a language server owns. Its navigation goes through the shared handlers (which the
   *  backend routes), so what this gates is the JSP/Struts/XML resolvers that must NOT be asked
   *  about it. */
  const isLspBuffer = $derived(isLspFileOf(activePath));
  /** Struts is on this project at all. The `*-validation.xml` tooling means nothing without it —
   *  a toolbar button that would create a file no framework reads is a button that teaches the
   *  wrong thing about the project. */
  const hasStruts = $derived(
    projectStore.capabilities?.struts_xml_config === true
      || projectStore.capabilities?.struts_convention === true,
  );
  /** Icon key → glyph, for a contributed toolbar action. An extension names what the action
   *  IS (`column`, `clock`, `search`); the mapping to a picture is the frontend's, and an
   *  unknown key falls back rather than leaving a hole — so an extension can add an action
   *  without waiting for a UI build to learn its icon. */
  const ACTION_ICONS: Record<string, typeof Database> = {
    database: Database,
    columns: Columns3,
    column: ListPlus,
    clock: Clock,
    query: FileCode2,
    search: SearchCode,
    pencil: SquarePen,
  };
  const actionIcon = (kind: string) => ACTION_ICONS[kind] ?? Wand2;

  /** Buttons the loaded plugins put on THIS file's toolbar.
   *
   *  A plugin declares which files it belongs on (`path_pattern`), so the bar keeps meaning
   *  "what kind of file is this" rather than growing a fixed row of plugin icons. Nothing
   *  here knows what any of them do — the host fires the action and the plugin decides. */
  const pluginToolbar = $derived(
    editorToolbarButtons(contributionStore.forPoint(EDITOR_TOOLBAR_POINT), activePath),
  );

  /** A dropdown built from one contributed action's children. */
  function actionMenu(a: ExtAction): DropdownItem[] {
    return a.children.map((c) => ({
      kind: 'item' as const,
      id: c.id,
      label: c.label,
      onclick: () => bennuUiStore.openJpaGenerate(c.id, activePath),
    }));
  }

  // JSP/JSTL/Struts tag snippets for the editor toolbar's "Insert tag" menu. Each inserts at the
  // caret via the shared `insertAtCursor`; `$0`-free plain text keeps it grammar-agnostic (the
  // placeholders are ordinary text the user overtypes). Kept declarative so adding a tag is one line.
  const JSP_SNIPPETS: { id: string; label: string; text: string }[] = [
    { id: 'c-set',     label: '<c:set>',     text: '<c:set var="name" value="${value}" />' },
    { id: 's-set',     label: '<s:set>',     text: '<s:set var="name" value="%{value}" />' },
    { id: 's-property', label: '<s:property>', text: '<s:property value="%{value}" />' },
    { id: 's-iterator', label: '<s:iterator>', text: '<s:iterator value="%{list}" var="item">\n  \n</s:iterator>' },
    { id: 'c-foreach', label: '<c:forEach>', text: '<c:forEach var="item" items="${list}">\n  \n</c:forEach>' },
    { id: 's-if',      label: '<s:if>',      text: '<s:if test="%{condition}">\n  \n</s:if>' },
    { id: 'c-if',      label: '<c:if>',      text: '<c:if test="${condition}">\n  \n</c:if>' },
    { id: 's-url',     label: '<s:url>',     text: '<s:url var="url" action="actionName" />' },
    { id: 's-text',    label: '<s:text>',    text: '<s:text name="key" />' },
    { id: 's-textfield', label: '<s:textfield>', text: '<s:textfield name="field" label="Label" />' },
  ];
  const snippetMenu = $derived<DropdownItem[]>([
    { kind: 'separator', label: 'Insert tag' },
    ...JSP_SNIPPETS.map((s) => ({
      kind: 'item' as const, id: s.id, label: s.label, onclick: () => insertAtCursor(s.text),
    })),
  ]);
  /** Files that support semantic navigation (go-to declaration / find usages / rename).
   *  On anything else (plain text, markdown, properties, …) those actions are hidden from
   *  the context menu AND their shortcuts no-op — not offered where they can't resolve. */
  const supportsNav = $derived(supportsCodeNav(activePath));

  function onEditorContextMenu(e: MouseEvent) {
    if (!activePath) return;
    e.preventDefault();
    // Move the caret to the click position first — the semantic actions below (go to
    // declaration / find usages / rename / generate) all classify at the caret, and a
    // right-click doesn't move it on its own, so without this they'd target wherever the
    // caret happened to be instead of the symbol under the pointer.
    editorComp?.setCaretAtCoords(e.clientX, e.clientY);
    // Navigation actions (go-to / usages / rename) only where they can resolve; Generate
    // only on Java. On a plain-text/markdown/properties file the menu is just edit + save.
    const navItems: MenuItem[] = supportsNav
      ? [
          { id: 's1', label: '', separator: true },
          { id: 'gotodef', label: 'Go to declaration', icon: Target, shortcut: 'Ctrl+B' },
          { id: 'usages', label: 'Find usages', icon: SearchCode, shortcut: 'Alt+F7' },
          { id: 'rename', label: 'Rename…', icon: PenLine, shortcut: 'Shift+F6' },
        ]
      : [];
    // On a Spring property file: the one thing you reach for that the file cannot tell you —
    // the environment variable that overrides the line you are pointing at.
    const envItems: MenuItem[] = isSpringPropertyFile(activePath)
      ? [
          { id: 's3', label: '', separator: true },
          { id: 'envvar', label: 'Show as environment variable…', icon: Variable },
        ]
      : [];
    const items: MenuItem[] = [
      { id: 'cut', label: 'Cut', icon: Scissors, shortcut: 'Ctrl+X' },
      { id: 'copy', label: 'Copy', icon: Copy, shortcut: 'Ctrl+C' },
      { id: 'paste', label: 'Paste', icon: ClipboardPaste, shortcut: 'Ctrl+V' },
      ...navItems,
      ...envItems,
      { id: 's2', label: '', separator: true },
      ...(isJavaFile
        ? [{ id: 'generate', label: 'Generate…', icon: Wand2, shortcut: 'Alt+Insert' } as MenuItem]
        : []),
      { id: 'save', label: 'Save', icon: Save, shortcut: 'Ctrl+S' },
      { id: 's4', label: '', separator: true },
      // Reachable from the buffer as well as from the tree: the moment you want the
      // previous version of a file is while you are looking at the current one, and going
      // to find its row in the tree first is the trip this saves.
      { id: 'history', label: 'Local History', icon: History, shortcut: 'Alt+Shift+H' },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, onEditorMenuSelect);
  }
  function onEditorMenuSelect(id: string) {
    switch (id) {
      case 'cut': editorComp?.cutSelection(); break;
      case 'copy': editorComp?.copySelection(); break;
      case 'paste': void editorComp?.pasteClipboard(); break;
      case 'gotodef': goToDefinition(); break;
      case 'history':
        if (projectStore.project && activePath) bennuHistoryStore.show(projectStore.project.root, activePath);
        break;
      case 'usages': void findUsages(); break;
      case 'rename': openRename(); break;
      case 'generate':
        if (isJavaFile) bennuUiStore.openGenerate();
        else toastStore.show('Generate works on Java files', 'info');
        break;
      case 'envvar': void showEnvVar(); break;
      case 'save':
        void projectStore.saveActive().then((ok) => { if (ok) toastStore.show('Saved', 'success'); });
        break;
    }
  }

  /**
   * Show the environment override for the property line under the pointer.
   *
   * The caret was already moved to the click by `onEditorContextMenu`, so the line is the one
   * that was right-clicked. Nothing is written — the modal is a thing to copy from.
   */
  async function showEnvVar() {
    const path = activePath;
    if (!path || !editorComp) return;
    const view = await springEnvVar(path, editorComp.getValue(), editorComp.caretByteOffset())
      .catch(() => null);
    if (!view) {
      toastStore.show('That line declares no property key', 'info');
      return;
    }
    envVarView = view;
  }

  // ── Tab-strip context menu (right-click a tab) ────────────────────────────────
  /** Right-click on a tab: the actions all target `path` (the clicked tab), NOT the
   *  active one — closeOthers/closeToRight are relative to what you clicked. */
  function onTabContextMenu(path: string, _item: TabItem, e: MouseEvent) {
    e.preventDefault();
    const idx = openPaths.indexOf(path);
    const hasOthers = openPaths.length > 1;
    const hasRight = idx >= 0 && idx < openPaths.length - 1;
    const items: MenuItem[] = [
      { id: 'close',        label: 'Close',            icon: X },
      { id: 'close-others', label: 'Close Others',     icon: X, disabled: !hasOthers },
      { id: 'close-all',    label: 'Close All',        icon: X },
      { id: 'close-right',  label: 'Close to the Right', icon: ArrowRightToLine, disabled: !hasRight },
      { id: 's1', label: '', separator: true },
      { id: 'copy-path',    label: 'Copy Path',        icon: Copy },
      { id: 'reveal',       label: 'Reveal in Project', icon: LocateFixed },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => onTabMenuSelect(id, path));
  }
  function onTabMenuSelect(id: string, path: string) {
    switch (id) {
      case 'close':        projectStore.closeFile(path); break;
      case 'close-others': projectStore.closeOthers(path); break;
      case 'close-all':    projectStore.closeAll(); break;
      case 'close-right':  projectStore.closeToRight(path); break;
      case 'copy-path':
        void navigator.clipboard?.writeText(path).catch(() => { /* clipboard denied — ignore */ });
        break;
      case 'reveal':
        // Reveal targets the *active* file (the sidebar relay), so make the clicked
        // tab active first, then bump the relay.
        projectStore.setActive(path);
        bennuUiStore.revealActiveInTree();
        break;
    }
  }

  function commitGoto() {
    const m = gotoValue.match(/(\d+)(?:\s*[:,]\s*(\d+))?/);
    if (m) {
      const line = parseInt(m[1], 10);
      const col = m[2] ? parseInt(m[2], 10) : 1;
      if (line > 0) editorComp?.scrollToLineCol(line, col);
    }
    gotoOpen = false;
  }
  function onGotoKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commitGoto(); }
    else if (e.key === 'Escape') { e.preventDefault(); gotoOpen = false; editorComp?.focus(); }
  }
</script>

<div class="ed">
  {#if openPaths.length > 0}
    <div class="ed-tabs">
      <Tabs
        items={tabs}
        value={activePath}
        variant="panel"
        size="sm"
        closable
        overflow
        onSelect={(id) => projectStore.setActive(id)}
        onClose={(id) => projectStore.closeFile(id)}
        onContextMenu={onTabContextMenu}
      />
    </div>

    <!-- The editor toolbar: breadcrumb (left) + file-type-specific actions (right). This is
         THE per-file action bar — new file-type tools slot into `.ed-actions`.
         Skipped for anything that opens as a preview — an image, a Word document — because each
         brings its own bar: every action here is about a document (go to line, reformat, the
         breadcrumb's symbol path) and a second row repeating the file name above a preview is
         just two toolbars. -->
    {#if !isPreviewTab}
    <div class="ed-toolbar">
      <div class="ed-crumbs">
        {#if isValidationFile}<ShieldCheck size={12} class="crumb-icon" />{/if}
        <span class="crumb last">{activePath ? baseName(activePath) : ''}</span>
      </div>
      <div class="ed-actions">
        <!-- What the frameworks offer to write into THIS file. Every button here was
             contributed by an extension that looked at the buffer, so an entity shows the entity
             authoring tools and a repository shows the query builders — and a class that is
             neither shows nothing at all rather than a row of disabled buttons.
             Not folded into the chain below: a Java file can be a Struts action AND an entity,
             and one of them silently winning is how a toolbar stops being trustworthy. -->
        <!-- Icon-only, throughout. A per-file action bar is glanceable or it is furniture: with
             labels this row was six words wide on an entity and the file name lost its space to
             it. Every icon carries its tooltip, and the tooltip is also the accessible name —
             `IconButton` makes that impossible to forget. -->
        {#if fwActions.length > 0}
          {#each fwActions as a (a.id)}
            {@const Icon = actionIcon(a.icon)}
            {#if a.children.length > 0}
              <!-- A dropdown, not a dialog tab strip: which shape you are writing is a decision
                   you have already made when you reach for this. -->
              <Dropdown items={actionMenu(a)} position="fixed" direction="down" width="230px">
                {#snippet trigger({ open, toggle })}
                  <IconButton
                    tooltip={a.detail || a.label}
                    size={26}
                    active={open}
                    ariaHasPopup
                    ariaExpanded={open}
                    onclick={toggle}
                  >
                    <span class="ed-menu-icon"><Icon size={13} /><ChevronDown size={9} /></span>
                  </IconButton>
                {/snippet}
              </Dropdown>
            {:else}
              <IconButton
                tooltip={a.detail || a.label}
                size={26}
                onclick={() => bennuUiStore.openJpaGenerate(a.id, activePath)}
              >
                <Icon size={13} />
              </IconButton>
            {/if}
          {/each}
          <span class="ed-tsep"></span>
        {/if}
        {#if pluginToolbar.length}
          <!-- What the loaded plugins put on THIS file's toolbar. First in the row, because a
               plugin's button is the least expected thing here and burying it behind the
               file-type tools is how it stays undiscovered — which is the whole reason it is
               not only a palette entry. -->
          {#each pluginToolbar as b (b.id)}
            <IconButton
              tooltip={b.tooltip}
              size={26}
              onclick={() => {
                firePluginAction(b.pluginName, b.action, JSON.stringify({ path: activePath }))
                  .catch((e) => console.error(`plugin '${b.pluginName}': action '${b.action}' failed`, e));
              }}
            >
              <span class="ed-plugin-icon" style:color={b.color ?? undefined}>
                <PluginIcon name={b.icon} size={13} />
              </span>
            </IconButton>
          {/each}
          <span class="ed-tsep"></span>
        {/if}
        {#if isI18nBundle(activePath)}
          <!-- The affordance for a panel with no rail button. A translation bundle is the one file
               where the i18n panel has something to say, so the button exists exactly there — which
               makes the toolbar's contents the answer to "what kind of file is this", the same rule
               the contributed actions above follow. -->
          <IconButton
            tooltip="i18n — preview this translation, and write markup into it"
            shortcut="Alt+Shift+I"
            size={26}
            active={bennuUiStore.rightPanel === 'i18n'}
            onclick={() => bennuUiStore.toggleRight('i18n')}
          >
            <Languages size={13} />
          </IconButton>
          <span class="ed-tsep"></span>
        {/if}
        {#if isValidationFile}
          <!-- Struts validation-file tools. -->
          <IconButton tooltip="Validation reference" size={26} onclick={() => bennuUiStore.toggleDocs()}>
            <BookOpen size={13} />
          </IconButton>
          <IconButton
            tooltip="Add a validator chain to a field"
            size={26}
            variant="accent"
            onclick={() => bennuUiStore.openValidationCreator()}
          >
            <Plus size={13} />
          </IconButton>
          <span class="ed-tsep"></span>
        {:else if isJavaFile && hasStruts}
          <!-- On a Java action class: create (or open) its `<Class>-validation.xml`. Offered only
               where Struts is actually in use — see `hasStruts`. -->
          <IconButton
            tooltip="Create or open the Struts validation file for this action class"
            size={26}
            onclick={createValidationFile}
          >
            <ShieldCheck size={13} />
          </IconButton>
          <span class="ed-tsep"></span>
        {:else if isJspFile}
          <!-- On a view JSP mapped from one or more actions: pick which action its OGNL is checked /
               navigated against (drives the "unknown property" lint + go-to for %{…} references).
               The one place a label survives: WHICH action is bound is state, not an action, and
               an icon cannot say `LoginAction`. -->
          {#if jspActionLabel}
            <Dropdown items={jspActionMenu} position="fixed" direction="down" width="240px">
              {#snippet trigger({ open, toggle })}
                <button class="ed-tbtn" class:active={open} onclick={toggle} use:tooltip={'Bind this view to a Struts action (for OGNL property checks / go-to)'} aria-haspopup="menu" aria-expanded={open}>
                  <Target size={12} /> {jspActionLabel} <ChevronDown size={11} />
                </button>
              {/snippet}
            </Dropdown>
            <span class="ed-tsep"></span>
          {/if}
          <!-- On a JSP: an "Insert tag" menu that drops JSTL/Struts snippets at the caret. -->
          <Dropdown items={snippetMenu} position="fixed" direction="down" width="200px">
            {#snippet trigger({ open, toggle })}
              <IconButton
                tooltip="Insert a JSTL / Struts tag at the caret"
                size={26}
                active={open}
                ariaHasPopup
                ariaExpanded={open}
                onclick={toggle}
              >
                <span class="ed-menu-icon"><Braces size={13} /><ChevronDown size={9} /></span>
              </IconButton>
            {/snippet}
          </Dropdown>
          <span class="ed-tsep"></span>
        {/if}
        {#if isHtmlTab}
          <!-- The page, rendered. Not a mode the editor is in — the source is one press away
               again — and gated the first time, because a page can run its own code. -->
          <IconButton
            tooltip={htmlPreviewing ? 'Close the preview' : 'Preview this page beside the source'}
            size={26}
            active={htmlPreviewing}
            onclick={() => {
              if (!activePath) return;
              if (htmlPreviewing) { htmlPreviewOpen.delete(activePath); htmlFullscreen = false; }
              else htmlPreviewOpen.add(activePath);
            }}
          >
            {#if htmlPreviewing}<FileCode2 size={13} />{:else}<Eye size={13} />{/if}
          </IconButton>
        {/if}
        {#if markdownLive}
          <!-- Only in the live preview: in the source view a table is markdown you type, and a
               picker that inserted pipes into a code editor would be answering a question
               nobody asked there. -->
          <BennuTableInsert onPick={insertMarkdownTable} />
        {/if}
        {#if isMarkdownTab}
          <!-- Rendered or raw. The rendered side is still an editor — this is not a preview
               pane — so the toggle is about what the markup looks like while you work on it,
               not about whether the file can be changed. -->
          <IconButton
            tooltip={markdownLive ? 'Edit the markdown source' : 'Live preview'}
            size={26}
            active={markdownLive}
            onclick={() => bennuSettingsStore.setMarkdownLivePreview(!markdownLive)}
          >
            {#if markdownLive}<FileCode2 size={13} />{:else}<Eye size={13} />{/if}
          </IconButton>
        {/if}
        {#if !markdownLive}
          <!-- Go-to-line belongs to the code editor, and in the live preview there is not one
               mounted to answer it. A button that quietly does nothing is worse than no button. -->
          <IconButton tooltip="Go to line" shortcut="Ctrl+G" size={26} onclick={openGoto}>
            <Hash size={13} />
          </IconButton>
        {/if}
      </div>
    </div>
    {/if}
  {/if}

  <!-- Decompiled dependency with no attached sources: offer a one-click "Download sources" fetch. -->
  {#if decompiledCtx?.canDownload}
    <div class="ed-sources-banner">
      <span class="ed-sources-msg"><FileDown size={13} /> Decompiled from bytecode — no sources attached.</span>
      <button
        class="ed-tbtn primary"
        onclick={downloadTabSources}
        disabled={decompiledStore.isDownloading(activePath)}
        use:tooltip={'Fetch this dependency’s -sources.jar via Maven'}
      >
        <DownloadCloud size={13} />
        {decompiledStore.isDownloading(activePath) ? 'Downloading…' : 'Download sources'}
      </button>
    </div>
  {/if}

  {#if activePath && isImageTab}
    <!-- An image has no buffer to edit, so it gets its own view rather than an editor with an
         empty document in it. The tab strip, the tree selection and Ctrl+W all behave the same. -->
    <BennuImageView path={activePath} />
  {:else if activePath && isDocxTab}
    <!-- Same bargain as an image: no buffer, its own viewer. A `.docx` is a ZIP of XML, and
         the only useful thing to do with one in a project tree is read it. -->
    <BennuDocxView path={activePath} />
  {:else if activePath && isFontTab}
    <!-- Same again, and every question about a font is visual: what it looks like, whether it
         has the accents this project needs, how it holds up small. -->
    <BennuFontView path={activePath} />
  {:else if activePath && markdownLive}
    <!-- Markdown, rendered as you type: the same live-preview editor Garrulus's notes and the
         markdown modal use, so a README reads the same everywhere in the app. It edits the real
         buffer — every change goes through `onInput` like the code editor's, so dirty state,
         Ctrl+S and autosave are unchanged. No editor context menu: every entry on it (go to
         declaration, find usages, the refactorings) is about code. -->
    <div class="ed-editor-wrap">
      <MarkdownEditor
        bind:this={mdComp}
        docKey={mdDocKey}
        text={projectStore.sourceOf(activePath)}
        docPath={activePath}
        readOnly={isDecompiledView}
        autofocus={false}
        onChange={onMarkdownInput}
        onCaret={onMarkdownCaret}
        onOpenLink={openMarkdownLink}
        fileIndex={projectFileIndex}
      />
    </div>
  {:else if activePath}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ed-editor-wrap" oncontextmenu={onEditorContextMenu}>
      {#key activePath}
        <CodeEditor
          bind:this={editorComp}
          value={projectStore.sourceOf(activePath)}
          language={editorLanguage}
          readOnly={isDecompiledView}
          diagnostics={allDiags}
          marks={springMarks}
          lineHighlights={pausedLine}
          gutterMarks={allGutterMarks}
          onGutterClick={onGutterClick}
          flagMarks={canBreak ? breakpointMarks : undefined}
          canFlag={canFlagLine}
          onFlagClick={onBreakpointClick}
          onFlagContext={onBreakpointContext}
          onFlagsMoved={onBreakpointsMoved}
          rulerColumn={bennuSettingsStore.rightMargin}
          minimap={false}
          scrollbarOverview={bennuSettingsStore.minimap}
          indentGuides={bennuSettingsStore.indentGuides}
          stickyScroll={bennuSettingsStore.stickyScroll}
          emmet={emmetEnabled}
          fontSize={bennuSettingsStore.fontSize}
          wrap={bennuSettingsStore.wordWrap}
          showWhitespace={bennuSettingsStore.showWhitespace}
          lineNumbers={bennuSettingsStore.showLineNumbers}
          highlightActiveLine={bennuSettingsStore.highlightCurrentLine}
          folding={bennuSettingsStore.foldingEnabled}
          foldBlockComments={bennuSettingsStore.foldBlockComments}
          completion={{
            autoPopup: bennuSettingsStore.autoPopup,
            delayMs: bennuSettingsStore.popupDelayMs,
            caseSensitive: bennuSettingsStore.caseSensitive,
          }}
          tabSize={bennuSettingsStore.tabSize}
          indentUnit={bennuSettingsStore.indentStyle === 'tabs' ? '\t' : ' '.repeat(bennuSettingsStore.tabSize)}
          initialState={viewStates.get(activePath)}
          oninput={onInput}
          oncaret={onCaret}
          onViewState={(s) => { if (activePath) viewStates.set(activePath, s); }}
          onGoto={onEditorGoto}
          onLensPress={(key) => void onLensPress(key)}
        />
      {/key}
      {#if htmlPreviewing && !htmlFullscreen}
        <!-- Beside the source, not instead of it: a page is edited and looked at in the same
             breath, and a preview that replaced the buffer would make every fix a round trip
             through a toggle. Resizable, and ⤢ takes it to nearly the whole window when the
             layout is what matters rather than the markup. -->
        <ResizablePanel direction="horizontal" initialSize={520} minSize={220} maxSize={1400} reverse>
          <BennuHtmlPreview
            path={activePath}
            html={projectStore.sourceOf(activePath)}
            scripts={htmlScriptsOn}
            onToggleScripts={toggleHtmlScripts}
            onClose={() => activePath && htmlPreviewOpen.delete(activePath)}
            onToggleFullscreen={() => (htmlFullscreen = true)}
          />
        </ResizablePanel>
      {/if}
      <!-- IntelliJ-style file health badge, pinned top-right over the editor. -->
      {#if isJavaFile}
        <div class="ed-health" class:clean={diagCounts.errors === 0 && diagCounts.warnings === 0}
             use:tooltip={diagCounts.errors === 0 && diagCounts.warnings === 0
               ? 'No problems in this file'
               : `${diagCounts.errors} error${diagCounts.errors === 1 ? '' : 's'}, ${diagCounts.warnings} warning${diagCounts.warnings === 1 ? '' : 's'}`}>
          {#if diagCounts.errors > 0}
            <span class="ed-health-item err"><CircleAlert size={13} /> {diagCounts.errors}</span>
          {/if}
          {#if diagCounts.warnings > 0}
            <span class="ed-health-item warn"><TriangleAlert size={13} /> {diagCounts.warnings}</span>
          {/if}
          {#if diagCounts.errors === 0 && diagCounts.warnings === 0}
            <span class="ed-health-item ok"><Check size={13} /></span>
          {/if}
        </div>
      {/if}
    </div>
  {:else}
    <div class="ed-empty">
      <EmptyState message="No file open. Pick a file from the project tree." />
    </div>
  {/if}

  {#if activePath && !isPreviewTab}
    <div class="ed-footer">
      <!-- What the last explicit completion request came back with, when it came back with
           nothing. Left-aligned and transient: it is an answer to a key that was just pressed,
           not a status the footer keeps. -->
      {#if completionNoteStore.note}
        <span class="ed-note">{completionNoteStore.note}</span>
      {/if}
      <span class="ed-pos"><MapPin size={11} /> Ln {caretLine}, Col {caretCol}</span>
      <span class="ed-foot-sep">·</span>
      <span class="ed-enc" use:tooltip={'File encoding'}>{projectStore.activeEncoding}</span>
    </div>
  {/if}

  {#if gotoOpen}
    <div class="ed-goto" role="dialog" aria-label="Go to line" tabindex="-1">
      <Hash size={13} />
      <input bind:this={gotoInputEl} bind:value={gotoValue} onkeydown={onGotoKey} onblur={() => (gotoOpen = false)} placeholder="Line or line:col…" inputmode="numeric" />
    </div>
  {/if}

  {#if renameOpen && renameAnchor}
    <div
      class="ed-rename"
      class:invalid={!renameValid && renameName.length > 0}
      role="dialog"
      aria-label="Rename symbol"
      tabindex="-1"
      bind:this={renameBoxEl}
      style="left:{renameAnchor.x}px; top:{renameAnchor.y}px;"
    >
      <PenLine size={13} />
      <input
        bind:this={renameInputEl}
        bind:value={renameName}
        oninput={onRenameInput}
        onkeydown={onRenameKey}
        onblur={onRenameBlur}
        spellcheck="false"
        autocomplete="off"
        aria-label="New name"
      />
      <button
        class="ed-rename-preview"
        type="button"
        onclick={openRenamePreview}
        use:tooltip={{ content: 'Preview all changes', shortcut: 'Shift+Enter' }}
        aria-label="Preview rename"
      >
        <Eye size={13} />
      </button>
    </div>
  {/if}
</div>

{#if envVarView}
  <BennuEnvVarModal view={envVarView} onClose={() => { envVarView = null; editorComp?.focus(); }} />
{/if}

<!-- Mounted here rather than by the window, like the modal above and for the same reason: what it
     shows is about the CARET, and the caret is this component's. -->
{#if macroView}
  <BennuMacroExpandModal
    name={macroView.name}
    expansion={macroView.expansion}
    onReexpand={() => expandMacroAtCaret(true)}
    onClose={() => { macroView = null; editorComp?.focus(); }}
  />
{/if}

<!-- The page, filling the window. The same component as the inline preview — the frame is
     re-created by the move, which is the honest behaviour: a preview that survived being resized
     with its timers running would be a second document nobody asked to keep. -->
{#if activePath && htmlPreviewing && htmlFullscreen}
  <Modal
    onClose={() => (htmlFullscreen = false)}
    width="min(1720px, 97vw)"
    height="min(1000px, 95vh)"
    padBody={false}
    ariaLabel={`Preview of ${activePath}`}
  >
    <!-- ⚠️ The wrapper is load-bearing. `.modal-body` is `flex: 1` inside the modal but is not
         itself a flex container, so a child asking for `flex: 1` gets no flex context: the
         preview collapsed to its content and the iframe fell back to its intrinsic ~150px,
         which is the "the fullscreen preview is not full height" this fixes. -->
    <div class="ed-fs-preview">
      <BennuHtmlPreview
        path={activePath}
        html={projectStore.sourceOf(activePath)}
        scripts={htmlScriptsOn}
        onToggleScripts={toggleHtmlScripts}
        onClose={() => { htmlFullscreen = false; if (activePath) htmlPreviewOpen.delete(activePath); }}
        onToggleFullscreen={() => (htmlFullscreen = false)}
        fullscreen
      />
    </div>
  </Modal>
{/if}

{#if htmlAsk}
  <BennuHtmlScriptsModal
    path={htmlAsk}
    onAllow={allowHtmlScripts}
    onCancel={() => (htmlAsk = null)}
  />
{/if}

<style>
  .ed {
    display: flex; flex-direction: column;
    flex: 1; min-width: 0; min-height: 0;
    background: var(--bg-base);
    position: relative;
  }

  .ed-tabs {
    display: flex; align-items: stretch;
    height: 32px; min-height: 32px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
  }
  .ed-tabs :global(.tabs) { flex: 1; min-width: 0; }

  /* A contributed icon inherits the IconButton's colour unless the plugin named one. */
  .ed-plugin-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: inherit;
  }

  .ed-toolbar {
    display: flex; align-items: center;
    height: 28px; min-height: 28px;
    padding: 0 8px 0 10px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
  }
  .ed-crumbs { flex: 1; min-width: 0; display: flex; align-items: center; gap: 2px; overflow: hidden; }
  .crumb { font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; }
  .crumb.last { color: var(--text-secondary); font-weight: 500; }

  .ed-actions { display: flex; align-items: center; gap: 1px; flex-shrink: 0; }
  /* A dropdown trigger in the toolbar: the icon with a small chevron tucked under its right
     edge, so an icon that opens a menu is distinguishable from one that acts immediately
     without costing the row a second glyph's width. */
  .ed-menu-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .ed-menu-icon :global(svg:last-child) {
    position: absolute;
    right: -5px;
    bottom: -4px;
    opacity: 0.75;
  }

  /* The one labelled button left in the toolbar: the JSP action binding, where WHICH action is
     bound is state rather than a verb and an icon cannot say `LoginAction`. */
  .crumb-icon { color: var(--accent); flex-shrink: 0; margin-right: 2px; }
  .ed-tbtn {
    display: flex; align-items: center; gap: 5px;
    height: 20px; padding: 0 8px;
    background: transparent; border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary); font-size: var(--font-size-xs); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .ed-tbtn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ed-tbtn.primary { background: var(--accent); border-color: var(--accent); color: var(--text-on-accent); }
  .ed-tbtn.primary:hover { filter: brightness(1.08); }
  .ed-tsep { width: 1px; height: 16px; background: var(--border-subtle); margin: 0 3px; }

  /* "Download sources" banner over a decompiled dependency tab (between toolbar and editor). */
  .ed-sources-banner {
    display: flex; align-items: center; gap: 10px;
    padding: 5px 10px;
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-base));
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .ed-sources-msg {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .ed-sources-banner .ed-tbtn:disabled { opacity: 0.6; cursor: default; }
  .ed-sources-banner .ed-tbtn.primary:disabled:hover { filter: none; }

  .ed-editor-wrap { flex: 1; display: flex; min-width: 0; min-height: 0; position: relative; }
  /* The modal body is a block box (see the note at the call site): this is the flex context the
     preview needs to fill it. */
  .ed-fs-preview { display: flex; width: 100%; height: 100%; min-height: 0; }
  .ed-editor-wrap > :global(.code-editor) { flex: 1; min-width: 0; min-height: 0; }

  /* IntelliJ-style file-health badge, pinned top-right over the editor (offset past the
     right-edge overview strip). */
  .ed-health {
    position: absolute;
    top: 4px;
    right: 20px;
    z-index: 7;
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
    background: color-mix(in srgb, var(--bg-elevated) 82%, transparent);
    font-size: var(--font-size-xs);
    font-variant-numeric: tabular-nums;
    pointer-events: auto;
  }
  .ed-health-item { display: inline-flex; align-items: center; gap: 3px; }
  .ed-health-item.err { color: var(--error); }
  .ed-health-item.warn { color: var(--warning); }
  .ed-health-item.ok { color: var(--success); }

  .ed-empty { flex: 1; display: flex; align-items: center; justify-content: center; min-height: 0; }

  /* The transient completion answer. `margin-right: auto` pins it to the left while the
     position + encoding stay right, so it never pushes them around as it comes and goes. */
  .ed-note {
    margin-right: auto;
    color: var(--text-secondary);
    font-style: italic;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  .ed-footer {
    display: flex; align-items: center; justify-content: flex-end; gap: 8px;
    height: 22px; min-height: 22px; flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-base);
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-muted);
    user-select: none;
  }
  .ed-pos { display: flex; align-items: center; gap: 4px; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .ed-pos :global(svg) { color: var(--text-disabled); }
  .ed-foot-sep { color: var(--text-disabled); }
  .ed-enc {
    white-space: nowrap; cursor: default;
    font-variant-numeric: tabular-nums; letter-spacing: 0.2px;
  }
  .ed-enc:hover { color: var(--text-secondary); }

  .ed-goto {
    position: absolute; top: 64px; right: 14px;
    display: flex; align-items: center; gap: 6px;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup);
    padding: 6px 8px; color: var(--text-muted); z-index: 20;
  }
  .ed-goto input {
    background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm); width: 140px;
  }
  .ed-goto input::placeholder { color: var(--text-disabled); }

  /* Inline rename — a small field anchored at the caret (fixed to the viewport). */
  .ed-rename {
    position: fixed;
    display: flex; align-items: center; gap: 6px;
    background: var(--bg-elevated); border: 1px solid var(--border-focus, var(--accent));
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup);
    padding: 4px 5px 4px 8px; color: var(--text-muted); z-index: 30;
    transform: translateY(4px);
  }
  .ed-rename.invalid { border-color: var(--error); }
  .ed-rename input {
    background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-code);
    font-size: var(--font-size-sm); width: 160px;
  }
  .ed-rename.invalid input { color: var(--error); }
  .ed-rename-preview {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 20px; flex-shrink: 0;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ed-rename-preview:hover { background: var(--bg-hover); color: var(--text-primary); }

  /*
   * Framework syntax inside string literals and XML attributes.
   *
   * `:global` is unavoidable and is the documented exception (CLAUDE.md): these classes
   * are attached by CodeMirror to spans it creates inside its own DOM, so Svelte's
   * scoping hash never reaches them. Same arrangement as Picus's SQL abbreviations.
   *
   * Restraint on purpose: the base `.cm-fw` is a faint tint that says "the framework
   * reads this", and only the parts you would actually click — the property key, the bean
   * reference, the path variable — get a real colour. Colouring every token of a SpEL
   * expression would turn one string into a rainbow and make the page harder to read
   * than it was with no highlighting at all.
   */
  /*
   * Two things make these rules look over-specified, and both are load-bearing.
   *
   * `.cm-content` in front: the syntax highlighter colours the enclosing string literal,
   * and a plain single-class rule ties with it on specificity — so which colour won came
   * down to stylesheet order, and the string's won. Inherited properties came through
   * (the default value did render italic), which is exactly the confusing half-working
   * state this avoids.
   *
   * The `span` descendant: where a mark covers only part of a token, CodeMirror nests the
   * two, and a colour set on the outer element loses to one set on the inner regardless
   * of specificity. Colouring the descendants is the only thing that reaches it.
   */
  /*
   * Colours come from the `--syntax-*` palette, not from the UI one (`--info`,
   * `--accent`, …). This is text in a code buffer sitting beside Java tokens, so it
   * belongs to the same family as those tokens; borrowing the app's info blue put a
   * colour that means "badge, chip, status" in the middle of source, and one that is
   * already carrying too many jobs elsewhere.
   *
   * Three colours, one idea each — a framework highlight should read as a small extension
   * of the language's own scheme, not as a second scheme competing with it:
   *   • violet — a NAME the framework resolves for you (a property key, a path variable);
   *   • gold   — something CALLABLE (a bean reference, a SpEL type reference);
   *   • orange — a keyword, the same orange keywords already have everywhere.
   * The default value stays muted italic: it is the fallback, not the subject.
   */
  /* The tint marks the whole expression, so it goes on the OUTER span only — putting it
     on every mark would stack it on the key and the default, which sit inside. */
  :global(.cm-content .cm-fw-spring-placeholder),
  :global(.cm-content .cm-fw-spring-spel) {
    background: color-mix(in srgb, var(--syntax-field, #9876aa) 12%, transparent);
    border-radius: 2px;
  }
  :global(.cm-content .cm-fw-spring-placeholder-key),
  :global(.cm-content .cm-fw-spring-placeholder-key span),
  :global(.cm-content .cm-fw-spring-path-var),
  :global(.cm-content .cm-fw-spring-path-var span) { color: var(--syntax-field, #9876aa); font-weight: 600; }
  :global(.cm-content .cm-fw-spring-spel-variable),
  :global(.cm-content .cm-fw-spring-spel-variable span) {
    color: var(--syntax-field, #9876aa); font-weight: 600; font-style: italic;
  }
  :global(.cm-content .cm-fw-spring-spel-bean),
  :global(.cm-content .cm-fw-spring-spel-bean span),
  :global(.cm-content .cm-fw-spring-spel-type),
  :global(.cm-content .cm-fw-spring-spel-type span) { color: var(--syntax-function, #ffc66d); font-weight: 600; }
  :global(.cm-content .cm-fw-spring-spel-keyword),
  :global(.cm-content .cm-fw-spring-spel-keyword span) { color: var(--syntax-keyword, #cc7832); font-weight: 600; }
  :global(.cm-content .cm-fw-spring-placeholder-default),
  :global(.cm-content .cm-fw-spring-placeholder-default span) { color: var(--text-muted); font-style: italic; }

  /*
   * The fulcrum i18n markup, inside a TOML string.
   *
   * Same idea as the Spring placeholders above and the same reason: the host language sees one opaque
   * string, so the structure inside it is invisible until something paints it. What is coloured is the
   * **names** — the part you have to get right — plus a tint on each construct so nesting reads as
   * nesting. `$red.bold{@potion{una pozione}}` is two boxes deep and looks it.
   *
   * A name the project does not declare is coloured as a warning rather than as a name. There is a
   * diagnostic saying so too, but the diagnostic arrives with the next scan and this arrives with the
   * keystroke — and the failure it prevents is silent: a style that does not exist renders as the
   * default, so the text appears, unstyled, and nothing complains.
   */
  :global(.cm-content .cm-fw-fulcrum-i18n-span-style),
  :global(.cm-content .cm-fw-fulcrum-i18n-span-glossary),
  :global(.cm-content .cm-fw-fulcrum-i18n-span-control) {
    background: color-mix(in srgb, var(--syntax-string, #6a8759) 10%, transparent);
    border-radius: 2px;
  }
  :global(.cm-content .cm-fw-fulcrum-i18n-placeholder),
  :global(.cm-content .cm-fw-fulcrum-i18n-placeholder span) {
    color: var(--syntax-field, #9876aa); font-weight: 600;
  }
  :global(.cm-content .cm-fw-fulcrum-i18n-style),
  :global(.cm-content .cm-fw-fulcrum-i18n-style span) {
    color: var(--syntax-function, #ffc66d); font-weight: 600;
  }
  :global(.cm-content .cm-fw-fulcrum-i18n-glossary),
  :global(.cm-content .cm-fw-fulcrum-i18n-glossary span) {
    color: var(--syntax-type, #4ec9b0); font-weight: 600;
  }
  :global(.cm-content .cm-fw-fulcrum-i18n-control),
  :global(.cm-content .cm-fw-fulcrum-i18n-control span) {
    color: var(--syntax-keyword, #cc7832); font-weight: 600;
  }
  :global(.cm-content .cm-fw-fulcrum-i18n-namespace),
  :global(.cm-content .cm-fw-fulcrum-i18n-namespace span) {
    color: var(--text-muted); font-weight: 600;
  }
  /* Underlined rather than recoloured: the point is that this name resolves to nothing, and giving it
     a colour of its own would make it look like a kind of name rather than a missing one. */
  :global(.cm-content .cm-fw-fulcrum-i18n-style-unknown),
  :global(.cm-content .cm-fw-fulcrum-i18n-style-unknown span),
  :global(.cm-content .cm-fw-fulcrum-i18n-glossary-unknown),
  :global(.cm-content .cm-fw-fulcrum-i18n-glossary-unknown span) {
    color: var(--warning);
    text-decoration: underline wavy var(--warning);
    text-underline-offset: 2px;
  }

  /*
   * A `@Query` is a second language living inside a Java string, and that is exactly why a
   * mistake in one survives review: the provider parses it, the compiler does not. The tint
   * marks the whole query so it reads as *not Java*; the keywords take the language's own
   * keyword colour; the `:param` placeholders take the same violet a `${key}` takes above,
   * because they are the same idea — a name something else resolves for you.
   *
   * Native SQL gets a warmer tint than JPQL, on purpose. The two are genuinely different
   * risks: JPQL is checked against the entity model, native SQL is sent to the database as
   * written and nothing here can vouch for it.
   */
  :global(.cm-content .cm-fw-jpa-query) {
    background: color-mix(in srgb, var(--syntax-function, #ffc66d) 9%, transparent);
    border-radius: 2px;
  }
  :global(.cm-content .cm-fw-jpa-query-native) {
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border-radius: 2px;
  }
  :global(.cm-content .cm-fw-jpa-query-keyword),
  :global(.cm-content .cm-fw-jpa-query-keyword span) {
    color: var(--syntax-keyword, #cc7832); font-weight: 600;
  }
  :global(.cm-content .cm-fw-jpa-query-param),
  :global(.cm-content .cm-fw-jpa-query-param span) {
    color: var(--syntax-field, #9876aa); font-weight: 600;
  }
  :global(.cm-content .cm-fw-jpa-query-string),
  :global(.cm-content .cm-fw-jpa-query-string span) { color: var(--syntax-string, #6a8759); }
  :global(.cm-content .cm-fw-jpa-query-number),
  :global(.cm-content .cm-fw-jpa-query-number span) { color: var(--syntax-number, #6897bb); }

  /* The run arrow. Green and clearly larger than the framework glyphs, because it is the one
     gutter mark that DOES something rather than pointing somewhere — and because a green ▶ is
     what a hand trained on IntelliJ looks for.
     ⚠️ The selector carries `.code-editor .cm-host-gutter-icon` on purpose. The widget's own base
     rule for a gutter icon is `.code-editor :global(.cm-host-gutter-icon)` — two classes — and a
     bare `:global(.cm-run-gutter)` is one, so it LOST: the arrow rendered at the base rule's 10px
     muted grey, which is exactly what it looked like. A host-supplied class has to out-specify
     the default it is meant to replace. */
  :global(.code-editor .cm-host-gutter-icon.cm-run-gutter) {
    color: var(--success);
    font-size: 15px;
    line-height: 1;
    width: 17px;
    transform: translateY(-0.5px);
  }
  :global(.code-editor .cm-host-gutter-icon.cm-run-gutter:hover) {
    color: color-mix(in srgb, var(--success) 70%, #fff);
    transform: translateY(-0.5px) scale(1.12);
  }

  /* Gutter icons: colour by what the mark means, so a glance separates "a bean is
     declared here" from "something is injected here" without reading the tooltip. */
  :global(.cm-fw-gutter-bean) { color: var(--success); }
  :global(.cm-fw-gutter-inject) { color: var(--info); }
  :global(.cm-fw-gutter-endpoint) { color: var(--warning); }
  :global(.cm-fw-gutter-entity) { color: var(--syntax-field, #9876aa); }
  :global(.cm-fw-gutter-repository) { color: var(--syntax-function, #ffc66d); }
  /* The ECS marks. Data is one family (component / bundle share a hue), a resource is a singleton
     and an event is a signal — so the colour carries the same distinction the glyph does. */
  :global(.cm-fw-gutter-component) { color: var(--syntax-type, #4ec9b0); }
  :global(.cm-fw-gutter-bundle) { color: var(--syntax-type, #4ec9b0); }
  :global(.cm-fw-gutter-resource) { color: var(--info); }
  :global(.cm-fw-gutter-message) { color: var(--warning); }
  :global(.cm-fw-gutter-event) { color: var(--warning); }
  :global(.cm-fw-gutter-states) { color: var(--syntax-keyword, #cc7832); }
  /* The usage count beside a property key: a number, so it reads as one. */
  :global(.cm-fw-gutter-usage) {
    color: var(--text-muted); font-family: var(--font-code); font-weight: 600;
  }
  :global(.cm-fw-gutter-usage:hover) { color: var(--accent); }

  /* Breakpoints. Solid = the VM accepted it. Hollow = it is waiting for the class to load,
     which resolves itself. Muted = disabled. The three read differently at a glance because
     the difference between them is what tells you whether to keep waiting. */
  :global(.cm-flag-icon.cm-bp-pending) {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--error);
  }
  :global(.cm-flag-icon.cm-bp-off) {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--text-muted);
  }
  /* A condition (or a pass count) on it. A ring around the dot rather than another colour:
     colour is already saying whether the VM accepted it, and those two are different questions —
     a conditional breakpoint can be verified, pending or disabled like any other. A breakpoint
     that does not stop is the most expensive thing to misread in a debugger, and this is what
     answers it before you think to right-click. */
  :global(.cm-flag-icon.cm-bp-cond) {
    outline: 1.5px solid var(--warning);
    outline-offset: 1.5px;
  }

  /* Where the program is stopped, on the frame you are looking at. A full-width band with a
     bar down its left edge — the band says WHICH row and the bar says it is the execution
     point rather than a selection, which is the distinction IntelliJ draws the same way. */
  :global(.cm-paused-line) {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
</style>
