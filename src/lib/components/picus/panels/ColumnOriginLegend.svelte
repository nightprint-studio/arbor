<script lang="ts">
  /**
   * Which tables a result's columns came from, and where each one is.
   *
   * ## It is a control, not a caption
   *
   * A legend you can only read is worth one glance: you learn that green is `enti`
   * and then you are back to counting columns. Picking a chip **dims every column
   * that is not that table's**, which turns the legend into the answer to the
   * question that actually gets asked — *where are that table's columns?* — in a
   * result forty columns wide that scrolls sideways.
   *
   * Dimming rather than filtering, and the header rather than the body: see
   * `DataGridColumn.muted` and `.accent` for why the cells are left alone.
   *
   * ## It is absent far more often than present
   *
   * The parent renders this only when a result has two or more source tables, which
   * most do not. See `column-origins.ts`.
   */
  import ChipBar, { type ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import FloatingBar from '$lib/components/shared/ui/FloatingBar.svelte';
  import type { OriginGroup } from './column-origins';

  interface Props {
    groups: OriginGroup[];
    /** The table being highlighted, or `null` when all are shown equally. */
    selected: string | null;
    /**
     * These groups were **deduced** from the views' SQL rather than reported by the
     * server. Said in the lead-in, because the chips look identical either way and
     * the difference decides whether you may trust the name to write to.
     */
    provisional?: boolean;
    onSelect: (table: string | null) => void;
  }

  let { groups, selected, provisional = false, onSelect }: Props = $props();

  const items = $derived<ChipItem[]>(
    groups.map((g) => ({
      id: g.table,
      label: g.table,
      count: g.columns.length,
      color: g.color,
      tooltip: `${g.columns.length} column${g.columns.length === 1 ? '' : 's'} from ${g.table} — click to pick them out`,
    })),
  );
</script>

<FloatingBar gap={8}>
  <span class="col-lead" class:col-deduced={provisional}>
    {provisional ? 'Traced to' : 'Columns from'}
  </span>
  <ChipBar
    {items}
    selected={selected ?? ''}
    size="sm"
    tintCount={false}
    ariaLabel="Source tables"
    onSelect={(id) => {
      // ChipBar's single-select always reports the chip that was pressed, so
      // "press the active one to clear it" is the consumer's to implement. Without
      // it there would be no way back to seeing every column at full strength
      // except reloading the result.
      const picked = Array.isArray(id) ? id[0] : id;
      onSelect(picked === selected ? null : picked);
    }}
  />
</FloatingBar>

<style>
  /* The surface — inset, rounded, no border — is `FloatingBar`'s, not this file's.
     What is left here is only what makes this strip a *legend*. */
  .col-lead {
    flex-shrink: 0;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  /* The same warning colour the lineage panel's band uses, so "this is deduced"
     looks the same wherever it is said. */
  .col-deduced { color: var(--warning); }
</style>
