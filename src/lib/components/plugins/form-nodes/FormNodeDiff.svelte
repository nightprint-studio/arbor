<!--
  FormNodeDiff — the `diff` host widget (read-only diff viewer).

  Display-only (NOT value-bearing): the node carries pre-diffed hunks supplied
  by the plugin. It reuses the app's own diff row renderers — DiffHunk and the
  virtualized VirtualHunk — so syntax highlighting (Prism), unified + split
  layouts and large-diff virtualization all come for free, without dragging in
  diffStore / partial staging / fullscreen / global keybindings.

  The unified/split toggle is local to the node (its own `$state`), not the
  app-wide diffStore, so two diff nodes in one form are independent and a
  toggle here never affects the git diff panel.

  Live updates: address the node by a stable `id` and swap `hunks` via the
  `patch` op (`merge`) — the component stays mounted, so the toggle/scroll
  position survive the update.
-->
<script lang="ts">
  import DiffHunkView from '$lib/components/corvus/diff/DiffHunk.svelte';
  import VirtualHunk   from '$lib/components/corvus/diff/VirtualHunk.svelte';
  import TypePill      from '$lib/components/shared/internal/TypePill.svelte';
  import { syntheticPathForLang } from '$lib/utils/diff-formatter';
  import type { FormNode } from '$lib/types/plugin';
  import { untrack } from 'svelte';
  import { normalizeDiffHunks, diffStats, totalLineCount } from './diff';

  interface Props {
    node: FormNode;
    ctx?: unknown; // unused — diff is display-only, but kept for renderer parity
  }
  let { node }: Props = $props();

  const n = $derived(node as any);

  const hunks      = $derived(normalizeDiffHunks(n.hunks));
  const stats      = $derived(diffStats(hunks));
  const totalLines = $derived(totalLineCount(hunks));

  // Path fed to the highlighter: an explicit `language` wins (synthesised into
  // a path the grammar resolver understands), else the real `path`.
  const hlPath = $derived(
    n.language ? syntheticPathForLang(String(n.language)) : (n.path ?? ''),
  );

  const wordWrap   = $derived(!!n.word_wrap);
  const threshold  = $derived(typeof n.virtualize_threshold === 'number' ? n.virtualize_threshold : 600);
  // Word wrap forces the simple renderer (variable row height breaks the
  // fixed-ROW_HEIGHT virtual layout — same rule as DiffViewer).
  const useVirtual = $derived(!wordWrap && totalLines > threshold);

  // Local layout toggle — defaults to the node's `mode`, then "unified".
  let mode = $state<'unified' | 'split'>(
    untrack(() => (node as any).mode === 'split' ? 'split' : 'unified'),
  );
  const showToggle = $derived(!n.hide_mode_toggle);

  const heightStyle = $derived(
    n.height == null ? '320px'
    : typeof n.height === 'number' ? `${n.height}px`
    : String(n.height),
  );

  const emptyText  = $derived(n.empty_text ?? 'No changes');
  const headerPath = $derived(
    n.old_path ? `${n.old_path} → ${n.path ?? ''}` : (n.path ?? ''),
  );
  const hasHeader  = $derived(!!(n.label || headerPath || showToggle));

  let hunksEl: HTMLElement | null = $state(null);
</script>

<div class="pf-field {n.class ?? ''}" style={n.style}>
  {#if n.label}
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="pf-label">{n.label}</label>
  {/if}

  <div class="pf-diff" style="height:{heightStyle}">
    {#if hasHeader}
      <div class="pf-diff-header">
        {#if headerPath}<span class="pf-diff-path">{headerPath}</span>{/if}
        <div class="pf-diff-stats">
          {#if stats.additions > 0}<span class="add">+{stats.additions}</span>{/if}
          {#if stats.deletions > 0}<span class="del">-{stats.deletions}</span>{/if}
        </div>
        {#if showToggle}
          <div class="pf-diff-modes">
            <button
              type="button"
              class="pf-diff-mode"
              class:active={mode === 'unified'}
              onclick={() => (mode = 'unified')}
            >Unified</button>
            <button
              type="button"
              class="pf-diff-mode"
              class:active={mode === 'split'}
              onclick={() => (mode = 'split')}
            >Split</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if hunks.length === 0}
      <div class="pf-diff-empty">{emptyText}</div>
    {:else}
      <div class="pf-diff-hunks" class:is-split={mode === 'split'} bind:this={hunksEl}>
        {#each hunks as hunk, hi (hunk.header + hi)}
          {#if useVirtual}
            <VirtualHunk
              {hunk}
              hunkIdx={hi}
              path={hlPath}
              {mode}
              scrollContainer={hunksEl}
            />
          {:else}
            <DiffHunkView
              {hunk}
              hunkIdx={hi}
              path={hlPath}
              {mode}
              {wordWrap}
            />
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  {#if n.hint}
    <span class="pf-hint">{n.hint}</span>
  {/if}
  {#if n.pill}
    <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
  {/if}
</div>

<style>
  .pf-diff {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md, 6px);
    overflow: hidden;
    background: var(--bg-base);
  }

  .pf-diff-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 10px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    font-size: var(--font-size-xs);
  }
  .pf-diff-path {
    flex: 1;
    min-width: 0;
    font-family: var(--font-code);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pf-diff-stats { display: flex; gap: 8px; flex-shrink: 0; }
  .pf-diff-stats .add { color: var(--success); }
  .pf-diff-stats .del { color: var(--error); }

  .pf-diff-modes { display: flex; gap: 2px; flex-shrink: 0; }
  .pf-diff-mode {
    padding: 2px 8px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .pf-diff-mode.active,
  .pf-diff-mode:hover {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: var(--accent);
  }

  .pf-diff-hunks {
    flex: 1;
    min-height: 0;
    overflow-x: scroll;
    overflow-y: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    position: relative;
  }
  /* In split mode each column owns its horizontal scrollbar (see DiffHunk). */
  .pf-diff-hunks.is-split { overflow-x: hidden; }

  .pf-diff-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }
</style>
