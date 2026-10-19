<script lang="ts">
  /**
   * The thin caption bar over a pane: what it is, how many, and anything that acts on it.
   *
   * `title` is the plain case; `children` replaces it when the caption is richer than text (a
   * file name and what it is compared to). `count` and `loading` sit after it; `end` holds the
   * pane's own controls and is pushed to the right edge as one group, so a single auto margin
   * does the job whatever is currently inside it.
   */
  import type { Snippet } from 'svelte';
  import Spinner from './Spinner.svelte';

  let {
    title,
    count,
    loading = false,
    children,
    end,
  }: {
    title?: string;
    count?: number | string;
    loading?: boolean;
    children?: Snippet;
    end?: Snippet;
  } = $props();
</script>

<div class="ph">
  {#if children}{@render children()}{:else if title}{title}{/if}
  {#if loading}<Spinner size={11} />{/if}
  {#if end}
    <div class="ph-end">{@render end()}</div>
  {:else if count !== undefined}
    <span class="ph-count">{count}</span>
  {/if}
</div>

<style>
  .ph {
    display: flex; align-items: center; gap: 8px; flex: none;
    height: 26px; padding: 0 10px;
    font-size: var(--font-size-2xs); letter-spacing: 0.05em; text-transform: uppercase;
    color: var(--text-faint); border-bottom: 1px solid var(--border-subtle);
  }
  .ph-count { margin-left: auto; text-transform: none; letter-spacing: 0; }
  .ph-end { margin-left: auto; display: flex; align-items: center; gap: 8px;
            text-transform: none; letter-spacing: 0; }
</style>
