<script lang="ts">
  /**
   * One cell of the session table.
   *
   * A component rather than a snippet inside the panel because it carries its own
   * styles and its own reading rules — the wait chain, the age formatting, the two
   * things the panel wants coloured — and none of that is layout the panel has an
   * opinion about.
   *
   * `row` may be absent: the grid is virtualised and asks for cells of rows that
   * have gone in the three seconds since the last read. Every branch treats that
   * as "just the value", never as an error.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import type { DataGridValue } from '$lib/components/shared/ui/DataGrid.svelte';
  import { formatAge, type ActivityRow } from '$lib/stores/picus/activity.svelte';

  interface Props {
    /** Column id, as declared by the panel. */
    columnId: string;
    value: DataGridValue;
    row: ActivityRow | undefined;
  }
  let { columnId, value, row }: Props = $props();

  const chain = $derived(row?.chain ?? []);
  const wait = $derived(row?.session.waitEvent ?? '');
  const isAge = $derived(columnId === 'queryAge' || columnId === 'stateAge' || columnId === 'txAge');
</script>

{#if columnId === 'pid'}
  <span class="ac-pid">
    {value}
    {#if row?.session.isSelf}
      <!-- Labelled, never discovered: stopping it is legal and occasionally what
           is wanted, but not by accident. -->
      <Badge variant="tone" tone="info" size="sm" label="Picus" />
    {/if}
  </span>
{:else if isAge}
  <!-- The number arrives in milliseconds and is formatted here rather than in the
       row, so the column still sorts by duration instead of alphabetically. -->
  <span class="ac-age">{formatAge(typeof value === 'number' ? value : null)}</span>
{:else if columnId === 'state'}
  <span class="ac-state" class:ac-idle-tx={row?.session.state === 'idle in transaction'}>
    {value}
    {#if wait}<span class="ac-wait">{wait}</span>{/if}
  </span>
{:else if columnId === 'blocked'}
  {#if row?.cyclic}
    <Badge variant="tone" tone="error" size="sm" label="deadlock" />
  {:else if chain.length}
    <!-- The walk out of the queue, nearest blocker first. The last hop is the one
         nothing is in front of — the only session where acting releases anybody. -->
    <span class="ac-chain">
      {#each chain as pid, i (pid)}
        <span class="ac-hop" class:ac-root={i === chain.length - 1}>{pid}</span>
        {#if i < chain.length - 1}<span class="ac-arrow">→</span>{/if}
      {/each}
    </span>
  {:else if row?.isRoot}
    <Badge variant="tone" tone="warning" size="sm" label="root of the wait" />
  {:else}
    <span class="ac-none">—</span>
  {/if}
{:else}
  <span class="ac-plain">{value}</span>
{/if}

<style>
  .ac-pid { display: inline-flex; align-items: center; gap: 5px; font-family: var(--font-code); }
  .ac-age { font-family: var(--font-code); color: var(--text-secondary); }
  .ac-plain { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ac-none { color: var(--text-disabled); }

  .ac-state { display: inline-flex; align-items: baseline; gap: 6px; min-width: 0; }
  /* The one state worth colouring: a session idle inside a transaction holds its
     locks indefinitely, and is what a blocked chain usually ends at. */
  .ac-state.ac-idle-tx { color: var(--warning); }
  .ac-wait {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ac-chain { display: inline-flex; align-items: center; gap: 4px; font-family: var(--font-code); }
  .ac-hop { color: var(--text-secondary); }
  .ac-arrow { color: var(--text-disabled); }
  .ac-root {
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--warning-subtle);
    color: var(--warning);
    font-weight: 600;
  }
</style>
