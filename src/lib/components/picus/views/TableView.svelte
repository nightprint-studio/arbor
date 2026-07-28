<script lang="ts">
  /**
   * Schema-object view — a table, a view, a sequence or a trigger.
   *
   * They share one component because they share a frame: name, connection,
   * sub-views. What differs is what those sub-views contain.
   *
   *  • **Table** — Data (paged), Structure (columns · primary key · foreign keys
   *    in both directions · indexes · triggers), DDL.
   *  • **View** — the same Data and Structure, plus its defining query instead
   *    of constraints.
   *  • **Sequence / trigger** — properties, because there is nothing else true
   *    about them.
   *
   * Data is one **continuous scroll** over a held cursor — the same behaviour a
   * query result has, because a table's rows and a statement's rows are the same
   * thing to the person reading them. The scrollbar is scaled to the row count
   * immediately (the server's estimate, marked `~`, replaced by the exact number
   * when the background count lands) and windows arrive before the viewport
   * reaches them. There is no page selector: two ways of moving through rows in
   * one product is one too many, and the total now lives in the status bar.
   *
   * Editing a cell is never a silent UPDATE — the statement is shown before it
   * runs, and never at all on a read-only connection.
   */
  import { Table2, Eye, ListOrdered, Zap, KeyRound, Link2, ArrowLeftRight } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { picusSettingsStore } from '$lib/stores/picus/settings.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { createResult, formatRowTotal, picusResultsStore } from '$lib/stores/picus/result.svelte';
  import { openRelation } from '$lib/ipc/picus/db';
  import type { PicusTab } from '$lib/types/picus';

  interface Props {
    tab: PicusTab;
  }

  let { tab }: Props = $props();

  const conn = $derived(picusTabsStore.activeConnection);
  const objectKind = $derived(tab.objectKind ?? 'table');
  const lowercase = $derived(conn?.dialect === 'postgres');
  const ident = (name: string) => (lowercase ? name.toLowerCase() : name);
  // Bound to the connection so the read-only DDL and view-definition panes still
  // answer a hover with the column's type — the same facts, in the place you are
  // most likely to want them.
  const language = $derived(sqlLanguage(conn?.dialect, conn?.id));

  const relation = $derived(
    tab.table && (objectKind === 'table' || objectKind === 'view')
      ? schemaStore.relation(tab.table)
      : null,
  );
  const sequence = $derived(
    tab.table && objectKind === 'sequence' ? schemaStore.sequence(tab.table) : null,
  );
  const trigger = $derived(
    tab.table && objectKind === 'trigger' ? schemaStore.trigger(tab.table) : null,
  );

  // The schema snapshot carries columns but not constraints — reading every
  // constraint in the database up front would make opening a connection slow for
  // detail almost nobody looks at. Ask for this relation's full detail as its tab
  // opens; the store merges it in, so a second visit is instant.
  $effect(() => {
    const name = tab.table;
    if (name && (objectKind === 'table' || objectKind === 'view')) void schemaStore.detail(name);
  });

  // ── Data (one held cursor, scrolled) ────────────────────────────────────────
  //
  // The cursor is opened as the tab opens, so switching to Data is instant, and
  // it is registered against the TAB — the results registry then closes it when
  // the tab closes, when the connection drops, or when this effect replaces it
  // with another. Nothing in this component has to remember to release it.
  const result = $derived(picusResultsStore.forOwner(tab.id));

  let rowsError = $state('');
  let opening = $state(false);

  /**
   * Which relation the cursor currently open here is on.
   *
   * A plain `let`, deliberately not `$state`: it exists to stop the effect below
   * from re-opening the same relation on every unrelated re-run, and making it
   * reactive would make writing it re-enter the effect that wrote it.
   */
  let openedKey = '';

  $effect(() => {
    const id = conn?.id;
    const name = tab.table;
    const kind = objectKind;
    if (!id || !name || kind === 'sequence' || kind === 'trigger') return;

    const key = `${id}::${name}`;
    if (key === openedKey) return;
    openedKey = key;

    let cancelled = false;
    opening = true;
    rowsError = '';
    // Same window size as a typed query — one setting, one behaviour.
    openRelation(id, name, picusSettingsStore.rowLimit)
      .then((res) => {
        const opened = createResult(id, res);
        // Rebinding the tab (or closing it) while this was in flight does NOT make
        // the cursor the server just opened go away — it has to be closed rather
        // than dropped, or it outlives everything that could have released it.
        if (cancelled) { void opened?.close(); return; }
        picusResultsStore.adopt(tab.id, opened);
      })
      .catch((e) => {
        if (cancelled) return;
        rowsError = String(e);
        // Let a retry happen: the tab is still open on a relation nothing is held
        // for, and a key left set would make every later attempt a no-op.
        openedKey = '';
      })
      .finally(() => { if (!cancelled) opening = false; });

    return () => { cancelled = true; };
  });

  const dataColumns = $derived<DataGridColumn[]>(
    (relation?.columns ?? []).map((c) => ({
      id: c.name,
      label: ident(c.name),
      hint: c.type,
      type: /NUMBER|INT|NUMERIC|DECIMAL/i.test(c.type) ? 'number' : 'text',
      width: 190,
    })),
  );

  // ── Structure ───────────────────────────────────────────────────────────────
  const columnGrid: DataGridColumn[] = [
    { id: 'name', label: 'Column', width: 220 },
    { id: 'type', label: 'Type', width: 160 },
    { id: 'nullable', label: 'Nullable', width: 100 },
    { id: 'default', label: 'Default', width: 140 },
    { id: 'key', label: 'Key', width: 80 },
  ];

  const columnRows = $derived(
    (relation?.columns ?? []).map((c) => [
      ident(c.name),
      c.type,
      c.notNull ? 'NOT NULL' : 'nullable',
      c.defaultValue ?? null,
      c.primaryKey ? 'PK' : '',
    ]),
  );

  const outgoingFks = $derived(relation?.foreignKeys ?? []);
  const incomingFks = $derived(tab.table ? schemaStore.incomingForeignKeys(tab.table) : []);
  const indexes = $derived(relation?.indexes ?? []);
  const objectTriggers = $derived(tab.table ? schemaStore.triggersOf(tab.table) : []);

  // ── DDL ─────────────────────────────────────────────────────────────────────
  // MOCK: rendered from the cached schema. The real DDL comes from the server so
  // storage clauses and constraint options are the server's own words.
  const ddl = $derived.by(() => {
    if (!relation) return '';
    if (relation.kind === 'view') {
      return `CREATE OR REPLACE VIEW ${ident(relation.name)} AS\n${relation.definition ?? '-- definition unavailable'}\n`;
    }
    const cols = relation.columns
      .map((c) => `  ${ident(c.name).padEnd(18)} ${c.type}${c.notNull ? ' NOT NULL' : ''}${c.defaultValue ? ` DEFAULT ${c.defaultValue}` : ''}`)
      .join(',\n');
    const pk = relation.columns.filter((c) => c.primaryKey).map((c) => ident(c.name));
    const pkClause = pk.length
      ? `,\n  CONSTRAINT ${ident(relation.primaryKeyName ?? `PK_${relation.name}`)} PRIMARY KEY (${pk.join(', ')})`
      : '';
    const fkClauses = outgoingFks
      .map(
        (fk) =>
          `\nALTER TABLE ${ident(relation.name)} ADD CONSTRAINT ${ident(fk.name)}\n` +
          `  FOREIGN KEY (${fk.columns.map(ident).join(', ')})\n` +
          `  REFERENCES ${ident(fk.referencedTable)} (${fk.referencedColumns.map(ident).join(', ')})` +
          `${fk.onDelete && fk.onDelete !== 'NO ACTION' ? ` ON DELETE ${fk.onDelete}` : ''};`,
      )
      .join('');
    const idxClauses = indexes
      .filter((ix) => !ix.primaryKey)
      .map(
        (ix) =>
          `\nCREATE ${ix.unique ? 'UNIQUE ' : ''}INDEX ${ident(ix.name)}\n` +
          `  ON ${ident(relation.name)} (${ix.columns.map((c) => (c.includes('(') ? c : ident(c))).join(', ')});`,
      )
      .join('');
    return `CREATE TABLE ${ident(relation.name)} (\n${cols}${pkClause}\n);\n${fkClauses}${idxClauses}\n`;
  });

  function openRelated(name: string) {
    picusTabsStore.openObject(name, schemaStore.table(name) ? 'table' : 'view', conn?.id);
  }
