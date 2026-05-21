<!--
  StudioDiffPane — format-agnostic diff view for every Studio modal.

  Two sub-views over the same parsed document (toggled by an internal
  Tree/Text segmented control with per-format localStorage persistence):
    · Tree — structural diff (host-computed via `backend.treeDiff`).
             Walks the AST, marks added/removed/modified/partial nodes,
             auto-expands every Partial ancestor so changes are visible.
    · Text — unified line hunks (host-computed via `backend.diff`) with
             prev/next chunk navigation (F3 / Shift+F3 from parent).

  The pane owns its load lifecycle: refreshes on `(visible, docId,
  refreshTick, currentText)` so it auto-updates on edits and can be
  kicked explicitly by the parent after save/RON↔JSON / format calls
  (where the original snapshot moves but `currentText` doesn't).

  Imperative API (via `bind:this`):
    · `nav(delta)` — advance the active sub-view's selection. Routed
      by parent's F3 / Shift+F3 global handler.

  Variant-tag chips (RON renders the struct/variant name with the
  parent's `.rs-row-tag` class) flow through the optional `tagChip`
  snippet so other formats can omit them.
-->
<script lang="ts" module>
  import type { Snippet } from 'svelte';
  import type {
    StudioBackend, StudioFormat, DiffHunk, DiffTreeNode,
  } from '$lib/ipc/studio-format';

  export interface StudioDiffPaneProps<TKind extends string = string> {
    /** Identifies the format for localStorage namespacing. */
    formatId: StudioFormat;
    /** Pre-bound backend. Uses `.diff(docId)` and `.treeDiff(docId)`. */
    backend: StudioBackend<TKind>;
    /** Active doc id — null short-circuits both sub-views to empty. */
    docId: string | null;
    /** Parent's visibility gate (i.e. diff tab active). When false the
     *  refresh effects skip; state is preserved (re-show is fast). */
    visible: boolean;
    /** Live text of the document. Tracked as an effect dep so the
     *  diff auto-refreshes on every edit while the pane is visible. */
    currentText: string;
    /** Monotonic counter the parent increments to force a refresh
     *  (post-save, post-mutation — anywhere `currentText` doesn't
     *  change but the diff result might). */
    refreshTick?: number;
    /** Override the localStorage key for the Tree/Text sub-toggle.
     *  Default: `arbor:studio:${formatId}:diff-sub`. RON keeps its
     *  legacy key for user-pref continuity. */
    storageKey?: string;
    /** Renders a variant/struct-name chip alongside a diff row. RON
     *  passes a snippet that uses `.rs-row-tag` (+ `.rs-row-tag-before`
     *  on the "before" side). Other formats can omit. */
    tagChip?: Snippet<[string, 'before' | 'after']>;
    /** Surfaced for the parent's tab badge — number of structural
     *  changes (Tree sub-view) and line hunks (Text sub-view). */
    treeChangeCount?: number;
    hunkCount?: number;
  }

  export interface StudioDiffPaneController {
    /** Step prev/next inside the active sub-view (Tree changes when
     *  Tree is active, hunks when Text is active). */
    nav(delta: 1 | -1): void;
  }
</script>

<script lang="ts" generics="TKind extends string">
  import { ListTree, FileText, ChevronUp, ChevronDown, ChevronRight, Check } from 'lucide-svelte';
  import Spinner from '../ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    formatId,
    backend,
    docId,
    visible,
    currentText,
    refreshTick = 0,
    storageKey,
    tagChip,
    treeChangeCount = $bindable(0),
    hunkCount = $bindable(0),
  }: StudioDiffPaneProps<TKind> = $props();

  // ── Sub-view toggle (Tree | Text) ────────────────────────────────
  type DiffSubView = 'text' | 'tree';
  const SUB_KEY = $derived(storageKey ?? `arbor:studio:${formatId}:diff-sub`);

  function loadSub(): DiffSubView {
    if (typeof localStorage === 'undefined') return 'tree';
    const v = localStorage.getItem(SUB_KEY) as DiffSubView | null;
    return v === 'text' || v === 'tree' ? v : 'tree';
  }
  let diffSubView = $state<DiffSubView>(loadSub());
  function setDiffSubView(v: DiffSubView) {
    diffSubView = v;
    if (typeof localStorage !== 'undefined') {
      try { localStorage.setItem(SUB_KEY, v); } catch { /* ignore */ }
    }
  }

  // ── Text diff (line hunks) ───────────────────────────────────────
  let diffHunks    = $state<DiffHunk[]>([]);
  let diffLoading  = $state(false);
  let currentChunkIdx = $state(0);
  let hunkEls: HTMLElement[] = [];
  let diffPaneEl: HTMLDivElement | undefined = $state();

  async function refreshTextDiff() {
    if (!docId) { diffHunks = []; hunkCount = 0; return; }
    diffLoading = true;
    try {
      diffHunks = await backend.diff(docId);
      hunkCount = diffHunks.length;
      if (currentChunkIdx >= diffHunks.length) currentChunkIdx = 0;
    } catch (e) {
      diffHunks = []; hunkCount = 0;
      console.warn(`${formatId}-studio: diff failed`, e);
    } finally {
      diffLoading = false;
    }
  }

  function goToChunk(idx: number) {
    if (diffHunks.length === 0) return;
    const wrapped = ((idx % diffHunks.length) + diffHunks.length) % diffHunks.length;
    currentChunkIdx = wrapped;
    const el = hunkEls[wrapped];
    if (el) el.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }

  // ── Structural (tree) diff ───────────────────────────────────────
  type Node = DiffTreeNode<TKind>;
  let diffTree         = $state<Node | null>(null);
  let diffTreeLoading  = $state(false);
  let diffTreeExpanded = $state<Set<string>>(new Set());
  let diffChanges      = $state<Node[]>([]);
  let currentTreeChangeIdx = $state(0);
  let diffTreeRowEls = $state<Record<string, HTMLElement | null>>({});

  function diffNodeId(n: Node): string {
    return n.path.join('\x00') || '$';
  }

  async function refreshTreeDiff() {
    if (!docId) {
      diffTree = null; diffChanges = []; treeChangeCount = 0; return;
    }
    diffTreeLoading = true;
    try {
      diffTree = await backend.treeDiff(docId);
      // Auto-expand every Partial ancestor so changes are visible.
      const next = new Set<string>();
      function walk(n: Node) {
        if (n.status === 'partial') {
          next.add(diffNodeId(n));
          for (const c of n.children) walk(c);
        }
      }
      walk(diffTree);
      diffTreeExpanded = next;
      diffChanges = flattenChanges(diffTree);
      treeChangeCount = diffTree.change_count;
      if (currentTreeChangeIdx >= diffChanges.length) currentTreeChangeIdx = 0;
    } catch (e) {
      diffTree = null; diffChanges = []; treeChangeCount = 0;
      console.warn(`${formatId}-studio: tree diff failed`, e);
    } finally {
      diffTreeLoading = false;
    }
  }

  /** Depth-first flatten of all non-Partial, non-Unchanged leaves —
   *  the ones the user navigates with F3 / Shift+F3 / chevrons. */
  function flattenChanges(root: Node): Node[] {
    const out: Node[] = [];
    function walk(n: Node) {
      if (n.status === 'added' || n.status === 'removed' || n.status === 'modified') {
        out.push(n);
      }
      for (const c of n.children) walk(c);
    }
    walk(root);
    return out;
  }

  function goToTreeChange(idx: number) {
    if (diffChanges.length === 0) return;
    const wrapped = ((idx % diffChanges.length) + diffChanges.length) % diffChanges.length;
    currentTreeChangeIdx = wrapped;
    const target = diffChanges[wrapped];
    const next = new Set(diffTreeExpanded);
    for (let i = 0; i < target.path.length; i++) {
      next.add(target.path.slice(0, i).join('\x00') || '$');
    }
    diffTreeExpanded = next;
    queueMicrotask(() => {
      const el = diffTreeRowEls[diffNodeId(target)];
      if (el) el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    });
  }

  function toggleDiffNode(id: string) {
    const next = new Set(diffTreeExpanded);
    if (next.has(id)) next.delete(id); else next.add(id);
    diffTreeExpanded = next;
  }

  type Row = { node: Node; depth: number };
  const diffVisibleRows = $derived.by<Row[]>(() => {
    if (!diffTree) return [];
    const out: Row[] = [];
    function walk(n: Node, depth: number) {
      out.push({ node: n, depth });
      if (n.status === 'partial' && diffTreeExpanded.has(diffNodeId(n))) {
        for (const c of n.children) walk(c, depth + 1);
      }
    }
    walk(diffTree, 0);
    return out;
  });

  // ── Refresh lifecycle ────────────────────────────────────────────
  // Re-runs whenever the pane becomes visible, the doc changes, the
  // live text edits, or the parent kicks the tick.
  // Always refresh so parent's badge counts stay live even when the
  // diff tab isn't visible. The compute is host-side and cheap; gating
  // by `visible` made the badge update at scoppio ritardato on switch.
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void docId;
    void currentText;
    void refreshTick;
    if (refreshTimer) clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => {
      void refreshTextDiff();
      void refreshTreeDiff();
    }, 120);
    return () => { if (refreshTimer) clearTimeout(refreshTimer); };
  });

  // Reset all state when the doc switches.
  $effect(() => {
    void docId;
    diffHunks = []; hunkCount = 0; currentChunkIdx = 0;
    diffTree = null; diffChanges = []; treeChangeCount = 0;
    diffTreeExpanded = new Set();
    currentTreeChangeIdx = 0;
    diffTreeRowEls = {};
  });

  // ── Imperative API ───────────────────────────────────────────────
  export function nav(delta: 1 | -1) {
    if (diffSubView === 'tree') goToTreeChange(currentTreeChangeIdx + delta);
    else                         goToChunk(currentChunkIdx + delta);
  }
