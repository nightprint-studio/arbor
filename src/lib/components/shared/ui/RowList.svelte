<script lang="ts" generics="T">
  /**
   * A plain list of rows separated by hairlines — a settings page's list of things, each row a line of
   * text and a cluster of actions.
   *
   * What is in a row is the caller's; the rhythm of the rows is this, so two lists on two pages cannot
   * drift a pixel apart.
   */
  import type { Snippet } from 'svelte';

  let {
    items,
    key,
    row,
    dimmed,
  }: {
    items: readonly T[];
    key: (item: T) => string;
    row: Snippet<[T, number]>;
    /** A row that is there but not in effect — switched off, replaced. */
    dimmed?: (item: T) => boolean;
  } = $props();
</script>

<ul class="rl">
  {#each items as item, index (key(item))}
    <li class="rl-row" class:dimmed={dimmed?.(item) ?? false}>{@render row(item, index)}</li>
  {/each}
</ul>

<style>
  .rl { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .rl-row {
    display: flex; align-items: center; gap: 10px; min-width: 0;
    padding: 6px 4px;
    border-top: 1px solid var(--border-subtle);
  }
  .rl-row:first-child { border-top: none; }
  .rl-row.dimmed > :global(:not(.rl-keep)) { opacity: 0.5; }
</style>
