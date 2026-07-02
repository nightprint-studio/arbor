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
  import { Hash, FileCode2, MapPin } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { tooltip } from '$lib/actions/tooltip';
  import { javaLanguage } from './java-lang';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { diagnostics as ipcDiagnostics } from '$lib/ipc/bennu';
  import { definition as ipcDefinition } from '$lib/ipc/bennu/nav';
  import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor';
  import { bennuIntentionsStore } from '$lib/stores/bennu/intentions.svelte';
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
    insertAtCursor: (text: string) => void;
  };
  let editorComp = $state<EditorController | null>(null);

  const activePath = $derived(projectStore.activeFilePath);
  const openPaths = $derived(projectStore.openFilePaths);

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

  /** Resolve + navigate to the definition of `action` (a JSP action reference).
   *  Prefers the config fragment, then the view JSP; if only a class FQCN is known
   *  (no openable path), reports it. No target → an info toast. */
  async function resolveDefinition(action: string) {
    const path = activePath;
    if (!path) return;
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
    const ref = editorComp.refAtCaret();
    if (!ref) { toastStore.show('Nothing to go to here', 'info'); return; }
    void resolveDefinition(ref);
  }

  /** Ctrl/Cmd+Click seam from the editor: the reference token at the click position
   *  (an identifier, a string-literal's contents, or a path) → go to definition. */
  function onEditorGoto(word: string) {
    if (!activePath) return;
    void resolveDefinition(word);
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
    {#key activePath}
      <CodeEditor
        bind:this={editorComp}
        value={projectStore.sourceOf(activePath)}
        language={javaLanguage}
        diagnostics={diags}
        oninput={onInput}
        oncaret={onCaret}
        onGoto={onEditorGoto}
      />
    {/key}
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
</style>
