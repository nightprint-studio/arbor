<script lang="ts">
  /**
   * One cell's value, rendered the way a data grid must render it.
   *
   * Two rules, and they exist because getting them wrong is expensive rather than
   * ugly:
   *
   *  • **NULL is not the empty string.** A null reads `NULL` in muted italics; an
   *    empty string shows a thin placeholder box. Anybody writing DML from what
   *    they see needs those to be distinguishable at a glance, and no amount of
   *    care downstream recovers from having conflated them on screen.
   *  • **A value that has not arrived is neither.** A windowed grid draws a quiet
   *    bar for a row still in flight — absent and not-yet-here are different facts.
   *
   * Its own component so {@link DataGrid}'s default and any consumer that overrides
   * the `cell` snippet render identically. A Picus result grid that adds an editor
   * and a mask on top of the value must not also re-decide what a null looks like.
   */
  import type { DataGridValue } from './DataGrid.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    value: DataGridValue;
    /** The row has not been fetched yet — draws the waiting bar. */
    loading?: boolean;
  }

  let { value, loading = false }: Props = $props();
</script>

{#if loading}
  <span class="dcv-loading"></span>
{:else if value === null || value === undefined}
  <span class="dcv-null">NULL</span>
{:else if value === ''}
  <span class="dcv-blank" use:tooltip={'empty string'}></span>
{:else}
  {value}
{/if}

<style>
  .dcv-null {
    color: var(--text-disabled);
    font-style: italic;
  }

  /* A box rather than nothing: an empty string and an empty cell look identical
     otherwise, and one of them is a value. */
  .dcv-blank {
    display: inline-block;
    width: 14px;
    height: 8px;
    border: 1px dashed var(--border);
    border-radius: 2px;
    opacity: 0.7;
  }

  .dcv-loading {
    display: inline-block;
    width: 60%;
    max-width: 90px;
    height: 7px;
    border-radius: 3px;
    background: var(--bg-hover);
  }
</style>
