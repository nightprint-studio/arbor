<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
  import type { DiffHunk, DiffLine } from '$lib/types/git';
  import { lineKey } from '$lib/utils/patch-builder';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    hunk,
    hunkIdx = 0,
    path = '',
    mode = 'split',
    stageable = false,
    staged = false,
    selectedLines = new Set<string>(),
    onToggleLine,
    onStageHunk,
    scrollContainer = null as HTMLElement | null,
  }: {
    hunk: DiffHunk;
    hunkIdx?: number;
    path?: string;
    mode?: 'unified' | 'split' | 'word_diff';
    stageable?: boolean;
    staged?: boolean;
    selectedLines?: Set<string>;
    onToggleLine?: (key: string) => void;
    onStageHunk?: (hunkIdx: number) => void;
    scrollContainer?: HTMLElement | null;
  } = $props();

  // Row height must match the CSS `.line { min-height: 20px; line-height: 20px }`.
  // We render a top spacer + visible rows in normal flow + bottom spacer so
  // line widths still drive the parent's max-content (= correct horizontal
  // scrollbar) while avoiding absolute positioning's width-collapse trap.
  const ROW_HEIGHT = 20;
  const BUFFER_ROWS = 16;

  const actionLabel = $derived(staged ? 'Unstage Hunk' : 'Stage Hunk');

  // Filtered lines per column (split mode); preserve original index in `idx`
  // so `lineKey()` and selection logic remain identical to DiffHunk.
  const oldLines = $derived.by(() => {
    const out: { line: DiffLine; idx: number }[] = [];
    for (let i = 0; i < hunk.lines.length; i++) {
      if (hunk.lines[i].kind !== 'added') out.push({ line: hunk.lines[i], idx: i });
    }
    return out;
  });
  const newLines = $derived.by(() => {
    const out: { line: DiffLine; idx: number }[] = [];
    for (let i = 0; i < hunk.lines.length; i++) {
      if (hunk.lines[i].kind !== 'removed') out.push({ line: hunk.lines[i], idx: i });
    }
    return out;
  });

  const unifiedCount = $derived(hunk.lines.length);
  const oldCount     = $derived(oldLines.length);
  const newCount     = $derived(newLines.length);

  // ── Scroll-driven window ──────────────────────────────────────────────────

  let hunkEl: HTMLElement | null = $state(null);
  let headerEl: HTMLElement | null = $state(null);
  let firstRow = $state(0);
  let lastRow  = $state(80);
  let inViewport = $state(true);

  function recompute() {
    if (!hunkEl || !scrollContainer) return;
    const cRect = scrollContainer.getBoundingClientRect();
    const hRect = hunkEl.getBoundingClientRect();

    const top    = hRect.top - cRect.top;
    const bottom = top + hRect.height;
    inViewport = bottom > 0 && top < cRect.height;
    if (!inViewport) {
      firstRow = 0;
      lastRow  = 0;
      return;
    }

    const headerH = headerEl?.offsetHeight ?? 0;
    const rowsTop = top + headerH;

    const visibleTop    = Math.max(0, -rowsTop);
    const visibleBottom = Math.max(0, cRect.height - rowsTop);

    const maxCount = mode === 'split' ? Math.max(oldCount, newCount) : unifiedCount;
    firstRow = Math.max(0, Math.floor(visibleTop / ROW_HEIGHT) - BUFFER_ROWS);
    lastRow  = Math.min(maxCount, Math.ceil(visibleBottom / ROW_HEIGHT) + BUFFER_ROWS);
  }

  let rafPending = false;
  function onScroll() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      recompute();
    });
  }

  $effect(() => {
    void unifiedCount; void oldCount; void newCount; void mode;
    if (!scrollContainer) return;
    scrollContainer.addEventListener('scroll', onScroll, { passive: true });
    const ro = new ResizeObserver(onScroll);
    ro.observe(scrollContainer);
    if (hunkEl) ro.observe(hunkEl);
    recompute();
    return () => {
      scrollContainer.removeEventListener('scroll', onScroll);
      ro.disconnect();
    };
  });

  // ── Selection helpers ─────────────────────────────────────────────────────
  function isChangeable(line: DiffLine): boolean { return line.kind !== 'context'; }
  function isSelected(lineIdx: number): boolean {
    return selectedLines.has(lineKey(hunkIdx, lineIdx));
  }
  function toggleLine(lineIdx: number) {
    if (!stageable || !onToggleLine) return;
    onToggleLine(lineKey(hunkIdx, lineIdx));
  }

  // Each split column has its own native horizontal scrollbar (overflow-x:auto).

  // ── Visible row slices ────────────────────────────────────────────────────
  // Lower-clamp lo/hi independently per column because the column counts
  // differ in split mode (filtered).
  const unifiedSlice = $derived.by(() => {
    if (!inViewport) return { lines: [] as { line: DiffLine; idx: number }[], padTop: 0, padBottom: unifiedCount * ROW_HEIGHT };
    const lo = Math.max(0, Math.min(firstRow, unifiedCount));
    const hi = Math.max(lo, Math.min(lastRow,  unifiedCount));
    const slice: { line: DiffLine; idx: number }[] = [];
    for (let i = lo; i < hi; i++) slice.push({ line: hunk.lines[i], idx: i });
    return { lines: slice, padTop: lo * ROW_HEIGHT, padBottom: (unifiedCount - hi) * ROW_HEIGHT };
  });
  const oldSlice = $derived.by(() => {
    if (!inViewport) return { lines: [] as { line: DiffLine; idx: number }[], padTop: 0, padBottom: oldCount * ROW_HEIGHT };
    const lo = Math.max(0, Math.min(firstRow, oldCount));
    const hi = Math.max(lo, Math.min(lastRow,  oldCount));
    return { lines: oldLines.slice(lo, hi), padTop: lo * ROW_HEIGHT, padBottom: (oldCount - hi) * ROW_HEIGHT };
  });
  const newSlice = $derived.by(() => {
    if (!inViewport) return { lines: [] as { line: DiffLine; idx: number }[], padTop: 0, padBottom: newCount * ROW_HEIGHT };
    const lo = Math.max(0, Math.min(firstRow, newCount));
    const hi = Math.max(lo, Math.min(lastRow,  newCount));
    return { lines: newLines.slice(lo, hi), padTop: lo * ROW_HEIGHT, padBottom: (newCount - hi) * ROW_HEIGHT };
  });
