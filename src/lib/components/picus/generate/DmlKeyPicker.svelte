<script lang="ts">
  /**
   * Which columns identify a row — the comparison key.
   *
   * It is the `WHERE` of an update or a delete, the conflict target of an upsert,
   * and the existence test of a skip-if-present guard. Three of the four
   * operations are *defined* by it, so getting it wrong does not produce a broken
   * script, it produces a working script that touches the wrong rows.
   *
   * The guided form picks it in the value grid, beside the values. The imported
   * sources have no grid — and, with no database connected, no primary key to fall
   * back on either — so they ask for it here. Deliberately the same store call,
   * so the two entry points cannot drift into meaning different things.
   *
   * Insert needs no key, so the picker says so rather than disappearing: a control
   * that vanishes teaches people it was never important.
   */
  import { KeyRound } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';

  const needed = $derived(dmlStore.operation !== 'insert');
  const chosen = $derived(new Set(dmlStore.keyColumns.map((c) => c.name)));
  /** No explicit tick anywhere — so what is highlighted is the primary key. */
  const implicit = $derived(
    chosen.size > 0 && !dmlStore.columns.some((c) => dmlStore.keySelection[c.name]),
  );
</script>

<div class="kp">
  <span class="kp-label">
    <KeyRound size={12} />
    Comparison key
  </span>

  <div class="kp-chips" role="group" aria-label="Columns that identify a row">
    {#each dmlStore.columns as column (column.name)}
      {@const on = chosen.has(column.name)}
      <button
        type="button"
        class="kp-chip"
        class:kp-on={on}
        aria-pressed={on}
        onclick={() => dmlStore.toggleKey(column.name)}
      >
        {column.name}
      </button>
    {/each}
  </div>

  <span class="kp-note">
    {#if !needed}
      An insert matches nothing, so it needs no key.
    {:else if !chosen.size}
      <span
        class="kp-missing"
        use:tooltip={'Without it there is nothing to put in the WHERE clause, and the statement would touch every row'}
      >
        Pick the columns that identify a row.
      </span>
    {:else if implicit}
      The table's primary key. Tick others to compare on something else.
    {:else}
      {chosen.size} column{chosen.size === 1 ? '' : 's'}.
    {/if}
  </span>
</div>

<style>
  .kp {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .kp-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .kp-label :global(svg) { color: var(--text-disabled); }

  .kp-chips { display: flex; gap: 4px; flex-wrap: wrap; }
  .kp-chip {
    padding: 2px 8px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-pill, 999px);
    color: var(--text-secondary);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .kp-chip:hover { background: var(--bg-hover); color: var(--text-primary); }
  .kp-on {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }

  .kp-note { font-size: var(--font-size-xs); color: var(--text-disabled); }
  .kp-missing { color: var(--warning); }
</style>
