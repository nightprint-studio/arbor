<script lang="ts">
  /**
   * DiffStat — what a change adds and removes: `+12 −3`, and a row of five blocks split in the same proportion,
   * so a list of files reads at a glance as "mostly new" or "mostly gone" without comparing numbers.
   *
   * The blocks are a proportion, not a size: a one-line change and a thousand-line one both fill five. Blocks are
   * rounded so a side that changed at all shows at least one. `blocks={0}` draws the numbers alone.
   */
  let {
    added,
    removed,
    blocks = 5,
    size = 'sm',
  }: {
    added: number;
    removed: number;
    blocks?: number;
    size?: 'sm' | 'md';
  } = $props();

  const cells = $derived.by(() => {
    const total = added + removed;
    if (blocks <= 0) return [] as ('add' | 'del' | 'none')[];
    if (total === 0) return Array<'none'>(blocks).fill('none');
    let add = Math.round((added / total) * blocks);
    if (added > 0 && add === 0) add = 1;
    if (removed > 0 && add === blocks) add = blocks - 1;
    return Array.from({ length: blocks }, (_, i) => (i < add ? 'add' : 'del'));
  });
</script>

<span class="ds ds-{size}" aria-label={`${added} added, ${removed} removed`}>
  <span class="add">+{added}</span>
  <span class="del">−{removed}</span>
  {#if cells.length}
    <span class="cells" aria-hidden="true">{#each cells as c, i (i)}<i class={c}></i>{/each}</span>
  {/if}
</span>

<style>
  .ds {
    display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0;
    font-family: var(--font-code); font-variant-numeric: tabular-nums; font-weight: 600;
  }
  .ds-sm { font-size: var(--font-size-2xs); }
  .ds-md { font-size: var(--font-size-xs); }
  .add { color: var(--success); }
  .del { color: var(--error); }
  .cells { display: inline-flex; gap: 2px; margin-left: 1px; }
  .cells i { width: 7px; height: 7px; border-radius: 2px; }
  .cells .add { background: var(--success); }
  .cells .del { background: var(--error); }
  .cells .none { background: var(--bg-overlay); }
</style>
