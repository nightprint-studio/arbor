<script lang="ts">
  /**
   * Editor pane: a JetBrains-style tab strip over the open `.grove` files, a
   * code-block toolbar (breadcrumb · goto-line · copy), and the CodeMirror 6
   * editor (`GroveEditor`) with Tree-sitter highlight + lint + active-hap. Drives
   * off the real `projectStore` (path-keyed source model from Step 1).
   *
   * Edits flow `GroveEditor → projectStore.setSource` and trigger a debounced
   * re-eval (`groveEngine.eval`) whose diagnostics come back through the store;
   * switching tabs re-evals immediately so lint matches the visible file. Exposes
   * `openGoto()` + `newFile()` for the GroveShell keybindings (Ctrl+G / Ctrl+N).
   *
   * Imports only shared/ui (Tabs, EmptyState) + grove-local code + the tooltip
   * action — the domain editor lives entirely under grove/.
   */
  import { untrack } from 'svelte';
  import { Hash, FileMusic, BookLock, Copy, ChevronRight, MapPin } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import GroveEditor from './GroveEditor.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { groveEngine } from '../stores/engine.svelte';
  import { groveStore } from '../grove-store.svelte';

  type EditorController = {
    focus: () => void;
    openSearch: () => void;
    scrollToLineCol: (line: number, col?: number) => void;
    scrollToOffset: (offset: number, select?: boolean) => void;
    gotoSymbol: (name: string) => boolean;
    commitControls: (
      index: number,
      edits: import('./grove-edit').ControlEdit[],
    ) => { treeReady: boolean; applied: number; skipped: string[] };
  };
  let editorComp = $state<EditorController | null>(null);

  const activePath = $derived(projectStore.activeFilePath);
  const openPaths = $derived(projectStore.openFilePaths);

  /** File metadata for a path — from the project manifest, else derived from the
   *  path (a file opened from outside the project). */
  function fileMeta(path: string) {
    const f = projectStore.files.find((x) => x.path === path);
    if (f) return { name: f.name, rel: f.rel, library: f.library };
    const name = path.split(/[\\/]/).pop() ?? path;
    return { name, rel: name, library: false };
  }

  const tabs = $derived<TabItem[]>(
    openPaths.map((p) => {
      const m = fileMeta(p);
      return { id: p, label: m.name, icon: m.library ? BookLock : FileMusic, iconSize: 13, title: p };
    }),
  );

  const crumbs = $derived(activePath ? fileMeta(activePath).rel.split('/') : []);

  // ── Re-eval: debounced on edit, immediate on tab switch ──────────────────────
  let evalTimer: ReturnType<typeof setTimeout> | null = null;
  function evalActive() {
    void groveEngine.eval(projectStore.activeSource, projectStore.project?.path);
  }
  function scheduleEval() {
    if (evalTimer) clearTimeout(evalTimer);
    evalTimer = setTimeout(evalActive, 300);
  }

  function onInput(text: string) {
    if (!activePath) return;
    projectStore.setSource(activePath, text);
    scheduleEval();   // async — never blocks typing
  }

  // Switching the active file re-evals it so diagnostics/active-haps line up
  // with the visible source (the highlight itself is client-side, no eval).
  // Track ONLY the path — reading the source is untracked so edits go through
  // the debounced `scheduleEval`, not this immediate one.
  $effect(() => {
    if (projectStore.activeFilePath) untrack(() => evalActive());
  });

  // ── Cross-file go-to-declaration ─────────────────────────────────────────────
  function crossFileGoto(word: string, importPath: string) {
    const target = projectStore.files.find(
      (f) => f.rel === importPath || f.rel.endsWith(importPath) || f.path.endsWith(importPath),
    );
    if (!target) return;
    void projectStore.openFile(target.path).then(() => gotoWhenReady(word, 0));
  }
  function gotoWhenReady(word: string, tries: number) {
    if (editorComp?.gotoSymbol(word)) return;
    if (tries >= 24) return;   // give up after ~0.4s of frames
    requestAnimationFrame(() => gotoWhenReady(word, tries + 1));
  }

  // ── Outline / Problems → editor jump (one-shot relay from the GroveShell store) ─
  // The Outline panel (left rail) and Problems panel (bottom) ask the editor to
  // jump to a source offset. Tracked by `seq` so the same target fired twice still
  // re-triggers. `seq` is consumed only once the editor is mounted (it may have
  // been collapsed) — and re-applied across a few frames to beat the CodeMirror
  // view-ready race when the pane was just revealed.
  let lastGotoSeq = 0;
  $effect(() => {
    const req = groveStore.gotoRequest;
    const comp = editorComp;
    if (!req || req.seq === lastGotoSeq || !comp) return;
    lastGotoSeq = req.seq;
    let tries = 0;
    const apply = () => {
      comp.scrollToOffset(req.offset, true);   // no-ops until the CM view exists
      if (++tries < 3) requestAnimationFrame(apply);
    };
    apply();
  });

  // ── Mixer / Inspector → editor commit (one-shot relay) ───────────────────────
  // A knob commit needs the editor's live Tree-sitter tree to resolve spans, so
  // it is routed here. Retried across frames until the grammar/tree is ready
  // (the editor may have just been revealed from a collapsed pane).
  let lastCommitSeq = 0;
  $effect(() => {
    const req = groveStore.commitRequest;
    const comp = editorComp;
    if (!req || req.seq === lastCommitSeq || !comp) return;
    lastCommitSeq = req.seq;
    let tries = 0;
    const apply = () => {
      const r = comp.commitControls(req.index, req.edits);
      if (!r.treeReady && ++tries < 24) requestAnimationFrame(apply);
    };
    apply();
  });

  // ── Goto-line overlay (Ctrl+G) ───────────────────────────────────────────────
  let gotoOpen = $state(false);
  let gotoValue = $state('');
  let gotoInputEl = $state<HTMLInputElement | null>(null);
  let copied = $state(false);

  export function openGoto() {
    if (!activePath) return;
    gotoOpen = true; gotoValue = '';
    queueMicrotask(() => gotoInputEl?.focus());
  }

  /** New `.grove` — delegated to the centralised project-action picker (writes a
   *  starter file into the project, opens it). Falls back to New Project when no
   *  project is open. */
  export function newFile() { projectActions.newFile(); }

  /** Open the editor's in-buffer search panel (Ctrl+F when the pane is focused). */
  export function openSearch() { editorComp?.openSearch(); }

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
    else if (e.key === 'Escape') { e.preventDefault(); gotoOpen = false; }
  }

  async function copySource() {
    if (!activePath) return;
    try {
      await navigator.clipboard.writeText(projectStore.activeSource);
      copied = true; setTimeout(() => copied = false, 1200);
    } catch { /* clipboard blocked — ignore */ }
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
        onSelect={(id) => projectStore.openFile(id)}
        onClose={(id) => projectStore.closeFile(id)}
        onAdd={newFile}
        addLabel="New .grove (Ctrl+N)"
      />
    </div>

    <div class="ed-toolbar">
      <div class="ed-crumbs">
        {#each crumbs as c, i (i)}
          {#if i > 0}<ChevronRight size={12} class="crumb-sep" />{/if}
          <span class="crumb" class:last={i === crumbs.length - 1}>{c}</span>
        {/each}
      </div>
      <div class="ed-actions">
        <button class="ed-tool" use:tooltip={'Go to line (Ctrl+G)'} aria-label="Go to line" onclick={openGoto}><Hash size={13} /></button>
        <button class="ed-tool" use:tooltip={copied ? 'Copied!' : 'Copy source'} aria-label="Copy source" onclick={copySource}><Copy size={13} /></button>
      </div>
    </div>
  {/if}

  {#if activePath}
    {#key activePath}
      <GroveEditor
        bind:this={editorComp}
        value={projectStore.sourceOf(activePath)}
        oninput={onInput}
        oncaret={(line, col) => groveStore.setCaret(line, col)}
        onCrossFileGoto={crossFileGoto}
      />
    {/key}
  {:else}
    <div class="ed-empty">
      <EmptyState
        message="No file open. Open a project (Ctrl+O) or create a new .grove (Ctrl+Shift+N)."
      />
    </div>
  {/if}

  {#if activePath}
    <div class="ed-footer">
      <span class="ed-pos"><MapPin size={11} /> Ln {groveStore.caretLine}, Col {groveStore.caretCol}</span>
    </div>
  {/if}

  {#if gotoOpen}
    <div class="ed-goto" role="dialog" aria-label="Go to line">
      <Hash size={13} />
      <input bind:this={gotoInputEl} bind:value={gotoValue} onkeydown={onGotoKey} onblur={() => gotoOpen = false} placeholder="Line or line:col…" inputmode="numeric" />
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

  /* Editor-local footer: caret position (moved off the window footer). */
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
  :global(.crumb-sep) { color: var(--text-disabled); flex-shrink: 0; }

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
