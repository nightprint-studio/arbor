<script lang="ts">
  import { tick } from 'svelte';
  import { ChevronUp, ChevronDown } from 'lucide-svelte';
  import DiffHunkView from './DiffHunk.svelte';
  import VirtualHunk from './VirtualHunk.svelte';
  import ImageDiff from './ImageDiff.svelte';
  import DiffToolbar from './DiffToolbar.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { DiffFile } from '$lib/types/git';
  import { hunkLineKeys, buildStagePatch, buildUnstagePatch } from '$lib/utils/patch-builder';
  import { computeChunkAnchors, totalDiffLines, type ChunkAnchor } from '$lib/utils/diff-chunks';
  import { tooltipForAction, shortcutFor } from '$lib/utils/shortcut';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';

  /**
   * Imperative handle exposed by `DiffViewer` so callers rendering the
   * toolbar in their own chrome (e.g. `StageArea` putting it in the bottom
   * panel header) can read the relevant reactive state and invoke the same
   * actions the built-in toolbar would.
   */
  export interface DiffViewerApi {
    selectedCount: number;
    currentChunkIdx: number;
    copyDone: boolean;
    stageSelected: () => void;
    copyCode: () => void;
    openFullscreen: () => void;
    prevChunk: () => void;
    nextChunk: () => void;
  }

  let {
    file,
    path,
    stageable = false,
    staged = false,
    onStageLines,
    onEncodingChange,
    chromeless = false,
    api = $bindable<DiffViewerApi | undefined>(undefined),
  }: {
    file: DiffFile | null;
    path?: string;
    stageable?: boolean;
    staged?: boolean;
    onStageLines?: (patch: string) => void;
    /** Fired after the user pins / clears an encoding override for this
     *  file. The store is already updated when this runs — the parent
     *  should re-fetch the diff so the new label takes effect (the IPC
     *  layer reads overrides automatically from `encodingOverrides`). */
    onEncodingChange?: () => void;
    /** Hide the built-in `.diff-header` row. The parent is then responsible
     *  for rendering `<DiffToolbar>` somewhere (typically in its own
     *  `BottomPanelHeader`) and wiring it to `api`. */
    chromeless?: boolean;
    /** Bindable handle that exposes toolbar state and actions to the
     *  parent. Populated whenever a diff is on screen, otherwise undefined. */
    api?: DiffViewerApi | undefined;
  } = $props();

  const mode = $derived(diffStore.mode);
  const wordWrap = $derived(diffStore.wordWrap);
  const fullFile = $derived(diffStore.fullFile);

  // Total content lines drives the virtualization fallback. Word wrap forces
  // the simple renderer because variable-height rows break the fixed
  // ROW_HEIGHT virtual layout.
  const totalLines = $derived(totalDiffLines(file));
  const useVirtual = $derived(!wordWrap && totalLines > diffStore.virtThreshold);

  let viewerH = $state(0);

  // Full-screen overlay
  let fullscreen = $state(false);
  let fsMode = $state<'unified' | 'split'>('unified');

  function openFullscreen() { fsMode = mode === 'split' ? 'split' : 'unified'; fullscreen = true; }
  function closeFullscreen() { fullscreen = false; }
  // ESC handling lives in `<Modal>` — no need for a duplicate window-level
  // listener here (would race with Modal's own ESC dispatch).

  // ── Partial staging ────────────────────────────────────────────────────────

  let selectedLines = $state(new Set<string>());

  $effect(() => {
    file; // track
    selectedLines = new Set();
    currentChunkIdx = 0;

    // Auto-focus the first change chunk when the file ref changes — covers
    // both "file selected from the list" and "diff reloaded after partial
    // stage" (StageArea replaces the file ref with the still-unstaged hunks
    // remaining). Wait for the next tick so hunksEl is mounted; jump
    // instantly (no smooth scroll) so the user lands on the change before
    // even seeing the top of the file.
    if (file && file.hunks.length > 0) {
      tick().then(() => {
        if (chunkAnchors.length === 0) return;
        const target = fullscreen ? fsHunksEl : hunksEl;
        if (!target) return;
        const isSplit = (fullscreen ? fsMode : mode) === 'split';
        scrollToAnchor(target, chunkAnchors[0], isSplit, false);
      });
    }
  });

  function toggleLine(key: string) {
    const next = new Set(selectedLines);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    selectedLines = next;
  }

  function stageHunk(hunkIdx: number) {
    if (!file) return;
    const hunk = file.hunks[hunkIdx];
    if (!hunk) return;
    const allKeys = new Set(hunkLineKeys(hunkIdx, hunk));
    const patch = staged
      ? buildUnstagePatch(file, allKeys)
      : buildStagePatch(file, allKeys);
    if (patch) onStageLines?.(patch);
  }

  function stageSelected() {
    if (!file || selectedLines.size === 0) return;
    const patch = staged
      ? buildUnstagePatch(file, selectedLines)
      : buildStagePatch(file, selectedLines);
    if (patch) {
      onStageLines?.(patch);
      selectedLines = new Set();
    }
  }

  // ── Copy code ──────────────────────────────────────────────────────────────
  let copyDone = $state(false);
  function copyCode() {
    if (!file) return;
    const lines = file.hunks.flatMap(h =>
      h.lines
        .filter(l => l.kind !== 'removed')
        .map(l => l.content.replace(/\n$/, ''))
    );
    copyToClipboard(lines.join('\n'), { successToast: 'Code copied', errorToast: 'Copy failed' }).then(ok => {
      if (ok) {
        copyDone = true;
        setTimeout(() => { copyDone = false; }, 1500);
      }
    });
  }

  // ── Chunk navigation ──────────────────────────────────────────────────────
  // Anchors are derived from the file's hunk runs. In default mode each hunk
  // is roughly one chunk; in full-file mode the single giant hunk contains
  // many chunks (one per contiguous added/removed run).
  const chunkAnchors = $derived<ChunkAnchor[]>(computeChunkAnchors(file));
  let currentChunkIdx = $state(0);

  // ROW_HEIGHT must match VirtualHunk and DiffHunk CSS.
  const ROW_HEIGHT = 20;

  /**
   * Jump to a chunk anchor. Works for both renderers:
   *  - simple: the line is in the DOM, just scrollIntoView via data-chunk-key.
   *  - virtual: the row may be unrendered. Compute the anchor's approximate
   *    absolute scrollTop from the visual row count of preceding hunks plus
   *    the position within its own hunk, scroll there, then re-target after
   *    the next render so the row is exact.
   */
  async function scrollToAnchor(target: HTMLElement, anchor: ChunkAnchor, isSplit: boolean, smooth = true) {
    if (!file) return;
    const sel = `[data-chunk-key="${anchor.key}"]`;
    const behavior: ScrollBehavior = smooth ? 'smooth' : 'auto';

    if (!useVirtual) {
      const el = target.querySelector(sel) as HTMLElement | null;
      el?.scrollIntoView({ block: 'center', behavior });
      return;
    }

    // Estimated absolute top inside the scroll container.
    const headerH = 24; // matches `.hunk-header` padding 3px + content 18px ≈ 24px
    let top = 0;
    for (let hi = 0; hi < anchor.hunkIdx; hi++) {
      const h = file.hunks[hi];
      const visualRows = isSplit
        ? Math.max(
            h.lines.filter(l => l.kind !== 'added').length,
            h.lines.filter(l => l.kind !== 'removed').length,
          )
        : h.lines.length;
      top += headerH + visualRows * ROW_HEIGHT;
    }
    const hunk = file.hunks[anchor.hunkIdx];
    if (hunk) {
      top += headerH;
      if (isSplit) {
        let oldN = 0, newN = 0;
        for (let i = 0; i < anchor.lineIdx; i++) {
          const k = hunk.lines[i].kind;
          if (k !== 'added')   oldN++;
          if (k !== 'removed') newN++;
        }
        top += Math.max(oldN, newN) * ROW_HEIGHT;
      } else {
        top += anchor.lineIdx * ROW_HEIGHT;
      }
    }

    const targetScrollTop = Math.max(0, top - target.clientHeight / 2);
    target.scrollTo({ top: targetScrollTop, behavior });

    // After the virtualizer re-renders the now-visible window, refine the
    // alignment by scrolling the actual element into view.
    await tick();
    requestAnimationFrame(() => {
      const el = target.querySelector(sel) as HTMLElement | null;
      el?.scrollIntoView({ block: 'center', behavior: 'auto' });
    });
  }

  function goToChunk(idx: number, scope: 'main' | 'fullscreen') {
    if (chunkAnchors.length === 0) return;
    const wrapped = ((idx % chunkAnchors.length) + chunkAnchors.length) % chunkAnchors.length;
    currentChunkIdx = wrapped;
    const target = scope === 'main' ? hunksEl : fsHunksEl;
    if (!target) return;
    const isSplit = (scope === 'main' ? mode : fsMode) === 'split';
    scrollToAnchor(target, chunkAnchors[wrapped], isSplit);
  }

  function nextChunk() { goToChunk(currentChunkIdx + 1, fullscreen ? 'fullscreen' : 'main'); }
  function prevChunk() { goToChunk(currentChunkIdx - 1, fullscreen ? 'fullscreen' : 'main'); }

  $effect(() => {
    function onNext() { if (file) nextChunk(); }
    function onPrev() { if (file) prevChunk(); }
    window.addEventListener('arbor:next-chunk', onNext);
    window.addEventListener('arbor:prev-chunk', onPrev);
    return () => {
      window.removeEventListener('arbor:next-chunk', onNext);
      window.removeEventListener('arbor:prev-chunk', onPrev);
    };
  });

  // Direct keydown listener as a safety net for F3 / Shift+F3. AppShell's
  // window-level handler also routes these, but it bails on many conditions
  // (active modal, non-graph panel, plugin keybindings) and we want chunk
  // nav to "just work" whenever a diff is on screen.
  //
  // Multiple DiffViewers can be mounted simultaneously (stage panel + commit
  // detail + modals), so we also gate on `offsetParent` to skip instances
  // hidden via display:none — only the visible viewer reacts.
  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (!file) return;
      if (e.key !== 'F3') return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      // Skip viewers that are mounted but not visible (other tabs, hidden
      // panels). The fullscreen overlay always reports a non-null offsetParent
      // since it's position:fixed, so this still lets the active viewer fire.
      const root = hunksEl ?? fsHunksEl;
      if (root && root.offsetParent === null && !fullscreen) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.shiftKey) prevChunk(); else nextChunk();
    }
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  });

  // Each split column now owns its own horizontal scrollbar — see DiffHunk /
  // VirtualHunk. No shared scrollbar / scrollLeft sync needed at this level.
  let hunksEl: HTMLElement | null = $state(null);
  let fsHunksEl: HTMLElement | null = $state(null);

  // ── Imperative API ─────────────────────────────────────────────────────────
  // Mirror the reactive bits the toolbar consumes plus the action callbacks,
  // so a chromeless host (StageArea's bottom panel header) can render its
  // own <DiffToolbar> and stay in sync with what the user clicks/selects.
  $effect(() => {
    api = file ? {
      selectedCount: selectedLines.size,
      currentChunkIdx,
      copyDone,
      stageSelected,
      copyCode,
      openFullscreen,
      prevChunk,
      nextChunk,
    } : undefined;
  });

