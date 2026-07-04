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
  } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { languageForPath } from './languages';
  import { isJavaFile as isJavaFileOf, supportsCodeNav } from './file-kind';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { diagnostics as ipcDiagnostics } from '$lib/ipc/bennu';
  import {
    definition as ipcDefinition, references as ipcReferences,
    actionUsages as ipcActionUsages,
    declaration as ipcDeclaration,
    jspNav as ipcJspNav,
    jspIncludeTarget as ipcJspIncludeTarget,
    beanClass as ipcBeanClass,
    mybatisNav as ipcMybatisNav,
    renameApply as ipcRenameApply, type RenameEdit,
  } from '$lib/ipc/bennu/nav';
  import { applyByteEdits } from './rename-apply';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { spellcheck as ipcSpellcheck, type SpellHit } from '$lib/ipc/bennu/spell';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';
  import type { EditorDiagnostic, EditorViewSnapshot } from '$lib/components/shared/ui/code-editor';
  import type { EditorView } from '@codemirror/view';
  import { bennuIntentionsStore } from '$lib/stores/bennu/intentions.svelte';
  import { bennuRefactorStore } from '$lib/stores/bennu/refactor.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuNavStore } from '$lib/stores/bennu/nav-history.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { collectIntentions, type GenerateMode } from './bennu-intentions';
  import { javaOutline } from './java-outline';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

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
    openSearch: () => void;
    scrollToLineCol: (line: number, col?: number) => void;
    scrollToByteOffset: (byteOffset: number) => void;
    coordsAtCaret: () => { x: number; y: number } | null;
    coordsAtByteOffset: (byteOffset: number) => { x: number; y: number } | null;
    setCaretAtCoords: (x: number, y: number) => boolean;
    wordAtCaret: () => string | null;
    refAtCaret: () => string | null;
    caretByteOffset: () => number;
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

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  const tabs = $derived<TabItem[]>(
    openPaths.map((p) => ({ id: p, label: baseName(p), icon: FileCode2, iconSize: 13, title: p })),
  );

  // ── Caret position (footer, via the UI store) ────────────────────────────────
  let caretLine = $state(1);
  let caretCol = $state(1);

  // ── Navigation history (Ctrl+Alt+←/→) ─────────────────────────────────────────
  // Record a "place" when the caret makes a real JUMP — a different file, or a big
  // in-file hop (a go-to / structure / find click) — not on every arrow keystroke. A
  // programmatic back/forward jump sets `suppressNav` so it doesn't record itself as a
  // fresh place (which would break the ring).
  let lastNav: { file: string; line: number } | null = null;
  let suppressNav = false;
  const NAV_JUMP_LINES = 3; // an in-file move larger than this counts as a jump

  function onCaret(line: number, col: number) {
    caretLine = line; caretCol = col;
    bennuUiStore.setCaret(line, col);

    const path = activePath;
    if (!path) return;
    if (suppressNav) {
      // This caret event is the landing of a Back/Forward jump — remember it, don't record.
      suppressNav = false;
      lastNav = { file: path, line };
      return;
    }
    const jumped = !lastNav || lastNav.file !== path || Math.abs(lastNav.line - line) > NAV_JUMP_LINES;
    if (jumped) bennuNavStore.record({ file: path, line, col });
    lastNav = { file: path, line };
  }

  /** Navigate to a recorded place (cross-file via the goto relay so the remounted editor
   *  picks it up on mount; same-file directly). `suppressNav` keeps the resulting caret
   *  event from recording a new place. */
  async function navGo(place: { file: string; line: number; col: number } | null) {
    if (!place) return;
    suppressNav = true;
    if (place.file !== projectStore.activeFilePath) {
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

  // ── Diagnostics (byte spans) from the backend, re-fetched per active file ─────
  // For a JSP the backend extracts + checks the `action="…"` refs (unknown → warning
  // squiggle). Re-fetched when the index rebuilds too (`buildRevision`), so squiggles
  // appear once the config graph finishes building after a fresh open.
  let diags = $state<EditorDiagnostic[]>([]);
  $effect(() => {
    const path = activePath;
    void bennuIndexStore.buildRevision; // re-run when the index (config graph) rebuilds
    if (!path) { diags = []; return; }
    let cancelled = false;
    void ipcDiagnostics(path)
      .then((ds) => {
        if (cancelled) return;
        diags = ds.map((d) => ({ from: d.start, to: d.end, severity: d.severity, message: d.message }));
      })
      .catch(() => { if (!cancelled) diags = []; });
    return () => { cancelled = true; };
  });

  // ── Spell-check (opt-in per project) — merged into the editor as hint squiggles ──
  // Runs against the live buffer (debounced), only when spell-check is enabled for
  // the project and dictionaries are installed. Each misspelled word carries quick-fix
  // actions: replace-with-suggestion + add-to-dictionary (project / global).
  let spellDiags = $state<EditorDiagnostic[]>([]);
  const allDiags = $derived([...diags, ...spellDiags]);

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

  // ── Intentions (Alt+Enter) ────────────────────────────────────────────────────
  /** Collect the context actions at the caret and open the intentions popup
   *  anchored there. No-op (with a toast) when no file is open or the caret has no
   *  anchor. The two "Generate…" items route through `onGenerate`. */
  export function openIntentions() {
    if (!activePath || !editorComp) return;
    const anchor = editorComp.coordsAtCaret();
    const items = collectIntentions(
      {
        src: projectStore.sourceOf(activePath),
        wordUnderCaret: editorComp.wordAtCaret(),
        outline: javaOutline(projectStore.sourceOf(activePath)),
      },
      { onGenerate: (mode) => onGenerate?.(mode) },
    );
    if (!items.length) {
      toastStore.show('No context actions here', 'info');
      return;
    }
    bennuIntentionsStore.openWith(items, anchor);
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
      // Group by file, then splice each file's byte edits and persist.
      const byFile = new Map<string, RenameEdit[]>();
      for (const e of edits) {
        const list = byFile.get(e.file);
        if (list) list.push(e); else byFile.set(e.file, [e]);
      }
      for (const [file, fileEdits] of byFile) {
        const current = await projectStore.loadText(file);
        await projectStore.saveText(file, applyByteEdits(current, fileEdits));
      }
      toastStore.show(
        `Renamed to “${target}” · ${edits.length} edit(s) in ${byFile.size} file(s)`,
        'success',
      );
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
      if (!isJavaFile) {
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

      const res = await ipcReferences(activePath, source, offset);
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
    void projectStore.openFile(path).then(() => {
      if (offset && offset > 0) bennuUiStore.requestGotoOffset(offset);
      else bennuUiStore.requestGoto(1);
    });
  }

  /** Normalize two paths for identity comparison (forward slashes, case-fold for the
   *  Windows FS). The BE returns forward-slash paths; the FE's `activePath` may carry
   *  native separators. */
  function isSamePath(a: string, b: string): boolean {
    const n = (p: string) => p.replace(/\\/g, '/').toLowerCase();
    return n(a) === n(b);
  }

  /** Try the BE go-to-declaration for the symbol at `offset` (any Java symbol — class,
   *  method, field, local). Resolves via `bennu_declaration` and jumps to the declaring
   *  file + line. When the click/caret is **already on the declaration itself** (its name
   *  token in this same file — a method signature, or a variable/class/record decl),
   *  go-to-declaration would be a no-op, so we fall back to **find usages** at that offset
   *  (IntelliJ's Ctrl+Click / Ctrl+B behaviour on a declaration). Returns true when it
   *  handled the gesture; false (gracefully) when the BE isn't attached, the symbol is
   *  JDK/dep-jar resident, or the caret isn't on a symbol. */
  async function tryGoToDeclarationBE(offset: number, word: string | null): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp) return false;
    // Java-symbol resolution only makes sense in a `.java` buffer. On a JSP/XML the Java
    // resolver would parse the text as Java and could mis-fire on a coincidental symbol
    // name (e.g. the `viewTree` inside `action="viewTree"` matching a Java method), hijacking
    // the gesture before the config-graph resolver (`bennu_definition`) gets its turn.
    if (!path.toLowerCase().endsWith('.java')) return false;
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
    const path = activePath;
    if (!path) return;
    // 1. BE go-to-declaration — any Java symbol (class/method/field/local) — when we have
    //    a byte offset to classify at. Authoritative + precise (jumps to the exact line).
    if (offset != null && (await tryGoToDeclarationBE(offset, action))) return;
    // 1b. JSP page-scoped variable (a `<c:set var>`/`${var}` under the caret) — single-file,
    //     resolved off the buffer with no project index (only runs for non-`.java` files).
    if (offset != null && (await tryGoToJspVar(offset))) return;
    // 1c. JSP include / view reference (a `<%@ include file>` / `<jsp:include page>` /
    //     `<s:include value>` path under the caret) — a `.jsp`/`.jspf` path is unambiguous, so
    //     resolve it before the Struts-action ref (they're disjoint). Non-`.java` files only.
    if (action && (await tryGoToJspInclude(action))) return;
    // 1c-bis. MyBatis mapper XML: a statement `id` → the Java interface method, `namespace` →
    //     the interface, an `<include refid>` → its `<sql>`, a `resultMap="…"` → its
    //     `<resultMap>`. Offset-classified (needs no ref token); a non-mapper XML is a no-op.
    if (offset != null && (await tryGoToMybatis(offset))) return;
    // 1d. Config XML (`struts.xml`/`spring-*.xml`/`tiles.xml`): a `class="…"` value — an FQCN
    //     or a Spring bean id — go to the Java class it names.
    if (action && (await tryGoToXmlClass(action))) return;
    // 2. Instant offline class-index fallback (types) when the BE resolver is cold.
    if (action) {
      if (await tryGoToClassDeclaration(action)) return;
    } else {
      if (!silent) toastStore.show('Nothing to go to here', 'info');
      return;
    }
    const seq = ++gotoDefSeq;
    let res;
    try {
      res = await ipcDefinition(path, action);
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
    // Only a class FQCN resolved — try to open that class from the index; else report it.
    if (res.class_fqcn) {
      if (await tryGoToClassDeclaration(res.class_fqcn)) return;
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
    const items: MenuItem[] = [
      { id: 'cut', label: 'Cut', icon: Scissors, shortcut: 'Ctrl+X' },
      { id: 'copy', label: 'Copy', icon: Copy, shortcut: 'Ctrl+C' },
      { id: 'paste', label: 'Paste', icon: ClipboardPaste, shortcut: 'Ctrl+V' },
      ...navItems,
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
      case 'save':
        void projectStore.saveActive().then((ok) => { if (ok) toastStore.show('Saved', 'success'); });
        break;
    }
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
        {#if isValidationFile}
          <!-- Struts validation-file tools (JPA-Buddy-style). -->
          <button class="ed-tbtn" use:tooltip={'Validation reference'} onclick={() => bennuUiStore.toggleDocs()}>
            <BookOpen size={12} /> Reference
          </button>
          <button class="ed-tbtn primary" use:tooltip={'Add a field validator'} onclick={() => bennuUiStore.openValidationCreator()}>
            <Plus size={12} /> New validator
          </button>
          <span class="ed-tsep"></span>
        {/if}
        <button class="ed-tool" use:tooltip={{ content: 'Go to line', shortcut: 'Ctrl+G' }} aria-label="Go to line" onclick={openGoto}><Hash size={13} /></button>
      </div>
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
          diagnostics={allDiags}
          rulerColumn={bennuSettingsStore.rightMargin}
          tabSize={bennuSettingsStore.tabSize}
          indentUnit={bennuSettingsStore.indentStyle === 'tabs' ? '\t' : ' '.repeat(bennuSettingsStore.tabSize)}
          initialState={viewStates.get(activePath)}
          oninput={onInput}
          oncaret={onCaret}
          onViewState={(s) => { if (activePath) viewStates.set(activePath, s); }}
          onGoto={onEditorGoto}
        />
      {/key}
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
  .crumb { font-size: 11px; color: var(--text-muted); white-space: nowrap; }
  .crumb.last { color: var(--text-secondary); font-weight: 500; }

  .ed-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  .ed-tool {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 22px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ed-tool:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* File-type action buttons in the editor toolbar (text+icon, next to the icon tools). */
  .crumb-icon { color: var(--accent); flex-shrink: 0; margin-right: 2px; }
  .ed-tbtn {
    display: flex; align-items: center; gap: 5px;
    height: 20px; padding: 0 8px;
    background: transparent; border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary); font-size: 11px; cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .ed-tbtn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ed-tbtn.primary { background: var(--accent); border-color: var(--accent); color: var(--text-on-accent); }
  .ed-tbtn.primary:hover { filter: brightness(1.08); }
  .ed-tsep { width: 1px; height: 16px; background: var(--border-subtle); margin: 0 3px; }

  .ed-editor-wrap { flex: 1; display: flex; min-width: 0; min-height: 0; }
  .ed-editor-wrap > :global(.code-editor) { flex: 1; min-width: 0; min-height: 0; }

  .ed-empty { flex: 1; display: flex; align-items: center; justify-content: center; min-height: 0; }

  .ed-footer {
    display: flex; align-items: center; justify-content: flex-end; gap: 8px;
    height: 22px; min-height: 22px; flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-base);
    border-top: 1px solid var(--border-subtle);
    font-size: 11px; color: var(--text-muted);
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
    font-size: 12px; width: 140px;
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
    font-size: 12.5px; width: 160px;
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
</style>
