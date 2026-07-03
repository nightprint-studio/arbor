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
    PenLine, Wand2, Save, Eye, X, ArrowRightToLine, LocateFixed,
  } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { languageForPath } from './languages';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { diagnostics as ipcDiagnostics } from '$lib/ipc/bennu';
  import {
    definition as ipcDefinition, references as ipcReferences,
    declaration as ipcDeclaration,
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
    coordsAtCaret: () => { x: number; y: number } | null;
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

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  const tabs = $derived<TabItem[]>(
    openPaths.map((p) => ({ id: p, label: baseName(p), icon: FileCode2, iconSize: 13, title: p })),
  );

  // ── Caret position (footer, via the UI store) ────────────────────────────────
  let caretLine = $state(1);
  let caretCol = $state(1);
  function onCaret(line: number, col: number) {
    caretLine = line; caretCol = col;
    bennuUiStore.setCaret(line, col);
  }

  // ── Goto relay: Structure / Outline / Problems request a jump; scroll there. ──
  $effect(() => {
    const t = bennuUiStore.gotoTarget;
    if (!t) return;
    // Read `nonce` so a repeat jump to the same line re-fires.
    void t.nonce;
    editorComp?.scrollToLineCol(t.line, 1);
  });

  // ── Edits → store ────────────────────────────────────────────────────────────
  function onInput(text: string) {
    if (activePath) projectStore.setSource(activePath, text);
  }

  // ── Diagnostics (byte spans) from the backend, re-fetched per active file ─────
  // Phase 0: bennu_diagnostics returns []; the wiring is here so it lights up for
  // free once the backend produces real diagnostics.
  let diags = $state<EditorDiagnostic[]>([]);
  $effect(() => {
    const path = activePath;
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

  /** Find usages of the symbol under the caret — opens the popover anchored there and
   *  fills it from `bennu_references`. Graceful: an unresolvable caret shows the
   *  empty state, never throws. */
  export async function findUsages() {
    if (!activePath || !editorComp) return;
    const source = editorComp.getValue();
    const offset = editorComp.caretByteOffset();
    bennuRefactorStore.startUsages(editorComp.coordsAtCaret(), editorComp.wordAtCaret());
    try {
      const res = await ipcReferences(activePath, source, offset);
      bennuRefactorStore.setUsages(res?.target_label ?? null, res?.usages ?? []);
    } catch {
      bennuRefactorStore.setUsages(null, []);
    }
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

  /** Open a resolved target file and scroll to its top (the definition site). The
   *  goto relay drives the scroll after the cross-file open settles. */
  function openDefinitionFile(path: string) {
    void projectStore.openFile(path).then(() => bennuUiStore.requestGoto(1));
  }

  /** Try the BE go-to-declaration for the symbol at `offset` (any Java symbol — class,
   *  method, field, local). Resolves via `bennu_declaration` and jumps to the declaring
   *  file + line. Returns true when it jumped; false (gracefully) when the BE isn't
   *  attached, the symbol is JDK/dep-jar resident, or the caret isn't on a symbol. */
  async function tryGoToDeclarationBE(offset: number): Promise<boolean> {
    const path = activePath;
    if (!path || !editorComp) return false;
    const target = await ipcDeclaration(path, editorComp.getValue(), offset).catch(() => null);
    if (!target) return false;
    await projectStore.openFile(target.file);
    bennuUiStore.requestGoto(target.line);
    return true;
  }

  /** Try to resolve `word` to a project CLASS declaration — an instant, offline fallback
   *  (from the FE class index) used when the BE resolver is cold. Matches by exact simple
   *  name, then by an FQCN ending in `.word`, and jumps to the declaring file + line.
   *  Returns true when it jumped. */
  async function tryGoToClassDeclaration(word: string): Promise<boolean> {
    const root = projectStore.project?.root;
    if (!root || !word || !/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(word)) return false;
    const classes = await bennuIndexStore.classesForRoot(root).catch(() => null);
    if (!classes) return false;
    const hit = classes.find((c) => c.simple === word) ?? classes.find((c) => c.fqcn.endsWith('.' + word));
    if (!hit) return false;
    await projectStore.openFile(hit.file);
    bennuUiStore.requestGoto(hit.line);
    return true;
  }

  /** Resolve + navigate to the definition of `action` (a JSP action reference).
   *  Prefers the config fragment, then the view JSP; if only a class FQCN is known
   *  (no openable path), reports it. No target → an info toast. */
  async function resolveDefinition(action: string, offset?: number) {
    const path = activePath;
    if (!path) return;
    // 1. BE go-to-declaration — any Java symbol (class/method/field/local) — when we have
    //    a byte offset to classify at. Authoritative + precise (jumps to the exact line).
    if (offset != null && (await tryGoToDeclarationBE(offset))) return;
    // 2. Instant offline class-index fallback (types) when the BE resolver is cold.
    if (action) {
      if (await tryGoToClassDeclaration(action)) return;
    } else {
      toastStore.show('Nothing to go to here', 'info');
      return;
    }
    const seq = ++gotoDefSeq;
    let res;
    try {
      res = await ipcDefinition(path, action);
    } catch {
      if (seq === gotoDefSeq) toastStore.show('Go to definition unavailable', 'info');
      return;
    }
    if (seq !== gotoDefSeq) return; // superseded by a newer request
    if (!res) {
      toastStore.show(`No definition for “${action}”`, 'info');
      return;
    }
    // Prefer the config fragment (where the <action> is declared); fall back to the
    // resolved view JSP. Both are file paths we can open.
    const target = res.config_file || res.view_jsp;
    if (target) {
      openDefinitionFile(target);
      return;
    }
    // Only a class FQCN resolved — a name, not a path we can open yet.
    if (res.class_fqcn) {
      toastStore.show(`Maps to ${res.class_fqcn}`, 'info');
      return;
    }
    toastStore.show(`No definition for “${action}”`, 'info');
  }

  /** Go to definition of the action reference under the caret (Ctrl+B / palette).
   *  No-op (with a toast) when nothing reference-like is under the caret. */
  export function goToDefinition() {
    if (!activePath || !editorComp) return;
    const ref = editorComp.refAtCaret() ?? editorComp.wordAtCaret() ?? '';
    void resolveDefinition(ref, editorComp.caretByteOffset());
  }

  /** Ctrl/Cmd+Click seam from the editor: the reference token at the click position (an
   *  identifier, a string-literal's contents, or a path) + the clicked byte offset → go
   *  to declaration/definition. */
  function onEditorGoto(word: string, _view: EditorView, byteOffset: number) {
    if (!activePath) return;
    void resolveDefinition(word, byteOffset);
  }

  // ── Editor context menu (right-click) ─────────────────────────────────────────
  /** Generate (constructors/getters/setters) is Java-only — its source scan is a Java
   *  outline, meaningless (and historically a freeze risk) on a `.jsp`/XML file. */
  const isJavaFile = $derived(!!activePath && activePath.toLowerCase().endsWith('.java'));

  function onEditorContextMenu(e: MouseEvent) {
    if (!activePath) return;
    e.preventDefault();
    const items: MenuItem[] = [
      { id: 'cut', label: 'Cut', icon: Scissors, shortcut: 'Ctrl+X' },
      { id: 'copy', label: 'Copy', icon: Copy, shortcut: 'Ctrl+C' },
      { id: 'paste', label: 'Paste', icon: ClipboardPaste, shortcut: 'Ctrl+V' },
      { id: 's1', label: '', separator: true },
      { id: 'gotodef', label: 'Go to definition', icon: Target, shortcut: 'Ctrl+B' },
      { id: 'usages', label: 'Find usages', icon: SearchCode, shortcut: 'Alt+F7' },
      { id: 'rename', label: 'Rename…', icon: PenLine, shortcut: 'Shift+F6' },
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

    <div class="ed-toolbar">
      <div class="ed-crumbs">
        <span class="crumb last">{activePath ? baseName(activePath) : ''}</span>
      </div>
      <div class="ed-actions">
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

  .ed-editor-wrap { flex: 1; display: flex; min-width: 0; min-height: 0; }
  .ed-editor-wrap > :global(.code-editor) { flex: 1; min-width: 0; min-height: 0; }

  .ed-empty { flex: 1; display: flex; align-items: center; justify-content: center; min-height: 0; }

  .ed-footer {
    display: flex; align-items: center; justify-content: flex-end;
    height: 22px; min-height: 22px; flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-base);
    border-top: 1px solid var(--border-subtle);
    font-size: 11px; color: var(--text-muted);
    user-select: none;
  }
  .ed-pos { display: flex; align-items: center; gap: 4px; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .ed-pos :global(svg) { color: var(--text-disabled); }

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
