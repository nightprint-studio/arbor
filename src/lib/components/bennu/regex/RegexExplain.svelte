<script lang="ts">
  /**
   * The pattern read aloud, token by token — see `explain.ts`. Kept in step with the pattern field:
   * hovering a row emphasises its token there, and the row the pattern's caret is in is marked.
   */
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { ExplainRow } from './explain';

  let { rows, caret = null, onhover }: {
    rows: ExplainRow[];
    /** The caret offset in the pattern field. */
    caret?: number | null;
    onhover?: (range: { from: number; to: number } | null) => void;
  } = $props();

  /** The innermost row the caret is in: the narrowest span that contains it. */
  const current = $derived.by(() => {
    if (caret === null) return -1;
    let best = -1;
    let width = Infinity;
    rows.forEach((r, i) => {
      if (caret >= r.from && caret <= r.to && r.to - r.from <= width) { best = i; width = r.to - r.from; }
    });
    return best;
  });
</script>

{#if rows.length === 0}
  <EmptyState message="The explanation appears here as you type a pattern." />
{:else}
  <ul class="ex">
    {#each rows as r, i (i)}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <li class="ex-row" class:current={i === current} style:padding-left="{6 + r.depth * 14}px"
        onmouseenter={() => onhover?.({ from: r.from, to: r.to })} onmouseleave={() => onhover?.(null)}>
        <code class="ex-token">{r.token}</code>
        <span class="ex-text">{r.text}</span>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .ex { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 1px; }
  .ex-row { display: flex; gap: 10px; align-items: baseline; padding: 3px 6px; border-radius: var(--radius-sm); }
  .ex-row:hover { background: var(--bg-hover); }
  .ex-row.current { background: color-mix(in srgb, var(--accent) 16%, transparent); }
  .ex-token {
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    color: var(--syntax-keyword, var(--accent));
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    padding: 0 5px;
    flex-shrink: 0;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: pre;
  }
  .ex-text { font-size: var(--font-size-sm); color: var(--text-secondary); min-width: 0; }
</style>