</script>

<div class="diff-viewer">
  <div class="diff-inner">
    {#if !file}
      <div class="empty">Select a file to view diff</div>
    {:else if file.is_binary && (file.image_old || file.image_new)}
      <ImageDiff {file} />
    {:else if file.is_binary}
      <div class="binary-msg">Binary file — no diff available</div>
    {:else if file.hunks.length === 0 && diffStore.pendingPaths.has(file.path)}
      <div class="empty parsing-skeleton">
        <svg class="spinner" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
        Parsing diff…
      </div>
    {:else if file.hunks.length === 0}
      <div class="empty">No changes</div>
    {:else}
      <!-- Header -->
      {#if !chromeless}
        <div class="diff-header">
          <DiffToolbar
            {file}
            {stageable}
            {staged}
            selectedCount={selectedLines.size}
            {currentChunkIdx}
            {copyDone}
            onStageSelected={stageSelected}
            onCopyCode={copyCode}
            onOpenFullscreen={openFullscreen}
            onPrevChunk={prevChunk}
            onNextChunk={nextChunk}
            {onEncodingChange}
          />
        </div>
      {/if}

      <!-- Hunks -->
      <div class="hunks" class:is-split={mode === 'split'} bind:this={hunksEl}>
        {#each file.hunks as hunk, hi (hunk.header + hi)}
          {#if useVirtual}
            <VirtualHunk
              {hunk}
              hunkIdx={hi}
              {path}
              {mode}
              {stageable}
              {staged}
              {selectedLines}
              onToggleLine={toggleLine}
              onStageHunk={stageHunk}
              scrollContainer={hunksEl}
            />
          {:else}
            <DiffHunkView
              {hunk}
              hunkIdx={hi}
              {path}
              {mode}
              {wordWrap}
              {stageable}
              {staged}
              {selectedLines}
              onToggleLine={toggleLine}
              onStageHunk={stageHunk}
            />
          {/if}
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Full-screen diff viewer — composes the shared <Modal> shell so it gets
     the standard backdrop, ESC handling, focus trap and animation for free. -->
{#if fullscreen && file}
  <Modal
    onClose={closeFullscreen}
    width="calc(100vw - 48px)"
    height="calc(100vh - 48px)"
    padBody={false}
    ariaLabel="Full-screen diff"
  >
    {#snippet header()}
      <span class="fs-path">{file.old_path ? `${file.old_path} → ` : ''}{file.path}</span>
      <div class="diff-stats">
        {#if file.stats.additions > 0}<span class="add">+{file.stats.additions}</span>{/if}
        {#if file.stats.deletions > 0}<span class="del">-{file.stats.deletions}</span>{/if}
      </div>
      {#if chunkAnchors.length > 0}
        <div class="chunk-nav" use:tooltip={`Navigate change chunks (${shortcutFor('next_chunk') ?? 'F3'} / ${shortcutFor('prev_chunk') ?? 'Shift+F3'})`}>
          <button class="expand-btn" onclick={prevChunk} use:tooltip={tooltipForAction('Previous chunk', 'prev_chunk')} aria-label="Previous chunk">
            <ChevronUp size={12} />
          </button>
          <span class="chunk-counter">{currentChunkIdx + 1}/{chunkAnchors.length}</span>
          <button class="expand-btn" onclick={nextChunk} use:tooltip={tooltipForAction('Next chunk', 'next_chunk')} aria-label="Next chunk">
            <ChevronDown size={12} />
          </button>
        </div>
      {/if}
      <div class="mode-toggle">
        <button class="mode-btn" class:active={fsMode === 'unified'} onclick={() => fsMode = 'unified'}>Unified</button>
        <button class="mode-btn" class:active={fsMode === 'split'} onclick={() => fsMode = 'split'}>Split</button>
      </div>
      <button class="mac-close-btn" onclick={closeFullscreen} use:tooltip={{ content: 'Close', shortcut: 'Esc' }} aria-label="Close"></button>
    {/snippet}

    <div class="fs-hunks" class:is-split={fsMode === 'split'} bind:this={fsHunksEl}>
      {#each file.hunks as hunk, hi (hunk.header + hi)}
        {#if useVirtual}
          <VirtualHunk
            {hunk}
            hunkIdx={hi}
            {path}
            mode={fsMode}
            scrollContainer={fsHunksEl}
          />
        {:else}
          <DiffHunkView
            {hunk}
            hunkIdx={hi}
            {path}
            mode={fsMode}
            {wordWrap}
          />
        {/if}
      {/each}
    </div>
  </Modal>
{/if}

<style>
  .diff-viewer {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-base);
    display: flex;
    flex-direction: column;
  }

  .diff-inner {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .diff-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    font-size: var(--font-size-xs);
  }

  .diff-stats { display: flex; gap: 8px; }
  .add { color: var(--success); }
  .del { color: var(--error); }

  .chunk-nav {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 4px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
  }
  .chunk-counter {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--text-muted);
    min-width: 28px;
    text-align: center;
    user-select: none;
  }

  .mode-toggle { display: flex; gap: 2px; }
  .mode-btn {
    padding: 2px 8px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    transition: all var(--transition-fast);
  }
  .mode-btn.active, .mode-btn:hover {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }

  .expand-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px; height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .expand-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .hunks {
    flex: 1;
    min-height: 0;
    overflow-x: scroll;
    overflow-y: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    position: relative;
  }
  /* In split mode the outer container only scrolls vertically — each column
     has its own native horizontal scrollbar (see DiffHunk / VirtualHunk). */
  .hunks.is-split { overflow-x: hidden; }

  .empty, .binary-msg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }
  .parsing-skeleton { gap: 8px; color: var(--accent); }
  .parsing-skeleton .spinner { animation: diff-spin 1s linear infinite; }
  @keyframes diff-spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }

  /* Header content lives inside Modal's header chrome — only typography
     and layout of the path span remains here; the bar (background,
     padding) is provided by `.modal-header` in Modal.svelte. */
  .fs-path {
    flex: 1;
    font-family: var(--font-code);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* `.fs-hunks` lives inside `.modal-body.no-pad` — fill it and own the
     scroll so the body's own `overflow: auto` never kicks in (avoids
     double scrollbars on edge cases). */
  .fs-hunks {
    width: 100%;
    height: 100%;
    overflow-x: scroll;
    overflow-y: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    position: relative;
    box-sizing: border-box;
  }
  /* Per-column horizontal scrollbars in split mode (see DiffHunk / VirtualHunk). */
  .fs-hunks.is-split { overflow-x: hidden; }
</style>
