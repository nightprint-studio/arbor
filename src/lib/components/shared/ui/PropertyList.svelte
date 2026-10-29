<script module lang="ts">
  export interface PropertyRow {
    label: string;
    /** Plain text value. Ignored when `custom` is given. */
    value?: string;
    /** A row that is only drawn when it has something to say. */
    hidden?: boolean;
    /** Render the value with the `custom` snippet instead of text (the row's key is passed back). */
    custom?: string;
    /** Break anywhere: for paths and other values without spaces. */
    wrap?: boolean;
  }
</script>

<script lang="ts">
  /**
   * PropertyList — label / value rows, the way an information sheet lists them (a file's properties, an
   * entry's details). App-agnostic: pass `rows`, or give a row a `custom` key and a snippet when the value is
   * more than text (a button, a spinner, a badge). Long values (a path) wrap rather than overflow, and can be
   * selected.
   */
  import type { Snippet } from 'svelte';

  let { rows, custom }: { rows: PropertyRow[]; custom?: Snippet<[string]> } = $props();

  const shown = $derived(rows.filter((r) => !r.hidden));
</script>

<dl class="pl">
  {#each shown as r (r.label)}
    <div class="pl-row">
      <dt>{r.label}</dt>
      <dd class:wrap={r.wrap}>
        {#if r.custom && custom}{@render custom(r.custom)}{:else}{r.value ?? ''}{/if}
      </dd>
    </div>
  {/each}
</dl>

<style>
  .pl { margin: 0; display: flex; flex-direction: column; gap: 2px; }
  .pl-row {
    display: grid; grid-template-columns: 110px minmax(0, 1fr); gap: 12px; align-items: baseline;
    padding: 6px 8px; border-radius: var(--radius-sm);
  }
  .pl-row:hover { background: var(--bg-hover); }
  dt { font-size: var(--font-size-xs); color: var(--text-muted); }
  dd { margin: 0; font-size: var(--font-size-sm); color: var(--text-primary); user-select: text; min-width: 0; }
  dd.wrap { overflow-wrap: anywhere; font-family: var(--font-code); font-size: var(--font-size-xs); }
</style>
