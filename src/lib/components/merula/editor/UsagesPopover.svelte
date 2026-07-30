<script lang="ts">
  /**
   * Floating "find usages" popover (IntelliJ "Show Usages" style). Anchored at the
   * caret, driven by `usagesStore`. The floating chrome + keyboard navigation live
   * in the shared {@link FloatingPicker}; this component only supplies the header,
   * the per-usage row, and the empty-state copy. One mount in the MerulaShell.
   */
  import { SearchCode, CornerDownRight } from 'lucide-svelte';
  import FloatingPicker from './FloatingPicker.svelte';
  import { merulaStore } from '../merula-store.svelte';
  import { usagesStore, type UsageItem } from '../stores/usages.svelte';

  function jump(u: UsageItem) {
    merulaStore.requestGoto(u.offset, u.line);
    usagesStore.close();
  }
</script>

<FloatingPicker
  open={usagesStore.open}
  anchor={usagesStore.anchor}
  width={380}
  items={usagesStore.items}
  ariaLabel="Usages"
  onSelect={(u) => jump(u)}
  onClose={() => usagesStore.close()}
>
  {#snippet header()}
    <SearchCode size={12} />
    {#if usagesStore.symbol}
      <span class="up-title">Usages of <code>{usagesStore.symbol}</code></span>
      <span class="up-count">{usagesStore.count}</span>
    {:else}
      <span class="up-title">Find usages</span>
    {/if}
  {/snippet}

  {#snippet row(u: UsageItem, sel: boolean)}
    <span class="up-icon"><CornerDownRight size={12} /></span>
    <span class="up-pos">{u.line}:{u.col}</span>
    <span class="up-preview" class:on={sel}>{u.preview}</span>
  {/snippet}

  {#snippet empty()}
    {#if usagesStore.symbol === null}
      <div class="up-empty">Place the caret on a name, then press <kbd>Alt</kbd>+<kbd>F7</kbd>.</div>
    {:else}
      <div class="up-empty">No usages of <code>{usagesStore.symbol}</code> in this file.</div>
    {/if}
  {/snippet}
</FloatingPicker>

<style>
  .up-title { flex: 1; min-width: 0; font-size: var(--font-size-xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .up-title code { font-family: var(--font-code); color: var(--text-primary); }
  .up-count {
    font-size: var(--font-size-2xs); font-weight: 700; font-variant-numeric: tabular-nums;
    padding: 0 5px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-muted);
  }

  .up-empty { padding: 12px 14px; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }
  .up-empty code { font-family: var(--font-code); color: var(--text-secondary); }
  .up-empty kbd {
    font-family: var(--font-code); font-size: var(--font-size-2xs); padding: 0 4px;
    background: var(--bg-overlay); border: 1px solid var(--border); border-radius: var(--radius-sm);
  }

  .up-icon { display: flex; flex-shrink: 0; color: var(--text-disabled); }
  .up-pos {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    flex-shrink: 0; min-width: 38px; font-variant-numeric: tabular-nums;
  }
  .up-preview {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs);
    color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .up-preview.on { color: var(--text-primary); }
</style>
