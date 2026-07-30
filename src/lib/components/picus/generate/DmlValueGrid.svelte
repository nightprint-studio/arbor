<script lang="ts">
  /**
   * DML value grid — one row per column of the target table: value, type, and
   * whether the column is part of the comparison key.
   *
   * Three things it has to get right:
   *  • **Validation is live.** A non-numeric value in a numeric column is
   *    flagged while you type, not when you save. The message says what is
   *    wrong, in the column's own terms.
   *  • **An expression is declared, never guessed.** A leading `=` means the cell
   *    is SQL — `=SYSDATE`, `=SEQ_ORDINI.nextval`, `=(SELECT MAX(ID)+1 FROM T)`,
   *    `=ALTRA_COLONNA` — and it passes through as written, with "now" translated
   *    per dialect. Everything else is a value and gets quoted, including the word
   *    `SYSDATE` typed into a description. `==` escapes a value that really does
   *    start with an equals sign.
   *  • **The key is explicit.** It decides the WHERE of updates and the
   *    existence check of "skip if present"; leaving it unset falls back to the
   *    primary key, and the grid says so.
   *
   * Keyboard: Tab walks value → key → next value, so a whole row is fillable
   * without the mouse.
   *
   * ## Several rows, one at a time
   *
   * The form composes as many rows as you like, and shows **one**. The obvious
   * alternative — a spreadsheet, a column per column — falls over on the tables
   * this product exists for: forty columns is a horizontal scroll where the type,
   * the NOT NULL flag and the validation message have nowhere to live. Keeping the
   * column-major layout and putting the rows on a strip above it means every row
   * is entered with the same affordances the single row had, and the strip says at
   * a glance which of them is the one with the bad value.
   */
  import { ChevronLeft, ChevronRight, Copy, KeyRound, Plus, Sigma, Trash2 } from 'lucide-svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { isNow, nowFunction, readValue } from '$lib/utils/picus/sql-values';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';

  const columns = $derived(dmlStore.columns);
  const keyNames = $derived(new Set(dmlStore.keyColumns.map((c) => c.name)));
  /** Only the explicit picks are "chosen"; the rest are the primary-key fallback. */
  const explicitKey = $derived(Object.values(dmlStore.keySelection).some(Boolean));

  const rows = $derived(dmlStore.formRows);
  const at = $derived(dmlStore.formCursor);

  /**
   * What a row's chip says.
   *
   * The comparison key where there is one — that is what identifies the row, so
   * it is what tells two of them apart — falling back to the first value typed,
   * and to the position when nothing has been typed at all.
   */
  function chipLabel(row: Record<string, string>, index: number): string {
    const key = dmlStore.keyColumns.map((c) => row[c.name]?.trim()).filter(Boolean).join(' · ');
    if (key) return key;
    const first = columns.map((c) => row[c.name]?.trim()).find(Boolean);
    return first || `Row ${index + 1}`;
  }
</script>

<!-- The strip only appears once there is more than one row, or once the user has
     reached for it. A single-row form is the common case and must not grow a
     navigator it has nothing to navigate. -->
