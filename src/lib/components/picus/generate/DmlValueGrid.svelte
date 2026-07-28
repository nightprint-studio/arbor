<script lang="ts">
  /**
   * DML value grid — one row per column of the target table: value, type, and
   * whether the column is part of the comparison key.
   *
   * Three things it has to get right:
   *  • **Validation is live.** A non-numeric value in a numeric column is
   *    flagged while you type, not when you save. The message says what is
   *    wrong, in the column's own terms.
   *  • **Special values are not literals.** `NULL`, `SYSDATE`,
   *    `CURRENT_TIMESTAMP` are marked as expressions and pass through
   *    unquoted — translated per dialect on emission (`SYSDATE` becomes
   *    `CURRENT_TIMESTAMP` on PostgreSQL).
   *  • **The key is explicit.** It decides the WHERE of updates and the
   *    existence check of "skip if present"; leaving it unset falls back to the
   *    primary key, and the grid says so.
   *
   * Keyboard: Tab walks value → key → next value, so a whole row is fillable
   * without the mouse.
   */
  import { KeyRound, Sigma } from 'lucide-svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { looksLikeExpression, nowFunction } from '$lib/utils/picus/sql-values';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';

  const columns = $derived(dmlStore.columns);
  const keyNames = $derived(new Set(dmlStore.keyColumns.map((c) => c.name)));
  /** Only the explicit picks are "chosen"; the rest are the primary-key fallback. */
  const explicitKey = $derived(Object.values(dmlStore.keySelection).some(Boolean));
</script>

<div class="vg" role="table" aria-label="Values to write">
  <div class="vg-head" role="row">
    <span role="columnheader">Column</span>
    <span role="columnheader">Value</span>
    <span role="columnheader">Type</span>
    <span role="columnheader" use:tooltip={'Comparison key — the WHERE of updates and the existence check'}>Key</span>
  </div>

  {#each columns as col (col.name)}
    {@const value = dmlStore.values[col.name] ?? ''}
    {@const error = dmlStore.validation[col.name] ?? null}
    {@const expression = looksLikeExpression(value)}
    <div class="vg-row" role="row">
      <span class="vg-col" role="cell">
        <span class="vg-col-name">{col.name}</span>
        {#if col.primaryKey}
          <span use:tooltip={'Primary key'}><Badge variant="tone" tone="accent" size="sm" label="PK" /></span>
        {:else if col.notNull}
          <span use:tooltip={'NOT NULL — the database rejects an empty value'}>
            <Badge variant="tone" tone="warning" size="sm" label="NN" />
          </span>
        {/if}
      </span>

      <span class="vg-value" role="cell">
        <Input
          {value}
          size="sm"
          error={error}
          ariaLabel={`Value for ${col.name}`}
          placeholder={col.primaryKey ? 'required' : 'empty = NULL'}
          oninput={(v) => dmlStore.setValue(col.name, v)}
        />
        {#if expression}
          <span
            class="vg-expr"
            use:tooltip={{
              content: 'Written as an expression, not a quoted literal',
              description: `Oracle: ${nowFunction('oracle')} · PostgreSQL: ${nowFunction('postgres')}`,
            }}
          >
            <Sigma size={11} /> expr
          </span>
        {/if}
      </span>

      <span class="vg-type" role="cell">{col.type}</span>

      <span class="vg-key" role="cell">
        <button
          type="button"
          class="vg-keybox"
          class:vg-on={keyNames.has(col.name)}
          class:vg-implied={keyNames.has(col.name) && !dmlStore.keySelection[col.name]}
          aria-pressed={keyNames.has(col.name)}
          aria-label={`Use ${col.name} as part of the comparison key`}
          use:tooltip={keyNames.has(col.name) && !dmlStore.keySelection[col.name]
            ? 'Part of the key by primary-key fallback — click to pin it explicitly'
            : 'Part of the comparison key'}
          onclick={() => dmlStore.toggleKey(col.name)}
        >
          <KeyRound size={10} />
        </button>
      </span>
    </div>
  {/each}
</div>

<p class="vg-note">
  {#if !dmlStore.keyColumns.length}
    <span class="vg-warn">No comparison key.</span>
    Updates would have no WHERE clause and "skip if present" could not check anything —
    pick at least one column.
  {:else if explicitKey}
    Key: <code>{dmlStore.keyColumns.map((c) => c.name).join(', ')}</code>.
  {:else}
    Key falls back to the primary key: <code>{dmlStore.keyColumns.map((c) => c.name).join(', ')}</code>.
  {/if}
  {#if connectionsStore.active}
    Types come from {connectionsStore.active.name}.
  {:else}
    Types come from the script inventory — no connection is open.
  {/if}

</p>

<style>
  .vg {
    display: grid;
    grid-template-columns: minmax(140px, 200px) minmax(200px, 1fr) minmax(90px, 130px) 44px;
    font-size: 12px;
  }

  .vg-head,
  .vg-row { display: contents; }

  .vg-head > span {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .vg-col,
  .vg-value,
  .vg-type,
  .vg-key {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .vg-key { justify-content: center; }

  .vg-col-name {
    font-family: var(--font-code);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .vg-type {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .vg-value { gap: 7px; }
  .vg-value :global(.input-wrap) { flex: 1; min-width: 0; }

  /* Expression marker: this value is emitted unquoted and dialect-translated. */
  .vg-expr {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
    padding: 1px 5px;
    border: 1px solid color-mix(in srgb, var(--info) 34%, transparent);
    border-radius: var(--radius-sm);
    background: var(--info-subtle);
    color: var(--info);
    font-size: 9.5px;
    font-weight: 600;
  }

  .vg-keybox {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-disabled);
    cursor: pointer;
  }
  .vg-keybox:hover { border-color: var(--border-focus); color: var(--text-secondary); }
  .vg-keybox.vg-on {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  /* Implied by the primary key rather than chosen — dimmer, still lit. */
  .vg-keybox.vg-implied { opacity: 0.6; border-style: dashed; }

  .vg-note {
    margin-top: 10px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-muted);
  }
  .vg-note code {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
  }
  .vg-warn { color: var(--warning); font-weight: 600; }
</style>
