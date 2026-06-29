<script lang="ts">
  /**
   * Editor pane: a JetBrains-style tab strip over the open `.merula` files, a
   * code-block toolbar (breadcrumb · goto-line · copy), and the CodeMirror 6
   * editor (`MerulaEditor`) with Tree-sitter highlight + lint + active-hap. Drives
   * off the real `projectStore` (path-keyed source model from Step 1).
   *
   * Edits flow `MerulaEditor → projectStore.setSource` and trigger a debounced
   * re-eval (`merulaEngine.eval`) whose diagnostics come back through the store;
   * switching tabs re-evals immediately so lint matches the visible file. Exposes
   * `openGoto()` + `newFile()` for the MerulaShell keybindings (Ctrl+G / Ctrl+N).
   *
   * Imports only shared/ui (Tabs, EmptyState) + merula-local code + the tooltip
   * action — the domain editor lives entirely under merula/.
   */
  import { untrack } from 'svelte';
  import { Hash, FileMusic, BookLock, Copy, ChevronRight, MapPin } from 'lucide-svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import type { TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import MerulaEditor from './MerulaEditor.svelte';
  import EuclidGenModal from '../shell/EuclidGenModal.svelte';
  import ChordProgModal from '../shell/ChordProgModal.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { merulaEngine } from '../stores/engine.svelte';
  import { merulaStore } from '../merula-store.svelte';
  import { usagesStore, type UsageItem, type UsageAnchor } from '../stores/usages.svelte';
  import { structureStore } from '../stores/structure.svelte';
  import { intentionsStore } from '../stores/intentions.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { withFileDeps } from './merula-lang';
  import { merulaMaterialize } from '$lib/ipc/merula';
  import type { MerulaSymbol } from './merula-lang';
  import type { IntentionItem } from './merula-intentions';
  import type { EditChange } from './merula-edit';

  type EditorController = {
    focus: () => void;
    openSearch: () => void;
    scrollToLineCol: (line: number, col?: number) => void;
    scrollToOffset: (offset: number, select?: boolean) => void;
    insertAtCursor: (text: string) => void;
    gotoSymbol: (name: string) => boolean;
    findUsages: () => { name: string | null; items: UsageItem[]; anchor: UsageAnchor | null } | null;
    getStructure: () => MerulaSymbol[];
    formatDocument: () => Promise<{ ok: boolean; error?: string }>;
    prepareRename: () => { name: string; anchor: UsageAnchor | null } | null;
    applyRename: (oldName: string, newName: string) => { ok: boolean; error?: string; note?: string };
    prepareExtract: () => { from: number; to: number; suggested: string; anchor: UsageAnchor | null } | null;
    applyExtract: (from: number, to: number, name: string) => { ok: boolean; error?: string; note?: string };
    applyInline: () => { ok: boolean; error?: string; note?: string };
    getIntentions: () => { items: IntentionItem[]; anchor: UsageAnchor | null } | null;
    applyIntentionEdits: (edits: EditChange[]) => void;
    prepareChangeScale: () => { spec: string; anchor: UsageAnchor | null } | null;
    applyChangeScale: (newSpec: string) => { ok: boolean; error?: string; note?: string };
    commitControls: (
      index: number,
      edits: import('./merula-edit').ControlEdit[],
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
    void merulaEngine.eval(projectStore.activeSource, projectStore.project?.path);
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

  // ── Outline / Problems → editor jump (one-shot relay from the MerulaShell store) ─
  // The Outline panel (left rail) and Problems panel (bottom) ask the editor to
  // jump to a source offset. Tracked by `seq` so the same target fired twice still
  // re-triggers. `seq` is consumed only once the editor is mounted (it may have
  // been collapsed) — and re-applied across a few frames to beat the CodeMirror
  // view-ready race when the pane was just revealed.
  let lastGotoSeq = 0;
  $effect(() => {
    const req = merulaStore.gotoRequest;
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
    const req = merulaStore.commitRequest;
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

  // ── Find-usages relay (Alt+F7 / Command Palette) ─────────────────────────────
  let lastUsagesSeq = 0;
  $effect(() => {
    const seq = merulaStore.findUsagesSeq;
    if (seq === lastUsagesSeq) return;
    lastUsagesSeq = seq;
    if (seq > 0) findUsages();
  });

  // ── Format relay (Command Palette → store seq; the Alt+Shift+L shortcut calls
  // formatDocument() directly via the editor ref). ────────────────────────────
  let lastFormatSeq = 0;
  $effect(() => {
    const seq = merulaStore.formatSeq;
    if (seq === lastFormatSeq) return;
    lastFormatSeq = seq;
    if (seq > 0) void formatDocument();
  });

  // ── Structure popup relay (Ctrl+F12 / Command Palette) ───────────────────────
  let lastStructureSeq = 0;
  $effect(() => {
    const seq = merulaStore.structureSeq;
    if (seq === lastStructureSeq) return;
    lastStructureSeq = seq;
    if (seq > 0) openStructure();
  });

  // ── Refactor relays (Command Palette → store seq; shortcuts call directly) ───
  let lastRenameSeq = 0, lastExtractSeq = 0, lastInlineSeq = 0;
  $effect(() => {
    const seq = merulaStore.renameSeq;
    if (seq === lastRenameSeq) return;
    lastRenameSeq = seq;
    if (seq > 0) startRename();
  });
  $effect(() => {
    const seq = merulaStore.extractSeq;
    if (seq === lastExtractSeq) return;
    lastExtractSeq = seq;
    if (seq > 0) startExtract();
  });
  $effect(() => {
    const seq = merulaStore.inlineSeq;
    if (seq === lastInlineSeq) return;
    lastInlineSeq = seq;
    if (seq > 0) inlineSymbol();
  });
  let lastFreezeSeq = 0;
  $effect(() => {
    const seq = merulaStore.freezeSeq;
    if (seq === lastFreezeSeq) return;
    lastFreezeSeq = seq;
    if (seq > 0) startFreeze();
  });
  let euclidOpen = $state(false);
  let lastEuclidSeq = 0;
  $effect(() => {
    const seq = merulaStore.euclidSeq;
    if (seq === lastEuclidSeq) return;
    lastEuclidSeq = seq;
    if (seq > 0) euclidOpen = true;
  });
  let chordOpen = $state(false);
  let lastChordSeq = 0;
  $effect(() => {
    const seq = merulaStore.chordSeq;
    if (seq === lastChordSeq) return;
    lastChordSeq = seq;
    if (seq > 0) chordOpen = true;
  });

  // ── Intentions relay: open from shortcut/palette, and apply the chosen action ─
  let lastIntentOpenSeq = 0;
  $effect(() => {
    const seq = merulaStore.intentionsSeq;
    if (seq === lastIntentOpenSeq) return;
    lastIntentOpenSeq = seq;
    if (seq > 0) showIntentions();
  });
  let lastIntentPickSeq = 0;
  $effect(() => {
    const seq = intentionsStore.pendingSeq;
    if (seq === lastIntentPickSeq) return;
    lastIntentPickSeq = seq;
    const it = intentionsStore.pending;
    if (!it) return;
    if (it.ui === 'rename') startRename();
    else if (it.ui === 'extract') startExtract();
    else if (it.ui === 'scale') startChangeScale();
    else if (it.ui === 'freeze' && it.freeze) void freezeRange(it.freeze.from, it.freeze.to);
    else if (it.edits) {
      editorComp?.applyIntentionEdits(it.edits);
      if (it.note) toastStore.show(it.note, 'success');
    }
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

  /** New `.merula` — delegated to the centralised project-action picker (writes a
   *  starter file into the project, opens it). Falls back to New Project when no
   *  project is open. */
  export function newFile() { projectActions.newFile(); }

  /** Open the editor's in-buffer search panel (Ctrl+F when the pane is focused). */
  export function openSearch() { editorComp?.openSearch(); }

  /** Reformat the active file to canonical style (Alt+Shift+L / Command Palette).
   *  On a syntax error the buffer is left as-is and a toast points the user at the
   *  lint markers, which already show where the problem is. */
  export async function formatDocument() {
    const r = await editorComp?.formatDocument();
    if (r && !r.ok && r.error) {
      toastStore.show('Format skipped — fix the syntax error first', 'warning');
    }
  }

  /** Open the file-structure picker (Ctrl+F12 / Command Palette) — a filterable
   *  list of every track / fn / let / import to jump to. */
  export function openStructure() {
    structureStore.openWith(editorComp?.getStructure() ?? []);
  }

  // ── Structural refactors (rename · extract → let · inline) ───────────────────
  // Rename + Extract share a small floating name input (anchored at the caret);
  // Inline applies straight away. The pure planners live in `merula-refactor`;
  // here we only drive the UI and surface the outcome as a toast.
  type RefactorKind = 'rename' | 'extract' | 'scale';
  let refactor = $state<{
    kind: RefactorKind;
    title: string;
    value: string;
    error: string | null;
    anchor: UsageAnchor | null;
    oldName?: string;       // rename
    from?: number; to?: number; // extract
  } | null>(null);
  let refactorInputEl = $state<HTMLInputElement | null>(null);

  function openRefactorInput(next: NonNullable<typeof refactor>) {
    refactor = next;
    queueMicrotask(() => { refactorInputEl?.focus(); refactorInputEl?.select(); });
  }

  /** Rename the symbol under the caret (Shift+F6 / Command Palette). */
  export function startRename() {
    const r = editorComp?.prepareRename();
    if (!r) { toastStore.show('Place the caret on a name you defined (let / fn / import)', 'info'); return; }
    openRefactorInput({ kind: 'rename', title: 'Rename', value: r.name, oldName: r.name, error: null, anchor: r.anchor });
  }

  /** Extract the selected pattern into a named let (Alt+Shift+V / Command Palette). */
  export function startExtract() {
    const r = editorComp?.prepareExtract();
    if (!r) { toastStore.show('Select a complete pattern to extract', 'info'); return; }
    openRefactorInput({ kind: 'extract', title: 'Extract to let', value: r.suggested, from: r.from, to: r.to, error: null, anchor: r.anchor });
  }

  /** Change the scale at the caret (Alt+Enter on a `.scale("…")`). */
  function startChangeScale() {
    const r = editorComp?.prepareChangeScale();
    if (!r) { toastStore.show('Place the caret on a .scale("…") call', 'info'); return; }
    openRefactorInput({ kind: 'scale', title: 'Change scale', value: r.spec, error: null, anchor: r.anchor });
  }

  /** Freeze the selected pattern to concrete notes: evaluate it (resolved against
   *  the file's constants/imports) and replace the selection with the literal
   *  `n(…)` / `s(…)` it produces over one cycle. Alt+Enter intention + palette. */
  async function freezeRange(from: number, to: number) {
    const src = projectStore.activeSource;
    const fragment = src.slice(from, to);
    if (!fragment.trim()) return;
    try {
      const snippet = await withFileDeps(src, fragment);
      const frozen = await merulaMaterialize(snippet, projectStore.project?.path);
      if (!frozen.trim()) { toastStore.show('Nothing to freeze in the selection', 'info'); return; }
      editorComp?.applyIntentionEdits([{ from, to, insert: frozen }]);
      toastStore.show('Frozen to notes', 'success');
    } catch {
      toastStore.show('Could not freeze the pattern', 'warning');
    }
  }

  /** Palette/shortcut entry: freeze the current selection (resolved to a complete
   *  pattern span, like Extract). */
  export function startFreeze() {
    const r = editorComp?.prepareExtract();
    if (!r) { toastStore.show('Select a complete pattern to freeze', 'info'); return; }
    void freezeRange(r.from, r.to);
  }

  /** Inline the let under the caret (Alt+Shift+N / Command Palette). */
  export function inlineSymbol() {
    const r = editorComp?.applyInline();
    if (!r) return;
    if (!r.ok) { if (r.error) toastStore.show(r.error, 'warning'); }
    else if (r.note) toastStore.show(r.note, 'success');
  }

  /** Show the context-actions popup at the caret (Alt+Enter / Command Palette). */
  export function showIntentions() {
    const r = editorComp?.getIntentions();
    if (!r) return;
    if (!r.items.length) { toastStore.show('No context actions here', 'info'); return; }
    intentionsStore.openWith(r.items, r.anchor);
  }

  function commitRefactor() {
    if (!refactor) return;
    const res = refactor.kind === 'rename'
      ? editorComp?.applyRename(refactor.oldName ?? '', refactor.value)
      : refactor.kind === 'scale'
        ? editorComp?.applyChangeScale(refactor.value)
        : editorComp?.applyExtract(refactor.from ?? 0, refactor.to ?? 0, refactor.value);
    if (!res) { refactor = null; return; }
    if (!res.ok) { refactor = { ...refactor, error: res.error ?? 'Refactor failed' }; return; }
    if (res.note) toastStore.show(res.note, 'success');
    refactor = null;
  }
  function onRefactorKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); commitRefactor(); }
    else if (e.key === 'Escape') { e.preventDefault(); refactor = null; editorComp?.focus(); }
  }
  const refactorPos = $derived.by(() => {
    const a = refactor?.anchor;
    const vw = window.innerWidth, vh = window.innerHeight;
    let x = a ? a.x : vw / 2 - 130;
    let y = a ? a.y + 6 : vh / 3;
    x = Math.min(Math.max(8, x), vw - 268);
    y = Math.min(Math.max(8, y), vh - 120);
    return { x, y };
  });

  /** Find usages of the identifier under the caret → open the floating popover
   *  anchored at the caret (Alt+F7 / Command Palette). No-op when the caret isn't
   *  on a name. */
  export function findUsages() {
    const res = editorComp?.findUsages();
    if (!res) return;
    usagesStore.openAt(res.name, res.items, res.anchor);
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
        draggable
        onSelect={(id) => projectStore.openFile(id)}
        onClose={(id) => projectStore.closeFile(id)}
        onReorder={(from, to) => projectStore.reorderTab(from, to)}
        onAdd={newFile}
        addLabel="New .merula (Ctrl+N)"
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
        <button class="ed-tool" use:tooltip={{ content: 'Go to line', shortcut: 'Ctrl+G' }} aria-label="Go to line" onclick={openGoto}><Hash size={13} /></button>
        <button class="ed-tool" use:tooltip={copied ? 'Copied!' : 'Copy source'} aria-label="Copy source" onclick={copySource}><Copy size={13} /></button>
      </div>
    </div>
  {/if}

  {#if activePath}
    {#key activePath}
      <MerulaEditor
        bind:this={editorComp}
        value={projectStore.sourceOf(activePath)}
        oninput={onInput}
        oncaret={(line, col) => merulaStore.setCaret(line, col)}
        onCrossFileGoto={crossFileGoto}
      />
    {/key}
  {:else}
    <div class="ed-empty">
      <EmptyState
        message="No file open. Open a project (Ctrl+O) or create a new .merula (Ctrl+Shift+N)."
      />
    </div>
  {/if}

  {#if activePath}
    <div class="ed-footer">
      <span class="ed-pos"><MapPin size={11} /> Ln {merulaStore.caretLine}, Col {merulaStore.caretCol}</span>
    </div>
  {/if}

  {#if gotoOpen}
    <div class="ed-goto" role="dialog" aria-label="Go to line">
      <Hash size={13} />
      <input bind:this={gotoInputEl} bind:value={gotoValue} onkeydown={onGotoKey} onblur={() => gotoOpen = false} placeholder="Line or line:col…" inputmode="numeric" />
    </div>
  {/if}

  {#if refactor}
    <div class="ed-refactor" role="dialog" aria-label={refactor.title} style="left: {refactorPos.x}px; top: {refactorPos.y}px;">
      <span class="rf-title">{refactor.title}</span>
      <input
        bind:this={refactorInputEl}
        bind:value={refactor.value}
        onkeydown={onRefactorKey}
        onblur={() => (refactor = null)}
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        placeholder="New name…"
      />
      {#if refactor.error}<span class="rf-err">{refactor.error}</span>{/if}
    </div>
  {/if}
</div>

{#if euclidOpen}
  <EuclidGenModal
    projectDir={projectStore.project?.path}
    onInsert={(text) => editorComp?.insertAtCursor(text)}
    onClose={() => { euclidOpen = false; editorComp?.focus(); }}
  />
{/if}

{#if chordOpen}
  <ChordProgModal
    projectDir={projectStore.project?.path}
    onInsert={(text) => editorComp?.insertAtCursor(text)}
    onClose={() => { chordOpen = false; editorComp?.focus(); }}
  />
{/if}

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

  /* Refactor name input (rename / extract) — anchored at the caret (viewport
     coords, hence fixed), keyboard-driven (Enter commits, Esc cancels). */
  .ed-refactor {
    position: fixed;
    z-index: var(--z-popup, 1000);
    display: flex; flex-direction: column; gap: 4px;
    width: 260px;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup);
    padding: 7px 9px;
  }
  .rf-title { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .ed-refactor input {
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 4px 7px; outline: none;
    color: var(--text-primary); font-family: var(--font-code); font-size: 12px;
  }
  .ed-refactor input:focus { border-color: var(--border-focus, var(--accent)); }
  .ed-refactor input::placeholder { color: var(--text-disabled); }
  .rf-err { font-size: 10.5px; color: var(--error); line-height: 1.3; }
</style>
