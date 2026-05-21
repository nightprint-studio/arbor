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
    wordWrap = false,
    stageable = false,
    staged = false,
    selectedLines = new Set<string>(),
    onToggleLine,
    onStageHunk,
  }: {
    hunk: DiffHunk;
    hunkIdx?: number;
    path?: string;
    mode?: 'unified' | 'split' | 'word_diff';
    wordWrap?: boolean;
    stageable?: boolean;
    staged?: boolean;
    selectedLines?: Set<string>;
    onToggleLine?: (key: string) => void;
    onStageHunk?: (hunkIdx: number) => void;
  } = $props();

  const actionLabel = $derived(staged ? 'Unstage Hunk' : 'Stage Hunk');

  // Pre-computed per-column line lists with their original index in hunk.lines.
  // Without this the template would allocate a new filtered array per render
  // AND call `hunk.lines.indexOf(line)` per row — O(N²) per re-render, and
  // re-renders fire on every `selectedLines` change (every line toggle).
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

  function isChangeable(line: DiffLine): boolean {
    return line.kind !== 'context';
  }

  function isSelected(lineIdx: number): boolean {
    return selectedLines.has(lineKey(hunkIdx, lineIdx));
  }

  function toggleLine(lineIdx: number) {
    if (!stageable || !onToggleLine) return;
    onToggleLine(lineKey(hunkIdx, lineIdx));
  }

  // Each split column owns its own native horizontal scrollbar
  // (overflow-x: auto). No JS-driven scrollLeft sync needed.
</script>

<div class="hunk" class:word-wrap={wordWrap} class:is-split={mode === 'split'}>
  <!-- Hunk header -->
  <div class="hunk-header">
    <span class="hunk-range">{hunk.header.trim()}</span>
    {#if stageable}
      <button
        class="hunk-action-btn"
        onclick={() => onStageHunk?.(hunkIdx)}
        use:tooltip={actionLabel}
      >
        {actionLabel}
      </button>
    {/if}
  </div>

  {#if mode === 'split'}
    <!--
      Two fixed-width columns (grid 1fr 1fr → always 50% each).
      Each column has its own native horizontal scrollbar (overflow-x: auto).
      `.split-col-inner` uses width:max-content so the column reports the full
      content width; lines use width:100% so their backgrounds always extend.
    -->
    <div class="split-pair">
      <div class="split-col split-old-col">
        <div class="split-col-inner">
          {#each oldLines as { line, idx: li }}
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
        </div>
      </div>

      <div class="split-col split-new-col">
        <div class="split-col-inner">
          {#each newLines as { line, idx: li }}
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
        </div>
      </div>
    </div>

  {:else}
    <!-- Unified / word-diff -->
    {#each hunk.lines as line, li}
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
  {/if}
</div>

<style>
  /* Unified mode: hunk grows to the widest line so short-line backgrounds extend fully. */
  .hunk {
    border-bottom: 1px solid var(--border-subtle);
    width: max-content;
    min-width: 100%;
  }
  /* Split mode: hunk fills the container (columns handle their own width). */
  .hunk.is-split {
    width: 100%;
    min-width: 0;
  }

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

  /* ── Split layout ────────────────────────────────────────────────────────── */

  /* Grid gives both columns identical width (1fr each = always 50/50).
     Grid also stretches both to the same height even if line counts differ. */
  .split-pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    border-top: 1px solid var(--border-subtle);
  }

  /* Each column owns its own native horizontal scrollbar. No JS sync. */
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

  .split-old-col {
    border-right: 1px solid var(--border-subtle);
  }

  /* width:max-content is an intrinsic size — it ignores the containing block's
     available width, so it isn't constrained by the overflow:hidden column.
     min-width:100% ensures it covers the full column when lines are short.
     Lines use width:100% so every line fills the inner div and backgrounds extend. */
  .split-col-inner {
    width: max-content;
    min-width: 100%;
  }

  .word-wrap .split-col-inner { width: auto; }

  /* ── Lines ───────────────────────────────────────────────────────────────── */

  .line {
    display: flex;
    align-items: flex-start;
    min-height: 20px;
    line-height: 20px;
    width: 100%;          /* fill the inner div so backgrounds always extend */
    min-width: max-content; /* but never narrower than the line's own content */
    transition: background var(--transition-fast);
  }

  .line-added   { background: var(--diff-add-bg); }
  .line-removed { background: var(--diff-del-bg); }

  /* Selectable lines get a pointer cursor and brighten on hover */
  .line-selectable { cursor: pointer; }
  .line-selectable:hover { filter: brightness(1.18); }

  /* Selected lines get a more vivid background */
  .line-selected.line-added   { background: var(--diff-add-bg-strong, rgba(95,173,86,0.28)); }
  .line-selected.line-removed { background: var(--diff-del-bg-strong, rgba(199,84,80,0.28)); }

  /* Selection gutter indicator (left edge dot) */
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
  .word-wrap .content {
    white-space: pre-wrap;
    word-break: break-all;
    overflow-wrap: anywhere;
  }
</style>
