<script lang="ts">
  /**
   * BennuEditor — the tabbed Java editor area.
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
    Braces, ArrowLeftRight, Package, FolderInput, CircleAlert, TriangleAlert, Check,
    DownloadCloud, FileDown, Variable, Database, Clock, Columns3, ListPlus, SquarePen,
  } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { languageForPath } from './languages';
  import {
    isJavaFile as isJavaFileOf, isJspFile as isJspFileOf,
    isLspFile as isLspFileOf, hasPushedDiagnostics,
    supportsCodeNav, supportsDiagnostics,
  } from './file-kind';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import {
    lspSemanticTokens, lspCodeActions, lspCodeLenses, lspExecuteCommand, lspExpandMacro,
    lspFormat, lspFolding, lspHighlights, lspLensLocations, lspSelectionRanges,
    type LspAction, type LspLens, type LspMacroExpansion,
  } from '$lib/ipc/bennu/lsp';
  import type { SourceEdit } from '$lib/types/bennu';
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
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { diagnostics as ipcDiagnostics } from '$lib/ipc/bennu';
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
  import BennuEnvVarModal from './BennuEnvVarModal.svelte';
  import BennuMacroExpandModal from './BennuMacroExpandModal.svelte';
  import { makeByteToU16, makeU16ToByte } from '$lib/components/shared/ui/code-editor';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { decompiledStore } from '$lib/stores/bennu/decompiled.svelte';
  // The gutter's breakpoints and the paused line — both are the debugger's state seen from
  // the editor, and both are read-only here: the store owns them and persists them.
  import { bennuDebugStore, canonFile } from '$lib/stores/bennu/debug.svelte';
  // Which lines compile to bytecode — the gutter offers a breakpoint only on those.
  import { breakpointableLines } from './breakpoint-lines';
  import { spellcheck as ipcSpellcheck, type SpellHit } from '$lib/ipc/bennu/spell';
  import { mojibakeCheck as ipcMojibakeCheck } from '$lib/ipc/bennu/mojibake';
  import { intentionsAt as ipcIntentionsAt } from '$lib/ipc/bennu/intentions';
  import { validationTarget as ipcValidationTarget } from '$lib/ipc/bennu/validation';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import type {
    EditorDiagnostic, EditorViewSnapshot, SemanticToken,
  } from '$lib/components/shared/ui/code-editor';
  import type { EditorView } from '@codemirror/view';
  import { bennuIntentionsStore } from '$lib/stores/bennu/intentions.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
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
  }: {
    onGenerate?: (mode: GenerateMode) => void;
  } = $props();

  type EditorController = {
    focus: () => void;
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

  // Per-tab cursor + scroll, so switching away and back restores where you left off.
  // The editor remounts on tab switch ({#key activePath}); it emits `onViewState` while
  // a tab is active and reads `initialState` for the returning tab from this map.
  const viewStates = new Map<string, EditorViewSnapshot>();
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

  // ── Goto relay: Structure / Outline / Problems request a jump; scroll there. ──
  $effect(() => {
    const t = bennuUiStore.gotoTarget;
    if (!t) return;
    // Read `nonce` so a repeat jump to the same line re-fires.
    void t.nonce;
    editorComp?.scrollToLineCol(t.line, 1);
  });

  // ── Goto-by-byte-offset relay: the Forms tool window requests a jump to a `<form>`
  //    tag / field-name byte span; move the caret there and reveal it. ──
  $effect(() => {
    const t = bennuUiStore.gotoOffsetTarget;
    if (!t) return;
    void t.nonce; // repeat jump to the same offset re-fires
    editorComp?.scrollToByteOffset(t.offset);
  });

  // ── Edits → store ────────────────────────────────────────────────────────────
  function onInput(text: string) {
    if (activePath) projectStore.setSource(activePath, text);
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
    editorComp?.selectByteRange(req.start, req.end);
  });

  // ── Diagnostics (byte spans) from the backend, re-fetched per active file ─────
  // For a JSP the backend extracts + checks the `action="…"` refs (unknown → warning
  // squiggle). Re-fetched when the index rebuilds too (`buildRevision`), so squiggles
  // appear once the config graph finishes building after a fresh open.
  let diags = $state<EditorDiagnostic[]>([]);
  $effect(() => {
    const path = activePath;
    void bennuIndexStore.buildRevision; // re-run when the index (config graph) rebuilds
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
          diags = ds.map((d) => ({ from: d.start, to: d.end, severity: d.severity, message: d.message }));
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
            diags = ds.map((d) => ({ from: d.start, to: d.end, severity: d.severity, message: d.message }));
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
            diags = ds.map((d) => ({ from: d.start, to: d.end, severity: d.severity, message: d.message }));
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
  const allDiags = $derived([...diags, ...spellDiags, ...mojibakeDiags, ...propertyDiags, ...strutsDiags]);

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
    | { kind: 'version'; hint: CargoVersionHint };
  let lenses: EditorLens[] = [];

  /** Push the current list, keyed by index — the key only has to identify a lens within the list the
   *  editor is showing right now, which is what comes back on a press. */
  function pushLenses(next: EditorLens[]) {
    lenses = next;
    editorComp?.setCodeLenses(
      next.map((entry, key) =>
        entry.kind === 'lsp'
          ? { start: entry.lens.start, title: entry.lens.title, actionable: !!entry.lens.command, key }
          : {
              start: entry.hint.offset,
              // An arrow and the accent tone, because this one is an OFFER rather than a count: it
              // has to survive being glanced past, and a grey line above a line of code reads as a
              // comment. The word stays so the first one is unambiguous.
              title: `↑ ${entry.hint.latest} available`,
              actionable: true,
              tone: 'accent' as const,
              key,
            },
      ),
    );
  }

  $effect(() => {
    const path = activePath;
    if (!path || !isLspFileOf(path)) {
      // Not cleared for a manifest: that buffer's lenses come from the effect below, and clearing
      // here would race it into an empty layer on every keystroke.
      if (!isCargoManifest(path)) pushLenses([]);
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
  async function onLensPress(key: number) {
    const entry = lenses[key];
    if (!entry) return;
    if (entry.kind === 'version') {
      updateDependencyVersion(entry.hint);
      return;
    }
    const path = activePath;
    const lens = entry.lens;
    if (!path || !lens.command || !editorComp) return;

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
        || /(^|[\\/])(application|bootstrap)[^\\/]*\.(ya?ml|properties)$/i.test(path));
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

  /** Glyph per gutter-mark kind. Text, not an icon set: the shared editor draws whatever
   *  string it is given, so a new kind costs a character here and nothing anywhere else. */
  const GUTTER_GLYPHS: Record<string, string> = {
    bean: '◆',       // ◆ a bean is declared on this line
    inject: '→',     // → something is injected here
    endpoint: '»',   // » a route enters here
    entity: '▤',     // ▤ a persistent entity — points at the repositories that manage it
    repository: '◇', // ◇ a repository — points at the entity it manages
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

  /**
   * Clicking a gutter icon opens what it points at — and when it points at more than one
   * thing, it asks.
   *
   * Silently picking one is the wrong answer twice over: it hides that there were others, and
   * the one it picks is the one *we* ranked rather than the one you meant. A bean injected in
   * six places has six real answers, so the menu is anchored at the pointer and lists them all.
   */
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

  /** Whether this file can hold a breakpoint at all. A decompiled view has no line numbers
   *  that mean anything to the VM, and a `.properties` file compiles to nothing — offering a
   *  gutter there is offering a click that can only ever be pending. */
  const canBreak = $derived(!!activePath && isJavaFileOf(activePath) && !isDecompiledView);

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
      const classes = ['cm-bp'];
      if (!b.enabled) classes.push('cm-bp-off');
      else if (status && !status.verified) classes.push('cm-bp-pending');
      return {
        line: b.line,
        className: classes.join(' '),
        tooltip: !b.enabled
          ? 'Breakpoint (disabled) — right-click for more'
          : (status?.message || 'Breakpoint — click to remove, right-click for more'),
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
    const t = setTimeout(() => { breakpointable = breakpointableLines(src); }, 200);
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
    const items: MenuItem[] = existing
      ? [
          { id: 'toggle', label: existing.enabled ? 'Disable breakpoint' : 'Enable breakpoint' },
          { id: 'remove', label: 'Remove breakpoint' },
          { id: 'sep', separator: true },
          { id: 'clear', label: 'Remove all breakpoints in this project' },
        ]
      : [{ id: 'add', label: 'Set breakpoint' }];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      if (id === 'add') bennuDebugStore.toggleBreakpoint(root, path, line);
      else if (id === 'remove') bennuDebugStore.removeBreakpoint(root, path, line);
      else if (id === 'clear') bennuDebugStore.clearBreakpoints(root);
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
            severity: d.severity as EditorDiagnostic['severity'],
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
  export function focusEditor() { editorComp?.focus(); }
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
    return Wand2; // the simplification family (isEmpty / boolean / negated comparison)
  }

  /** Move the file into the folder matching its declared package (the `move-to-package` intention).
   *  Delegates to the store (save → move → re-point tab → refresh tree) and reports the outcome. */
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
    const dynamic: IntentionItem[] = [];
    if (isJavaFileOf(path)) {
      const src = editorComp.getValue();
      const offset = editorComp.caretByteOffset();
      const offers = await ipcIntentionsAt(path, src, offset).catch(() => []);
      if (projectStore.activeFilePath === path) {
        for (const o of offers) {
          dynamic.push({
            id: o.id,
            label: o.label,
            icon: intentionIcon(o.id),
            // A non-edit action (a filesystem move) is dispatched by the store; a plain edit
            // applies the byte-range replacement in place.
            run: o.action === 'move-to-package'
              ? () => void moveFileToPackage(path)
              : () => editorComp?.replaceByteRange(o.start, o.end, o.replacement),
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
      const actions = await lspCodeActions(path, src, start, end).catch(() => []);
      if (projectStore.activeFilePath === path) {
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
    if (!isLspFileOf(path)) {
      toastStore.show('No formatter for this file type', 'info');
      return;
    }
    const src = editorComp.getValue();
    const edits = await lspFormat(
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

  /** Insert text at the caret (Generate modal → editor). Mirrors merula's insert. */
  export function insertAtCursor(text: string) { editorComp?.insertAtCursor(text); }

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
    if (refocus) editorComp?.focus();
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
    if (!activePath || !editorComp || !isLspFileOf(activePath)) return;
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
  /** What implements the trait at the caret (and, by direction, what it is built on). */
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
   * field's candidate beans, and — in a bean XML — `class=`, `ref=` and
   * `<property name=>`. Empty on a caret that is none of those, which is most of a file.
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
    // 1a. A server-backed buffer stops here. Everything below is a Java-stack resolver — a JSP
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
    // 1a-bis. Framework extensions (Spring): a `${property}` key → its `application*.yml`
    //     entry, a `@Qualifier` / SpEL `@bean` → the bean declaration, an injected field →
    //     its candidate beans, and in a bean XML a `class=`, a `ref=` or a
    //     `<property name=>` → the member it names. Runs AFTER the language's own answer,
    //     because a caret inside a string literal or an XML attribute is invisible to the
    //     Java resolver by construction — this is the only place those can resolve.
    if (offset != null && (await tryGoToFrameworkExt(offset))) return;
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
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, onEditorMenuSelect);
  }
  function onEditorMenuSelect(id: string) {
    switch (id) {
      case 'cut': editorComp?.cutSelection(); break;
      case 'copy': editorComp?.copySelection(); break;
      case 'paste': void editorComp?.pasteClipboard(); break;
      case 'gotodef': goToDefinition(); break;
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
         THE per-file action bar — new file-type tools slot into `.ed-actions`. -->
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
        <IconButton tooltip="Go to line" shortcut="Ctrl+G" size={26} onclick={openGoto}>
          <Hash size={13} />
        </IconButton>
      </div>
    </div>
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

  {#if activePath}
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
          gutterMarks={springGutterMarks}
          onGutterClick={onSpringGutterClick}
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

  {#if activePath}
    <div class="ed-footer">
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

  /* Gutter icons: colour by what the mark means, so a glance separates "a bean is
     declared here" from "something is injected here" without reading the tooltip. */
  :global(.cm-fw-gutter-bean) { color: var(--success); }
  :global(.cm-fw-gutter-inject) { color: var(--info); }
  :global(.cm-fw-gutter-endpoint) { color: var(--warning); }
  :global(.cm-fw-gutter-entity) { color: var(--syntax-field, #9876aa); }
  :global(.cm-fw-gutter-repository) { color: var(--syntax-function, #ffc66d); }
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

  /* Where the program is stopped, on the frame you are looking at. A full-width band with a
     bar down its left edge — the band says WHICH row and the bar says it is the execution
     point rather than a selection, which is the distinction IntelliJ draws the same way. */
  :global(.cm-paused-line) {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
</style>
