<script lang="ts">
  /**
   * Pagination — page controls for a list or grid that is fetched in slices.
   *
   * Shows where you are before what you can press: "1–100 of 4,210" answers the
   * question people actually have, and the buttons follow. The page size is part
   * of the control rather than buried in settings, because the right size
   * depends on what you are looking at right now.
   *
   * Keyboard: everything is a button, so Tab reaches it; the host can also wire
   * PageUp/PageDown to `onPage`.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no stores — numbers in,
   * intent out.
   */
  import { ChevronFirst, ChevronLast, ChevronLeft, ChevronRight } from 'lucide-svelte';
  import Select from './Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** 1-based current page. */
    page: number;
    pageSize: number;
    /** Total rows across every page. */
    total: number;
    pageSizes?: number[];
    onPage: (page: number) => void;
    onPageSize?: (size: number) => void;
    /** Right-hand free content (row counts, timings, actions). */
    trailing?: import('svelte').Snippet;
    /** Noun used in the summary — "rows", "records", "files". */
    unit?: string;
    /** Dim the controls while a fetch is in flight. */
    busy?: boolean;
  }

  let {
    page,
    pageSize,
    total,
    pageSizes = [50, 100, 250, 500, 1000],
    onPage,
    onPageSize,
    trailing,
    unit = 'rows',
    busy = false,
  }: Props = $props();

  const pageCount = $derived(Math.max(1, Math.ceil(total / pageSize)));
  const clamped = $derived(Math.min(Math.max(1, page), pageCount));
  const first = $derived(total === 0 ? 0 : (clamped - 1) * pageSize + 1);
  const last = $derived(Math.min(total, clamped * pageSize));

  const sizeOptions = $derived(pageSizes.map((n) => ({ value: String(n), label: `${n} / page` })));

  const atStart = $derived(clamped <= 1);
  const atEnd = $derived(clamped >= pageCount);
</script>

<div class="pg" class:pg-busy={busy}>
  <span class="pg-summary">
    {#if total === 0}
      No {unit}
    {:else}
      <strong>{first.toLocaleString()}–{last.toLocaleString()}</strong>
      of {total.toLocaleString()}
      {unit}
    {/if}
  </span>

  <div class="pg-nav" role="group" aria-label="Pagination">
    <button
      type="button" class="pg-btn" disabled={atStart || busy}
      use:tooltip={'First page'} aria-label="First page"
      onclick={() => onPage(1)}
    ><ChevronFirst size={13} /></button>
    <button
      type="button" class="pg-btn" disabled={atStart || busy}
      use:tooltip={'Previous page'} aria-label="Previous page"
      onclick={() => onPage(clamped - 1)}
    ><ChevronLeft size={13} /></button>

    <span class="pg-pos">
      page <strong>{clamped.toLocaleString()}</strong> of {pageCount.toLocaleString()}
    </span>

    <button
      type="button" class="pg-btn" disabled={atEnd || busy}
      use:tooltip={'Next page'} aria-label="Next page"
      onclick={() => onPage(clamped + 1)}
    ><ChevronRight size={13} /></button>
    <button
      type="button" class="pg-btn" disabled={atEnd || busy}
      use:tooltip={'Last page'} aria-label="Last page"
      onclick={() => onPage(pageCount)}
    ><ChevronLast size={13} /></button>
  </div>

  {#if onPageSize}
    <Select
      value={String(pageSize)}
      options={sizeOptions}
      narrow
      disabled={busy}
      onchange={(v) => onPageSize?.(Number(v))}
    />
  {/if}

  <span class="pg-spacer"></span>
  {#if trailing}{@render trailing()}{/if}
</div>

<style>
  .pg {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    height: 28px;
    padding: 0 8px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .pg-busy { opacity: 0.6; }

  .pg-summary strong,
  .pg-pos strong {
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }

  .pg-nav { display: flex; align-items: center; gap: 2px; }
  .pg-pos { padding: 0 6px; font-variant-numeric: tabular-nums; }

  .pg-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
  }
  .pg-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .pg-btn:disabled { opacity: 0.35; cursor: default; }

  .pg-spacer { flex: 1; }
</style>
