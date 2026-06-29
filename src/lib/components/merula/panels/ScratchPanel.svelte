<script lang="ts">
  /**
   * Scratch — the expression evaluator + mini audio tester, with tabs. Each tab
   * holds an independent chunk: paste, type, or load the editor selection, then
   * **evaluate** it in isolation to inspect the events it generates, and **play**
   * it one-shot to hear its effect — none of which touches the live arrangement.
   *
   * One CodeMirror instance backs the active tab (its doc swaps on tab change),
   * with the same highlight + autocomplete as the main editor. A snippet is
   * resolved against the active file's preamble (`withFileDeps`), so a bare
   * variable / an expression using file-level bindings evaluates + plays here too.
   * Errors come back inline from `merula_eval_snippet`.
   */
  import { onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import { FlaskConical, Play, Square, ClipboardPaste, Plus, X, CircleAlert, AlertTriangle } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { createMerulaExtensions } from '../editor/merula-cm';
  import { withFileDeps } from '../editor/merula-lang';
  import type { MerulaIntelSource } from '../editor/merula-intel';
  import { merulaEvalSnippet, type MerulaQueryHap } from '$lib/ipc/merula';
  import { merulaStore } from '../merula-store.svelte';
  import { merulaEngine } from '../stores/engine.svelte';
  import { scratchStore } from '../stores/scratch.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { editorSelectionStore } from '../stores/editor-selection.svelte';
  import { referenceStore } from '../stores/reference.svelte';
  import { soundsStore } from '../stores/sounds.svelte';
  import { noteName } from '../viz/arrangement.svelte';

  const intel: MerulaIntelSource = {
    entries: () => referenceStore.entries,
    byName: (name) => referenceStore.byName(name),
    instruments: () => soundsStore.instruments,
  };

  let hostEl = $state<HTMLDivElement | undefined>();
  let view: EditorView | undefined;
  let suppress = false;        // guard the updateListener during a programmatic swap
  let mountedId: string | null = null;
  // Per-tab debounce + in-flight token so tabs evaluate independently (switching
  // tabs never cancels or misroutes another tab's eval).
  const debounces = new Map<string, ReturnType<typeof setTimeout>>();
  const seqs = new Map<string, number>();

  const tabs = $derived(scratchStore.tabs);
  const active = $derived(scratchStore.active);
  const result = $derived(active.result);
  const evaluating = $derived(active.evaluating);
  const errors = $derived(result?.diagnostics ?? []);
  const haps = $derived(result?.haps ?? []);
  const hasSelection = $derived(editorSelectionStore.active);

  // Cap the readout so a dense snippet can't render thousands of rows.
  const MAX_ROWS = 250;
  const rows = $derived(haps.slice(0, MAX_ROWS));
  const overflow = $derived(haps.length - rows.length);

  function scheduleEval(tabId: string, source: string) {
    const prev = debounces.get(tabId);
    if (prev) clearTimeout(prev);
    if (!source.trim()) { scratchStore.setResult(tabId, null); scratchStore.setEvaluating(tabId, false); return; }
    scratchStore.setEvaluating(tabId, true);
    const timer = setTimeout(async () => {
      const seq = (seqs.get(tabId) ?? 0) + 1;
      seqs.set(tabId, seq);
      try {
        // Resolve against the active file's preamble so a bare variable / an
        // expression using file-level bindings evaluates here too.
        const full = await withFileDeps(projectStore.activeSource, source);
        const r = await merulaEvalSnippet(full, projectStore.project?.path);
        if (seqs.get(tabId) === seq) scratchStore.setResult(tabId, r);
      } finally {
        if (seqs.get(tabId) === seq) scratchStore.setEvaluating(tabId, false);
      }
    }, 250);
    debounces.set(tabId, timer);
  }

  function mount(target: HTMLDivElement) {
    const updateListener = EditorView.updateListener.of((u) => {
      if (u.docChanged && !suppress) {
        const text = u.state.doc.toString();
        const a = scratchStore.active;
        scratchStore.setSource(a.id, text);
        scheduleEval(a.id, text);
      }
    });
    const state = EditorState.create({
      doc: active.source,
      extensions: [createMerulaExtensions({ intel }), updateListener],
    });
    view = new EditorView({ state, parent: target });
    mountedId = active.id;
    if (active.source.trim() && !active.result) scheduleEval(active.id, active.source);
  }

  $effect(() => { if (hostEl && !view) mount(hostEl); });

  // Swap the editor doc when the active tab changes (or an external seed replaced
  // the active tab's text), without echoing it back through the updateListener.
  $effect(() => {
    const a = scratchStore.active;
    if (!view) return;
    if (a.id !== mountedId || view.state.doc.toString() !== a.source) {
      const switched = a.id !== mountedId;
      mountedId = a.id;
      suppress = true;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: a.source } });
      suppress = false;
      if (switched && a.source.trim() && !a.result) scheduleEval(a.id, a.source);
    }
  });

  onDestroy(() => { for (const t of debounces.values()) clearTimeout(t); view?.destroy(); view = undefined; });

  function loadSelection() {
    const r = editorSelectionStore.primary;
    if (!r) return;
    scratchStore.load(projectStore.activeSource.slice(r.from, r.to));
    view?.focus();
  }
  async function play() {
    const full = await withFileDeps(projectStore.activeSource, active.source);
    void merulaEngine.playSnippet(full, projectStore.project?.path);
  }
  function stop() { void merulaEngine.stopSnippet(); }

  // ── Tab rename (double-click the title) ──────────────────────────────────────
  let editingId = $state<string | null>(null);
  let editName = $state('');
  function startRename(id: string, current: string) { editingId = id; editName = current; }
  function commitRename() {
    if (editingId) scratchStore.renameTab(editingId, editName);
    editingId = null;
  }
  function renameKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commitRename(); }
    else if (e.key === 'Escape') { e.preventDefault(); editingId = null; }
  }
  // Focus + select on mount (Arbor hard rule forbids the `autofocus` attribute).
  function focusSelect(el: HTMLInputElement) { el.focus(); el.select(); }

  function onKeydown(e: KeyboardEvent) {
    // Ctrl/Cmd+Enter plays the active tab (CM doesn't bind it, so it bubbles here).
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void play(); }
  }

  /** bar:beat for a cycle position (1 cycle = 1 bar, 4 beats/bar). */
  function barBeat(cyc: number): string {
    const bar = Math.floor(cyc) + 1;
    const beat = (cyc - Math.floor(cyc)) * 4 + 1;
    return `${bar}:${beat.toFixed(2).replace(/\.?0+$/, '')}`;
  }
  function rowLabel(h: MerulaQueryHap): string {
    if (h.note != null) return noteName(h.note);
    if (h.sound) return h.sound;
    return h.has_onset ? 'event' : 'signal';
  }