</script>

<div class="hunk virt-hunk" class:is-split={mode === 'split'} bind:this={hunkEl}>
  <div class="hunk-header" bind:this={headerEl}>
    <span class="hunk-range">{hunk.header.trim()}</span>
    {#if stageable}
      <button class="hunk-action-btn" onclick={() => onStageHunk?.(hunkIdx)} use:tooltip={actionLabel}>
        {actionLabel}
      </button>
    {/if}
  </div>

  {#if mode === 'split'}
    <div class="split-pair">
      <div class="split-col split-old-col">
        <div class="split-col-inner">
          {#if oldSlice.padTop > 0}<div class="virt-spacer" style="height: {oldSlice.padTop}px;"></div>{/if}
          {#each oldSlice.lines as { line, idx: li } (li)}
            {@const changeable = isChangeable(line)}
            {@const sel = isSelected(li)}
            {#snippet oldLineInner()}
              {#if stageable && changeable}
                <span class="sel-gutter" class:sel-active={sel}></span>
              {/if}
              <span class="lineno">{line.old_lineno ?? ''}</span>
              <span class="prefix">{line.kind === 'removed' ? (staged ? '+' : '-') : ' '}</span>
              <!-- eslint-disable-next-line svelte/no-at-html-tags -->
              <span class="content">{@html highlight(line.content.replace(/\n$/, ''), path)}</span>
            {/snippet}
            {#if stageable && changeable}
              <div
                class="line line-{line.kind} line-selectable"
                class:line-selected={sel}
                data-chunk-key={lineKey(hunkIdx, li)}
                onclick={() => toggleLine(li)}
                role="checkbox"
                aria-checked={sel}
                tabindex="0"
                onkeydown={(e) => e.key === ' ' && toggleLine(li)}
              >
                {@render oldLineInner()}
              </div>
            {:else}
              <div class="line line-{line.kind}" data-chunk-key={lineKey(hunkIdx, li)}>
                {@render oldLineInner()}
              </div>
            {/if}
          {/each}
          {#if oldSlice.padBottom > 0}<div class="virt-spacer" style="height: {oldSlice.padBottom}px;"></div>{/if}
        </div>
      </div>

      <div class="split-col split-new-col">
        <div class="split-col-inner">
          {#if newSlice.padTop > 0}<div class="virt-spacer" style="height: {newSlice.padTop}px;"></div>{/if}
          {#each newSlice.lines as { line, idx: li } (li)}
            {@const changeable = isChangeable(line)}
            {@const sel = isSelected(li)}
            {#snippet newLineInner()}
              {#if stageable && changeable}
                <span class="sel-gutter" class:sel-active={sel}></span>
              {/if}
              <span class="lineno">{line.new_lineno ?? ''}</span>
              <span class="prefix">{line.kind === 'added' ? (staged ? '-' : '+') : ' '}</span>
              <!-- eslint-disable-next-line svelte/no-at-html-tags -->
              <span class="content">{@html highlight(line.content.replace(/\n$/, ''), path)}</span>
            {/snippet}
            {#if stageable && changeable}
              <div
                class="line line-{line.kind} line-selectable"
                class:line-selected={sel}
                data-chunk-key={lineKey(hunkIdx, li)}
                onclick={() => toggleLine(li)}
                role="checkbox"
                aria-checked={sel}
                tabindex="0"
                onkeydown={(e) => e.key === ' ' && toggleLine(li)}
              >
                {@render newLineInner()}
              </div>
            {:else}
              <div class="line line-{line.kind}" data-chunk-key={lineKey(hunkIdx, li)}>
                {@render newLineInner()}
              </div>
            {/if}
          {/each}
          {#if newSlice.padBottom > 0}<div class="virt-spacer" style="height: {newSlice.padBottom}px;"></div>{/if}
        </div>
      </div>
    </div>

  {:else}
    {#if unifiedSlice.padTop > 0}<div class="virt-spacer" style="height: {unifiedSlice.padTop}px;"></div>{/if}
    {#each unifiedSlice.lines as { line, idx: li } (li)}
      {@const changeable = isChangeable(line)}
      {@const sel = isSelected(li)}
      {#snippet unifiedLineInner()}
        {#if stageable && changeable}
          <span class="sel-gutter" class:sel-active={sel}></span>
        {/if}
        <span class="lineno">{line.old_lineno ?? ' '}</span>
        <span class="lineno">{line.new_lineno ?? ' '}</span>
        <span class="prefix">{line.kind === 'added' ? (staged ? '-' : '+') : line.kind === 'removed' ? (staged ? '+' : '-') : ' '}</span>
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        <span class="content">{@html highlight(line.content.replace(/\n$/, ''), path)}</span>
      {/snippet}
      {#if stageable && changeable}
        <div
          class="line line-{line.kind} line-selectable"
          class:line-selected={sel}
          data-chunk-key={lineKey(hunkIdx, li)}
          onclick={() => toggleLine(li)}
          role="checkbox"
          aria-checked={sel}
          tabindex="0"
          onkeydown={(e) => e.key === ' ' && toggleLine(li)}
        >
          {@render unifiedLineInner()}
        </div>
      {:else}
        <div class="line line-{line.kind}" data-chunk-key={lineKey(hunkIdx, li)}>
          {@render unifiedLineInner()}
        </div>
      {/if}
    {/each}
    {#if unifiedSlice.padBottom > 0}<div class="virt-spacer" style="height: {unifiedSlice.padBottom}px;"></div>{/if}
  {/if}
</div>

<style>
  .hunk {
    border-bottom: 1px solid var(--border-subtle);
    width: max-content;
    min-width: 100%;
  }
  .hunk.is-split { width: 100%; min-width: 0; }

  .hunk-header {
    display: flex;
    align-items: center;
    padding: 3px 8px;
    background: rgba(77, 120, 204, 0.08);
    border-top: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
  }
  .hunk-range {
    flex: 1;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    font-family: var(--font-code);
  }
  .hunk-action-btn {
    flex-shrink: 0;
    padding: 1px 8px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 10px;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .hunk-action-btn:hover {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }

  .virt-spacer {
    width: 100%;
    flex-shrink: 0;
  }

  /* ── Split layout (mirrors DiffHunk.svelte) ──────────────────────────── */
  .split-pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    border-top: 1px solid var(--border-subtle);
  }
  .split-col {
    overflow-x: auto;
    overflow-y: hidden;
    min-width: 0;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }
  .split-col::-webkit-scrollbar       { height: 10px; }
  .split-col::-webkit-scrollbar-track { background: transparent; }
  .split-col::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: var(--radius-sm);
  }
  .split-col::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }
  .split-old-col { border-right: 1px solid var(--border-subtle); }
  .split-col-inner {
    width: max-content;
    min-width: 100%;
  }

  /* ── Lines (mirror DiffHunk.svelte) ──────────────────────────────────── */
  .line {
    display: flex;
    align-items: flex-start;
    min-height: 20px;
    line-height: 20px;
    width: 100%;
    min-width: max-content;
    transition: background var(--transition-fast);
  }
  .line-added   { background: var(--diff-add-bg); }
  .line-removed { background: var(--diff-del-bg); }
  .line-selectable { cursor: pointer; }
  .line-selectable:hover { filter: brightness(1.18); }
  .line-selected.line-added   { background: var(--diff-add-bg-strong, rgba(95,173,86,0.28)); }
  .line-selected.line-removed { background: var(--diff-del-bg-strong, rgba(199,84,80,0.28)); }

  .sel-gutter {
    width: 6px;
    min-width: 6px;
    align-self: stretch;
    margin-right: 2px;
    border-radius: 0 2px 2px 0;
    background: transparent;
    transition: background var(--transition-fast);
  }
  .sel-gutter.sel-active { background: var(--accent); }
  .line-selectable:hover .sel-gutter:not(.sel-active) { background: var(--border); }

  .lineno {
    width: 44px;
    min-width: 44px;
    text-align: right;
    padding-right: 8px;
    color: var(--text-disabled);
    font-size: 11px;
    user-select: none;
    flex-shrink: 0;
    border-right: 1px solid var(--border-subtle);
  }
  .prefix {
    width: 18px;
    text-align: center;
    flex-shrink: 0;
    color: var(--text-muted);
    user-select: none;
  }
  .line-added .prefix   { color: var(--success); }
  .line-removed .prefix { color: var(--error); }
  .content {
    flex: 1;
    white-space: pre;
    min-width: 0;
    padding-left: 4px;
    font-size: var(--font-size-sm);
    user-select: text;
    cursor: text;
  }
</style>