<div class="vg-rows">
  <div class="vg-chips" role="tablist" aria-label="Rows to write">
    {#each rows as row, i (i)}
      {@const issues = dmlStore.rowIssues.get(i)}
      <button
        type="button"
        role="tab"
        class="vg-chip"
        class:vg-chip-on={i === at}
        class:vg-chip-bad={!!issues}
        aria-selected={i === at}
        use:tooltip={issues ? issues.join('\n') : undefined}
        onclick={() => dmlStore.selectFormRow(i)}
      >
        <span class="vg-chip-n">{i + 1}</span>
        <span class="vg-chip-label">{chipLabel(row, i)}</span>
      </button>
    {/each}
  </div>

  <span class="vg-rows-spacer"></span>

  {#if rows.length > 1}
    <Button
      variant="icon"
      size="xs"
      ariaLabel="Previous row"
      disabled={at === 0}
      tooltip={'Previous row'}
      onclick={() => dmlStore.selectFormRow(at - 1)}
    >
      {#snippet iconStart()}<ChevronLeft size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      ariaLabel="Next row"
      disabled={at === rows.length - 1}
      tooltip={'Next row'}
      onclick={() => dmlStore.selectFormRow(at + 1)}
    >
      {#snippet iconStart()}<ChevronRight size={13} />{/snippet}
    </Button>
  {/if}
  <Button
    variant="icon"
    size="xs"
    ariaLabel="Duplicate this row"
    tooltip={'Duplicate this row — the fast way to enter a near-identical one'}
    onclick={() => dmlStore.addFormRow(true)}
  >
    {#snippet iconStart()}<Copy size={13} />{/snippet}
  </Button>
  <Button
    variant="icon"
    size="xs"
    ariaLabel="Add a row"
    tooltip={'Add an empty row'}
    onclick={() => dmlStore.addFormRow(false)}
  >
    {#snippet iconStart()}<Plus size={13} />{/snippet}
  </Button>
  <Button
    variant="icon"
    size="xs"
    ariaLabel="Remove this row"
    tooltip={rows.length > 1 ? 'Remove this row' : 'Empty this row'}
    onclick={() => dmlStore.removeFormRow()}
  >
    {#snippet iconStart()}<Trash2 size={13} />{/snippet}
  </Button>
</div>

<!-- No columns is an empty STATE, not an empty table with a sentence under it.
     It used to draw the header row over nothing and add a bare grey line —
     one of four different ways this screen said "there is nothing here yet",
     none of which looked like the other three. -->
{#if !columns.length}
  <StateBlock
    tone="info"
    fill={false}
    label="Choose a table above — its columns appear here, ready to fill in."
  />
{:else}
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
    {@const written = readValue(value)}
    <div class="vg-row" role="row">
      <span class="vg-col" role="cell">
        <span class="vg-col-name">{col.name}</span>
        {#if col.primaryKey}
          <span use:tooltip={'Primary key'}><Badge variant="tone" tone="accent" size="sm" label="PK" /></span>
        {:else if col.notNull}
          <!-- Neutral, not amber. A NOT NULL column is a fact about the table,
               true before anybody typed anything; painting it in the warning
               colour makes an ordinary schema look like a list of problems. The
               field that is actually left empty gets the amber, from `error`. -->
          <span use:tooltip={'NOT NULL — the database rejects an empty value'}>
            <Badge variant="tone" tone="neutral" size="sm" label="NN" />
          </span>
        {/if}
      </span>

      <span class="vg-value" role="cell">
        <Input
          {value}
          size="sm"
          error={error}
          ariaLabel={`Value for ${col.name}`}
          placeholder={col.primaryKey ? 'required' : 'empty leaves the column out'}
          oninput={(v) => dmlStore.setValue(col.name, v)}
        />
        {#if written.kind === 'expression'}
          <!-- Marked because the difference is invisible in the emitted statement
               until it is too late: one of these two goes in with quotes. -->
          <span
            class="vg-expr"
            use:tooltip={{
              content: 'SQL, emitted as written — not a quoted value',
              description: isNow(written.sql)
                ? `Translated per dialect — Oracle: ${nowFunction('oracle')} · PostgreSQL: ${nowFunction('postgres')}`
                : 'Passed through exactly. Picus does not interpret it.',
            }}
          >
            <Sigma size={11} /> SQL
          </span>
        {/if}
      </span>

      <!-- The declared type, elided rather than cut. `character varying(50)` is
           wider than this column at any sensible width, and it used to be sliced
           mid-token — "character varying(5" reads as a length of 5. -->
      <span class="vg-type" role="cell">
        <span class="vg-type-text" use:tooltip={col.type}>{col.type}</span>
      </span>

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
  <!-- Only reached with columns on screen — the empty case is the state block
       above, so "pick at least one column" is never advice over an empty grid. -->
  {#if !dmlStore.keyColumns.length}
    <span class="vg-warn">No comparison key.</span>
    Updates would have no WHERE clause and "skip if present" could not check anything —
    pick at least one column.
  {:else if explicitKey}
    Key: <code>{dmlStore.keyColumns.map((c) => c.name).join(', ')}</code>.
  {:else}
    Key falls back to the primary key: <code>{dmlStore.keyColumns.map((c) => c.name).join(', ')}</code>.
  {/if}
  {#if dmlStore.columnsFromScripts}
    Columns and types come from what this repository's scripts write — no connected
    database has this table.
  {:else if connectionsStore.active}
    Types come from {connectionsStore.active.name}.
  {:else}
    Types come from the statements themselves — no connection is open.
  {/if}
  {#if rows.length > 1}
    {rows.length} rows go into one block — a row with nothing typed in it is skipped.
  {/if}
</p>
{/if}

<style>
  /* The row strip. Sits above the grid rather than beside it, because the grid is
     already as wide as the window allows and the strip has to be able to wrap. */
  .vg-rows {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    padding-bottom: 8px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .vg-rows-spacer { flex: 1; min-width: 8px; }

  .vg-chips { display: flex; gap: 4px; flex-wrap: wrap; min-width: 0; }
  .vg-chip {
    display: inline-flex;
    align-items: baseline;
    gap: 5px;
    max-width: 220px;
    padding: 2px 8px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .vg-chip:hover { background: var(--bg-hover); color: var(--text-primary); }
  .vg-chip-on {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  /* A row holding a value that cannot be written — the reason the strip exists:
     with one row on screen at a time, this is the only thing that says the
     problem is somewhere else. */
  .vg-chip-bad { border-color: var(--error); color: var(--error); }
  .vg-chip-n {
    font-variant-numeric: tabular-nums;
    font-size: var(--font-size-3xs);
    color: var(--text-disabled);
    flex-shrink: 0;
  }
  .vg-chip-on .vg-chip-n { color: inherit; }
  .vg-chip-label {
    font-family: var(--font-code);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .vg {
    display: grid;
    grid-template-columns: minmax(140px, 200px) minmax(200px, 1fr) minmax(110px, 170px) 44px;
    font-size: var(--font-size-sm);
  }

  .vg-head,
  .vg-row { display: contents; }

  .vg-head > span {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    font-size: var(--font-size-2xs);
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
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .vg-type-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vg-type {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
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
    font-size: var(--font-size-3xs);
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
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }
  .vg-note code {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .vg-warn { color: var(--warning); font-weight: 600; }
</style>