</script>

<div class="sdp-root">
  <div class="sdp-toolbar">
    <!-- Sub-view toggle: Tree (structural) vs Text (line hunks). -->
    <div class="sdp-subtoggle" role="tablist" aria-label="Diff view">
      <button type="button" role="tab" class="sdp-subtoggle-btn" class:active={diffSubView === 'tree'}
        onclick={() => setDiffSubView('tree')} use:tooltip={'Tree diff — show path-level changes'}>
        <ListTree size={12} /> Tree
      </button>
      <button type="button" role="tab" class="sdp-subtoggle-btn" class:active={diffSubView === 'text'}
        onclick={() => setDiffSubView('text')} use:tooltip={'Text diff — unified hunks against original'}>
        <FileText size={12} /> Text
      </button>
    </div>
    {#if diffSubView === 'text' && diffHunks.length > 0}
      <div class="sdp-chunk-nav" use:tooltip={'Navigate change chunks (F3 / Shift+F3)'}>
        <button class="sdp-chunk-btn" onclick={() => goToChunk(currentChunkIdx - 1)}
          use:tooltip={{ content: 'Previous chunk', shortcut: 'Shift+F3' }} aria-label="Previous chunk">
          <ChevronUp size={12} />
        </button>
        <span class="sdp-chunk-counter">{currentChunkIdx + 1}/{diffHunks.length}</span>
        <button class="sdp-chunk-btn" onclick={() => goToChunk(currentChunkIdx + 1)}
          use:tooltip={{ content: 'Next chunk', shortcut: 'F3' }} aria-label="Next chunk">
          <ChevronDown size={12} />
        </button>
      </div>
    {:else if diffSubView === 'tree' && diffChanges.length > 0}
      <div class="sdp-chunk-nav" use:tooltip={'Navigate changes (F3 / Shift+F3)'}>
        <button class="sdp-chunk-btn" onclick={() => goToTreeChange(currentTreeChangeIdx - 1)}
          use:tooltip={{ content: 'Previous change', shortcut: 'Shift+F3' }} aria-label="Previous change">
          <ChevronUp size={12} />
        </button>
        <span class="sdp-chunk-counter">{currentTreeChangeIdx + 1}/{diffChanges.length}</span>
        <button class="sdp-chunk-btn" onclick={() => goToTreeChange(currentTreeChangeIdx + 1)}
          use:tooltip={{ content: 'Next change', shortcut: 'F3' }} aria-label="Next change">
          <ChevronDown size={12} />
        </button>
      </div>
    {/if}
    <span class="sdp-spacer"></span>
    {#if diffSubView === 'tree'}
      {#if diffTreeLoading}
        <span class="sdp-meta">Computing diff…</span>
      {:else if diffTree && diffTree.change_count === 0}
        <span class="sdp-meta">No changes vs. original.</span>
      {:else if diffTree}
        <span class="sdp-meta">{diffTree.change_count} change{diffTree.change_count === 1 ? '' : 's'}</span>
      {/if}
    {:else}
      {#if !diffLoading && diffHunks.length === 0}
        <span class="sdp-meta">No changes vs. original.</span>
      {/if}
    {/if}
  </div>

  {#if diffSubView === 'tree'}
    <div class="sdp-tree-pane">
      {#if diffTree && diffTree.change_count > 0}
        {#each diffVisibleRows as { node, depth }, ri (diffNodeId(node))}
          {@const id = diffNodeId(node)}
          {@const isOpen = diffTreeExpanded.has(id)}
          {@const isContainer = node.status === 'partial'}
          {@const isCurrent = (node.status === 'added' || node.status === 'removed' || node.status === 'modified')
                              && diffChanges[currentTreeChangeIdx] && diffNodeId(diffChanges[currentTreeChangeIdx]) === id}
          <div
            class="sdp-row sdp-row-{node.status}"
            class:sdp-row-current={isCurrent}
            style:padding-left="{8 + depth * 14}px"
            bind:this={diffTreeRowEls[id]}
            role="treeitem"
            aria-selected={isCurrent}
          >
            {#if isContainer}
              <button type="button" class="sdp-caret"
                onclick={() => toggleDiffNode(id)}
                aria-label={isOpen ? 'Collapse' : 'Expand'}
              >
                {#if isOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
              </button>
            {:else}
              <span class="sdp-caret sdp-caret-empty"></span>
            {/if}

            <span class="sdp-status sdp-status-{node.status}" use:tooltip={node.status}>
              {#if node.status === 'added'}+
              {:else if node.status === 'removed'}−
              {:else if node.status === 'modified'}~
              {:else if node.status === 'partial'}…
              {:else}·{/if}
            </span>

            <span class="sdp-key">{node.key}</span>

            {#if node.status === 'added'}
              {#if node.tag_after && tagChip}{@render tagChip(node.tag_after, 'after')}{/if}
              <span class="sdp-preview sdp-side-after">{node.preview_after ?? ''}</span>
            {:else if node.status === 'removed'}
              {#if node.tag_before && tagChip}{@render tagChip(node.tag_before, 'before')}{/if}
              <span class="sdp-preview sdp-side-before">{node.preview_before ?? ''}</span>
            {:else if node.status === 'modified'}
              {#if node.tag_before && tagChip}{@render tagChip(node.tag_before, 'before')}{/if}
              <span class="sdp-preview sdp-side-before">{node.preview_before ?? ''}</span>
              <span class="sdp-arrow">→</span>
              {#if node.tag_after && tagChip}{@render tagChip(node.tag_after, 'after')}{/if}
              <span class="sdp-preview sdp-side-after">{node.preview_after ?? ''}</span>
            {:else if node.status === 'partial'}
              {#if (node.tag_after ?? node.tag_before) && tagChip}
                {@render tagChip((node.tag_after ?? node.tag_before)!, 'after')}
              {/if}
              <span class="sdp-partial-count">{node.change_count} change{node.change_count === 1 ? '' : 's'}</span>
            {/if}
          </div>
        {/each}
      {:else if diffTreeLoading}
        <div class="sdp-empty"><Spinner size="sm" /> <span>Computing diff…</span></div>
      {:else}
        <div class="sdp-empty">
          <Check size={14} />
          <span>No changes — document matches the saved/loaded original.</span>
        </div>
      {/if}
    </div>
  {:else}
    <div class="sdp-text-pane" bind:this={diffPaneEl}>
      {#each diffHunks as hunk, i (i)}
        <div class="sdp-hunk" bind:this={hunkEls[i]} class:active={i === currentChunkIdx}>
          <div class="sdp-hunk-header">
            @@ -{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count} @@
          </div>
          {#each hunk.lines as ln, li (li)}
            <div class="sdp-line sdp-{ln.kind}">
              <span class="sdp-num">{ln.old_line ?? ''}</span>
              <span class="sdp-num">{ln.new_line ?? ''}</span>
              <span class="sdp-marker">{ln.kind === 'add' ? '+' : ln.kind === 'del' ? '-' : ' '}</span>
              <span class="sdp-text">{ln.text}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .sdp-root {
    display: flex; flex-direction: column;
    flex: 1; min-height: 0;
  }

  /* ── Toolbar ─────────────────────────────────────────────────── */
  .sdp-toolbar {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 10px;
    background: var(--bg-overlay);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    flex-shrink: 0;
  }
  .sdp-spacer { flex: 1; }
  .sdp-meta { color: var(--text-muted); font-size: 11px; }

  .sdp-chunk-nav {
    display: inline-flex; align-items: center; gap: 2px;
    padding: 1px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
  }
  .sdp-chunk-btn {
    display: inline-flex; align-items: center;
    padding: 2px 5px;
    background: transparent; color: var(--text-secondary);
    border: none; cursor: pointer; border-radius: 2px;
  }
  .sdp-chunk-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .sdp-chunk-counter {
    font-family: var(--font-ui-sans); font-size: 10px;
    color: var(--text-muted); padding: 0 4px;
  }

  /* Tree/Text sub-toggle — small segmented control in the toolbar. */
  .sdp-subtoggle {
    display: inline-flex;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px;
  }
  .sdp-subtoggle-btn {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 8px;
    background: transparent; color: var(--text-secondary);
    border: none; border-radius: 3px;
    font-size: 11px; cursor: pointer;
  }
  .sdp-subtoggle-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .sdp-subtoggle-btn.active { background: var(--accent-subtle); color: var(--accent); }

  /* ── Tree diff body ──────────────────────────────────────────── */
  .sdp-tree-pane {
    flex: 1; overflow: auto;
    background: var(--bg-base);
    font-family: var(--font-code); font-size: 11px;
    padding: 4px 0;
  }
  .sdp-row {
    display: flex; align-items: center; gap: 6px;
    padding: 2px 12px 2px 0;
    line-height: 1.5;
    border-left: 2px solid transparent;
    min-height: 22px;
  }
  .sdp-row-added    { background: color-mix(in srgb, var(--success, #98c379) 8%, transparent); }
  .sdp-row-removed  { background: color-mix(in srgb, var(--error, #e06c75)   8%, transparent); }
  .sdp-row-modified { background: color-mix(in srgb, var(--accent)           8%, transparent); }
  /* .sdp-row-partial — container, no row tint */
  .sdp-row-current  {
    border-left-color: var(--accent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .sdp-caret {
    display: inline-flex; align-items: center; justify-content: center;
    width: 16px; height: 16px;
    background: transparent;
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .sdp-caret:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .sdp-caret-empty { cursor: default; }
  .sdp-caret-empty:hover { background: transparent; }

  .sdp-status {
    display: inline-flex; align-items: center; justify-content: center;
    width: 14px; height: 14px;
    border-radius: 3px;
    font-family: var(--font-code); font-size: 11px; font-weight: 700;
    color: #fff;
    flex-shrink: 0;
  }
  .sdp-status-added    { background: var(--success, #98c379); }
  .sdp-status-removed  { background: var(--error, #e06c75); }
  .sdp-status-modified { background: var(--accent); }
  .sdp-status-partial  { background: var(--text-muted); color: var(--bg-base); }

  .sdp-key {
    font-family: var(--font-code); font-size: 11px;
    color: var(--text-primary);
    font-weight: 500;
    flex-shrink: 0;
  }
  .sdp-preview {
    font-family: var(--font-code); font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    min-width: 0;
  }
  .sdp-side-before {
    color: var(--error, #e06c75);
    text-decoration: line-through;
    text-decoration-thickness: 1px;
    opacity: 0.8;
    flex-shrink: 1;
  }
  .sdp-side-after {
    color: var(--success, #98c379);
    flex-shrink: 1;
  }
  .sdp-row-modified .sdp-side-before {
    color: var(--text-muted);
    text-decoration: line-through;
  }
  .sdp-row-modified .sdp-side-after {
    color: var(--accent);
  }
  .sdp-arrow {
    color: var(--text-muted);
    font-family: var(--font-code);
    flex-shrink: 0;
  }
  .sdp-partial-count {
    color: var(--text-muted);
    font-size: 10px;
    margin-left: auto;
    padding: 1px 6px;
    background: var(--bg-overlay);
    border-radius: 8px;
  }

  .sdp-empty {
    display: flex; align-items: center; gap: 8px;
    padding: 20px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 12px;
  }

  /* ── Text diff body ──────────────────────────────────────────── */
  .sdp-text-pane {
    flex: 1; overflow: auto;
    background: var(--bg-base);
    font-family: var(--font-code); font-size: 11px;
  }
  .sdp-hunk { border-top: 1px solid var(--border-subtle); }
  .sdp-hunk:first-child { border-top: none; }
  .sdp-hunk.active { box-shadow: inset 3px 0 0 var(--accent); }
  .sdp-hunk-header {
    padding: 4px 12px;
    background: var(--bg-overlay);
    color: var(--text-muted);
    font-size: 10px;
  }
  .sdp-line {
    display: grid;
    grid-template-columns: 40px 40px 16px 1fr;
    gap: 0; padding: 0 6px; line-height: 1.55;
  }
  .sdp-num {
    color: var(--text-disabled);
    font-size: 10px;
    text-align: right;
    padding-right: 8px;
    user-select: none;
  }
  .sdp-marker { color: var(--text-muted); text-align: center; }
  .sdp-text { white-space: pre; overflow: hidden; text-overflow: ellipsis; }
  .sdp-add { background: color-mix(in srgb, var(--success, #98c379) 10%, transparent); }
  .sdp-add .sdp-marker, .sdp-add .sdp-text { color: var(--success, #98c379); }
  .sdp-del { background: color-mix(in srgb, var(--danger, #e06c75) 10%, transparent); }
  .sdp-del .sdp-marker, .sdp-del .sdp-text { color: var(--danger, #e06c75); }
</style>