</script>

{#if objectKind === 'sequence'}
  {#if !sequence}
    <StateBlock tone="error" label="This sequence is not in the schema cache — refresh the connection." />
  {:else}
    <div class="ov">
      <header class="ov-head">
        <ListOrdered size={15} />
        <h1>{ident(sequence.name)}</h1>
        <Badge variant="tone" tone="neutral" size="sm" label="sequence" />
      </header>
      <dl class="ov-props">
        <div><dt>Last value</dt><dd>{sequence.lastValue.toLocaleString()}</dd></div>
        <div><dt>Increment</dt><dd>{sequence.incrementBy}</dd></div>
        <div><dt>Minimum</dt><dd>{sequence.minValue?.toLocaleString() ?? '—'}</dd></div>
        <div><dt>Maximum</dt><dd>{sequence.maxValue?.toLocaleString() ?? 'no limit'}</dd></div>
        <div><dt>Cache</dt><dd>{sequence.cacheSize ?? '—'}</dd></div>
        <div><dt>Cycles</dt><dd>{sequence.cycle ? 'yes' : 'no'}</dd></div>
      </dl>
      <p class="ov-note">
        A sequence missing from one engine's scripts is the kind of gap that only shows up when an
        insert runs — the inventory tracks it like any other object.
      </p>
    </div>
  {/if}

{:else if objectKind === 'trigger'}
  {#if !trigger}
    <StateBlock tone="error" label="This trigger is not in the schema cache — refresh the connection." />
  {:else}
    <div class="ov">
      <header class="ov-head">
        <Zap size={15} />
        <h1>{ident(trigger.name)}</h1>
        <Badge
          variant="tone"
          tone={trigger.enabled ? 'success' : 'warning'}
          size="sm"
          label={trigger.enabled ? 'enabled' : 'disabled'}
        />
      </header>
      <dl class="ov-props">
        <div><dt>Table</dt>
          <dd><button class="ov-link" onclick={() => openRelated(trigger.table)}>{ident(trigger.table)}</button></dd>
        </div>
        <div><dt>Timing</dt><dd>{trigger.timing}</dd></div>
        <div><dt>Events</dt><dd>{trigger.events.join(', ')}</dd></div>
        <div><dt>Level</dt><dd>{trigger.forEachRow ? 'FOR EACH ROW' : 'statement'}</dd></div>
      </dl>
      {#if !trigger.enabled}
        <p class="ov-warn">
          This trigger is disabled on {conn?.name ?? 'this database'}. If the scripts create
          it enabled, the installed database and the repository disagree.
        </p>
      {/if}
      <p class="ov-note">The trigger body comes from the server with the driver milestone.</p>
    </div>
  {/if}

{:else if !relation}
  <StateBlock tone="error" label="This object is not in the schema cache — refresh the connection." />

{:else if picusUiStore.tableSubview === 'data'}
  <div class="tv">
    {#if conn?.readOnly}
      <div class="tv-note">
        <Badge variant="tone" tone="warning" size="sm" label="read-only" />
        <span>{conn.name} refuses writes: cells are shown but cannot be edited here.</span>
      </div>
    {/if}
    {#if rowsError}
      <div class="tv-note">
        <Badge variant="tone" tone="error" size="sm" label="error" />
        <span>{rowsError}</span>
      </div>
    {:else if result && !result.complete}
      <!-- Said once, above the rows: the grid below shows the whole length but
           holds part of it, and its sorting and filters are inert until it holds
           all of it. The counter climbs on its own and the note leaves when there
           is nothing left to qualify. -->
      <div class="tv-note tv-note-info">
        <Badge variant="tone" tone="neutral" size="sm" label="loading" />
        <span>
          {result.loaded.toLocaleString()} of {formatRowTotal(result)} rows loaded — the rest
          arrives as you scroll. Sorting and the per-column filters come back once the whole
          relation is here; to reach a specific distant row, query it with a
          <code>WHERE</code> on an indexed column instead of scrolling to it.
        </span>
      </div>
    {/if}
    <DataGrid
      columns={dataColumns}
      source={result ?? undefined}
      filterable
      editable={!conn?.readOnly && relation.kind === 'table'}
      ariaLabel={`Rows of ${relation.name}`}
      onEditCell={() => toastStore.show('Inline editing shows its UPDATE before running it — arriving with the driver.', 'info')}
      emptyMessage={opening ? 'Opening…' : `This ${relation.kind} has no rows.`}
    />
  </div>

{:else if picusUiStore.tableSubview === 'structure'}
  <div class="st">
    <section class="st-block">
      <h2><Table2 size={13} /> Columns <Badge variant="count" label={String(relation.columns.length)} /></h2>
      <div class="st-grid">
        <DataGrid
          columns={columnGrid}
          rows={columnRows}
          showRowNumbers={false}
          ariaLabel={`Columns of ${relation.name}`}
        />
      </div>
    </section>

    {#if relation.kind === 'view'}
      <section class="st-block">
        <h2><Eye size={13} /> Definition</h2>
        <div class="st-code">
          <CodeEditor value={relation.definition ?? ''} {language} readOnly />
        </div>
      </section>
    {:else}
      <section class="st-block">
        <h2><KeyRound size={13} /> Primary key</h2>
        {#if relation.primaryKeyName}
          <p class="st-line">
            <code>{ident(relation.primaryKeyName)}</code>
            on ({relation.columns.filter((c) => c.primaryKey).map((c) => ident(c.name)).join(', ')})
          </p>
        {:else}
          <p class="st-empty">
            No primary key. Generated upserts fall back to the comparison key you pick by hand.
          </p>
        {/if}
      </section>

      <section class="st-block">
        <h2><Link2 size={13} /> Foreign keys <Badge variant="count" label={String(outgoingFks.length)} /></h2>
        {#if outgoingFks.length}
          <ul class="st-list">
            {#each outgoingFks as fk (fk.name)}
              <li>
                <code class="st-name">{ident(fk.name)}</code>
                <span class="st-rel">
                  ({fk.columns.map(ident).join(', ')}) →
                  <button class="ov-link" onclick={() => openRelated(fk.referencedTable)}>
                    {ident(fk.referencedTable)}
                  </button>
                  ({fk.referencedColumns.map(ident).join(', ')})
                </span>
                {#if fk.onDelete && fk.onDelete !== 'NO ACTION'}
                  <span use:tooltip={'What happens to these rows when the parent row is deleted'}>
                    <Badge variant="tone" tone="warning" size="sm" label={`ON DELETE ${fk.onDelete}`} />
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="st-empty">This table references nothing.</p>
        {/if}
      </section>

      {#if incomingFks.length}
        <section class="st-block">
          <h2><ArrowLeftRight size={13} /> Referenced by <Badge variant="count" label={String(incomingFks.length)} /></h2>
          <ul class="st-list">
            {#each incomingFks as ref (ref.fk.name)}
              <li>
                <button class="ov-link" onclick={() => openRelated(ref.from)}>{ident(ref.from)}</button>
                <span class="st-rel">via <code>{ident(ref.fk.name)}</code> ({ref.fk.columns.map(ident).join(', ')})</span>
                {#if ref.fk.onDelete === 'CASCADE'}
                  <span use:tooltip={'Deleting a row here deletes the referencing rows too'}>
                    <Badge variant="tone" tone="error" size="sm" label="ON DELETE CASCADE" />
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="st-block">
        <h2><ListOrdered size={13} /> Indexes <Badge variant="count" label={String(indexes.length)} /></h2>
        {#if indexes.length}
          <ul class="st-list">
            {#each indexes as ix (ix.name)}
              <li>
                <code class="st-name">{ident(ix.name)}</code>
                <span class="st-rel">({ix.columns.join(', ')})</span>
                {#if ix.unique}<Badge variant="tone" tone="accent" size="sm" label="unique" />{/if}
                {#if ix.primaryKey}<Badge variant="tone" tone="neutral" size="sm" label="backs the PK" />{/if}
                {#if ix.kind}<span class="st-kind">{ix.kind}</span>{/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="st-empty">No index. Every lookup on this table is a full scan.</p>
        {/if}
      </section>

      {#if objectTriggers.length}
        <section class="st-block">
          <h2><Zap size={13} /> Triggers <Badge variant="count" label={String(objectTriggers.length)} /></h2>
          <ul class="st-list">
            {#each objectTriggers as trg (trg.name)}
              <li>
                <button class="ov-link" onclick={() => picusTabsStore.openObject(trg.name, 'trigger', conn?.id)}>
                  {ident(trg.name)}
                </button>
                <span class="st-rel">{trg.timing} {trg.events.join('/')}</span>
                {#if !trg.enabled}<Badge variant="tone" tone="warning" size="sm" label="disabled" />{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </div>

{:else}
  <div class="tv-ddl">
    <div class="tv-ddl-bar">
      <span>Rendered from the cached schema — the server's own DDL arrives with the driver.</span>
      <span class="tv-spacer"></span>
      <Button
        variant="secondary"
        size="xs"
        onclick={() => { void navigator.clipboard.writeText(ddl).then(() => toastStore.show('DDL copied.', 'success')); }}
      >
        Copy
      </Button>
    </div>
    <div class="tv-ddl-code">
      <CodeEditor value={ddl} {language} readOnly />
    </div>
  </div>
{/if}

<style>
  .tv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  .tv-note {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 6px 10px;
    background: var(--warning-subtle);
    border-bottom: 1px solid color-mix(in srgb, var(--warning) 28%, transparent);
    font-size: 11.5px;
    color: var(--text-secondary);
  }
  .tv-note code {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-primary);
  }
  /* "Still filling" is a state, not a problem — it must not wear the warning
     colour the read-only and error notes above it use. */
  .tv-note-info {
    background: var(--bg-elevated);
    border-bottom-color: var(--border-subtle);
  }

  .tv-ddl { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .tv-ddl-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-muted);
  }
  .tv-spacer { flex: 1; }
  .tv-ddl-code { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .tv-ddl-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  /* ── Structure ───────────────────────────────────────────────────────────── */
  .st {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px 16px 40px;
    overflow-y: auto;
  }
  .st > :global(*) { flex-shrink: 0; }

  .st-block h2 {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 0 7px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .st-block h2 :global(svg) { color: var(--text-disabled); }

  .st-grid {
    display: flex;
    max-height: 320px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .st-code {
    display: flex;
    height: 220px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .st-code > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .st-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; }
  .st-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 5px 9px;
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
  }
  .st-name { font-family: var(--font-code); color: var(--text-primary); }
  .st-rel { color: var(--text-muted); font-family: var(--font-code); font-size: 11px; }
  .st-kind { color: var(--text-disabled); font-size: 10.5px; }
  .st-line { font-size: 11.5px; color: var(--text-secondary); }
  .st-line code { font-family: var(--font-code); }
  .st-empty { font-size: 11.5px; color: var(--text-disabled); font-style: italic; }

  /* ── Sequence / trigger properties ───────────────────────────────────────── */
  .ov { display: flex; flex-direction: column; gap: 14px; padding: 16px 20px 40px; overflow-y: auto; }
  .ov > :global(*) { flex-shrink: 0; }
  .ov-head { display: flex; align-items: center; gap: 9px; }
  .ov-head h1 { font-size: 16px; font-weight: 600; font-family: var(--font-code); }
  .ov-head :global(svg) { color: var(--text-muted); }

  .ov-props {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 8px;
    margin: 0;
    max-width: 780px;
  }
  .ov-props > div {
    padding: 8px 10px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
  }
  .ov-props dt { font-size: 10px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); }
  .ov-props dd {
    margin: 3px 0 0;
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-primary);
  }

  .ov-link {
    padding: 0;
    background: none;
    border: none;
    color: var(--accent);
    font-family: var(--font-code);
    font-size: inherit;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .ov-note { font-size: 11.5px; line-height: 1.5; color: var(--text-muted); max-width: 80ch; }
  .ov-warn {
    padding: 8px 10px;
    background: var(--warning-subtle);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: var(--radius-md);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-secondary);
    max-width: 80ch;
  }
</style>