</script>

<!-- The keydown handler only catches a shortcut (Ctrl/Cmd+Enter) that bubbles up
     from the CodeMirror editor — the container itself isn't a focus target, so the
     noninteractive-listener rule is a false positive here. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="scr" onkeydown={onKeydown} role="group">
  <BottomPanelHeader title="Scratch" onClose={() => merulaStore.toggleBottom('scratch')}>
    {#snippet icon()}<FlaskConical size={13} />{/snippet}
    {#snippet children()}
      {#if result}<span class="scr-meta">{haps.length} events · loop {result.loop_cycles} cyc</span>{/if}
    {/snippet}
  </BottomPanelHeader>

  <!-- Tabs: each holds an independent snippet. -->
  <div class="scr-tabs" role="tablist">
    {#each tabs as t (t.id)}
      <div class="scr-tab" class:active={t.id === scratchStore.activeId} role="tab" tabindex="-1"
           aria-selected={t.id === scratchStore.activeId}>
        {#if editingId === t.id}
          <input class="scr-tab-input" bind:value={editName} onkeydown={renameKey} onblur={commitRename}
                 use:focusSelect aria-label="Rename tab" />
        {:else}
          <button class="scr-tab-label" onclick={() => scratchStore.setActive(t.id)}
                  ondblclick={() => startRename(t.id, t.name)} use:tooltip={'Double-click to rename'}>{t.name}</button>
        {/if}
        <button class="scr-tab-close" onclick={() => scratchStore.closeTab(t.id)} aria-label="Close tab" use:tooltip={'Close'}>
          <X size={11} />
        </button>
      </div>
    {/each}
    <button class="scr-tab-add" onclick={() => scratchStore.addTab()} aria-label="New scratch tab" use:tooltip={'New tab'}>
      <Plus size={13} />
    </button>
  </div>

  <div class="scr-toolbar">
    <button class="scr-btn" onclick={loadSelection} disabled={!hasSelection}
            use:tooltip={hasSelection ? 'Load the editor selection into a new tab' : 'Select text in the editor first'}>
      <ClipboardPaste size={12} /> Load selection
    </button>
    <div class="scr-spacer"></div>
    <button class="scr-btn primary" onclick={() => void play()} disabled={!active.source.trim()}
            use:tooltip={{ content: 'Play one-shot', shortcut: 'Ctrl+Enter' }}>
      <Play size={12} fill="currentColor" /> Play
    </button>
    <button class="scr-btn" onclick={stop} use:tooltip={'Stop the preview'}>
      <Square size={11} fill="currentColor" /> Stop
    </button>
  </div>

  <div class="scr-body">
    <div class="scr-editor" bind:this={hostEl}></div>

    <div class="scr-readout">
      {#if evaluating}
        <div class="scr-state"><Spinner size={13} /> Evaluating…</div>
      {:else if errors.length > 0}
        {#each errors as e, i (i)}
          {@const isErr = e.severity === 'error'}
          <div class="scr-diag" class:err={isErr}>
            {#if isErr}<CircleAlert size={12} />{:else}<AlertTriangle size={12} />{/if}
            <span class="scr-diag-msg">{e.message}</span>
          </div>
        {/each}
      {:else if !active.source.trim()}
        <div class="scr-hint">Paste a snippet, load the selection, or type — then Play to hear it.</div>
      {:else if haps.length === 0}
        <div class="scr-hint">No events generated.</div>
      {:else}
        <div class="scr-rows">
          {#each rows as h, i (i)}
            <div class="scr-row">
              <span class="scr-time">{barBeat(h.start)}</span>
              <span class="scr-track">T{h.track + 1}</span>
              <span class="scr-name">{rowLabel(h)}</span>
              <span class="scr-gain">{h.gain != null ? h.gain.toFixed(2) : ''}</span>
            </div>
          {/each}
          {#if overflow > 0}<div class="scr-more">+{overflow} more…</div>{/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .scr { display: flex; flex-direction: column; height: 100%; background: var(--bg-base); }
  .scr-meta { font-size: 10.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  /* ── Tabs ─────────────────────────────────────────────────────────────────── */
  .scr-tabs {
    display: flex; align-items: stretch; gap: 2px;
    padding: 3px 6px 0; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
    overflow-x: auto;
  }
  .scr-tab {
    display: flex; align-items: center; gap: 2px;
    padding: 0 2px 0 9px; height: 26px; flex-shrink: 0;
    border: 1px solid transparent; border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .scr-tab:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .scr-tab.active {
    background: var(--bg-input); color: var(--text-primary);
    border-color: var(--border-subtle);
  }
  .scr-tab-label {
    background: transparent; border: none; cursor: pointer; color: inherit;
    font-size: 11.5px; font-family: var(--font-ui-sans); white-space: nowrap; padding: 0;
  }
  .scr-tab-input {
    width: 84px; background: var(--bg-base); color: var(--text-primary);
    border: 1px solid var(--border-focus); border-radius: var(--radius-sm);
    font-size: 11.5px; font-family: var(--font-ui-sans); padding: 1px 4px; outline: none;
  }
  .scr-tab-close, .scr-tab-add {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; padding: 0; flex-shrink: 0;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-muted); border-radius: var(--radius-sm);
  }
  .scr-tab-close:hover, .scr-tab-add:hover { background: var(--bg-hover); color: var(--text-primary); }
  .scr-tab-add { align-self: center; margin-left: 2px; }

  .scr-toolbar {
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .scr-spacer { flex: 1; }
  .scr-btn {
    display: flex; align-items: center; gap: 5px;
    height: 24px; padding: 0 9px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    background: var(--bg-input); color: var(--text-secondary); cursor: pointer;
    font-size: 11.5px; font-weight: 600; font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .scr-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .scr-btn:disabled { opacity: 0.4; cursor: default; }
  .scr-btn.primary {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border-subtle));
  }
  .scr-btn.primary:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 14%, transparent); }

  .scr-body { flex: 1; min-height: 0; display: flex; }
  .scr-editor {
    flex: 1.4; min-width: 0; min-height: 0; overflow: hidden;
    border-right: 1px solid var(--border-subtle);
  }
  .scr-editor :global(.cm-editor) { height: 100%; }
  .scr-readout { flex: 1; min-width: 0; min-height: 0; overflow-y: auto; padding: 4px 0; }

  .scr-state { display: flex; align-items: center; gap: 7px; padding: 12px 14px; font-size: 12px; color: var(--text-muted); }
  .scr-hint { padding: 12px 14px; font-size: 11.5px; color: var(--text-muted); font-style: italic; }

  .scr-diag {
    display: flex; align-items: flex-start; gap: 7px;
    padding: 6px 12px; font-size: 11.5px; color: var(--warning);
  }
  .scr-diag.err { color: var(--error); }
  .scr-diag-msg { color: var(--text-secondary); line-height: 1.35; }

  .scr-rows { font-family: var(--font-code); font-size: 11px; }
  .scr-row {
    display: grid; grid-template-columns: 52px 30px 1fr 44px; gap: 6px; align-items: center;
    padding: 2px 12px; color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .scr-row:hover { background: var(--bg-hover); }
  .scr-time { color: var(--text-muted); }
  .scr-track { color: var(--accent); }
  .scr-name { color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .scr-gain { text-align: right; color: var(--text-muted); }
  .scr-more { padding: 4px 12px; font-size: 10.5px; color: var(--text-muted); font-style: italic; }
</style>
