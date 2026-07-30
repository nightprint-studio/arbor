<script lang="ts">
  /**
   * The rows a statement returned — a panel of the bottom dock.
   *
   * It lives in the dock rather than under the editor for the reason every other
   * panel does: it is a *result*, not part of the document. A pane welded under
   * the editor could not be closed, so a tab that had once run a query kept a
   * third of its height for a grid nobody was reading, and there was no way to
   * get it back. In the dock it closes like Consistency, reopens on the next run,
   * and shares the one place the window puts answers.
   *
   * Results and Messages stay two panes of the same panel. A failed statement's
   * reason has to be one click from the grid that did not fill — separating them
   * is how a user ends up staring at an empty grid with the explanation filed
   * somewhere else.
   *
   * The grid is a **window onto a held cursor**, not a block of fetched rows: the
   * scrollbar is scaled to the result's length from the first frame and the rest
   * arrives as you approach it. That length starts as the planner's estimate and
   * is therefore marked `~` everywhere it appears, until the background count
   * replaces it with the real number.
   */
  import { FormInput, Gauge, Network } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import DataGrid, {
    type DataGridColumn,
    type DataGridValue,
  } from '$lib/components/shared/ui/DataGrid.svelte';
  import ResultExportButton from './ResultExportButton.svelte';
  import ResultCell from './ResultCell.svelte';
  import ResultEditBar from './ResultEditBar.svelte';
  import LobViewerModal from './LobViewerModal.svelte';
  import QueryPlanView from './QueryPlanView.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { formatElapsed, queryStore, type ResultPane } from '$lib/stores/picus/query.svelte';
  import { picusPlanStore } from '$lib/stores/picus/plan.svelte';
  import { picusProvidersStore } from '$lib/stores/picus/providers.svelte';
  import { formatRowTotal, picusResultsStore } from '$lib/stores/picus/result.svelte';
  import { editability, resultEditStore } from '$lib/stores/picus/result-edit.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import type { CellValue } from '$lib/types/picus';

  /**
   * The query tab being looked at, if the active tab is one.
   *
   * `null` for a generate or an inventory tab, and the panel then says so rather
   * than showing the last query's rows: a grid that outlived the tab it belongs
   * to is a grid nobody can tell the provenance of.
   */
  const tab = $derived(picusTabsStore.active?.kind === 'query' ? picusTabsStore.active : null);
  // `tabState`, never `state`: a local called `state` makes every later `$state(…)`
  // in this file parse as a subscription to it rather than as the rune.
  const tabState = $derived(tab ? queryStore.read(tab.id) : null);
  const result = $derived(tab ? picusResultsStore.forOwner(tab.id) : null);

  /**
   * The panes.
   *
   * Called **Rows**, not "Results": the dock tab above this row is already called
   * Results, so the word appeared twice, one line under the other, naming two
   * different scopes. The pane holds rows; the tab holds the result.
   *
   * Messages carries the number of errors in it, because a failed statement's
   * reason lands there while the eye is on an empty grid — and a pane that has
   * something to say should say so from the outside.
   *
   * Plan is here rather than in a panel of its own because it answers a question
   * asked *about the rows on screen* — "why was that slow" — and a separate panel
   * would put the answer somewhere the question is not.
   */
  const errorCount = $derived(
    (tabState?.messages ?? []).filter((m) => m.level === 'error').length,
  );
  /**
   * The connection the rows came from — for the engine the `INSERT` export quotes
   * for, and for whether they can be edited at all.
   *
   * Through the tabs store, never `connectionsStore.byId(tab.connectionId)` on its
   * own: that misses the fallback every other reader applies, so a tab whose
   * binding does not resolve ran its statement against the window's connection
   * while this panel described nothing — and `editability` reads a missing
   * connection as read-only, which is how a restored tab came back unwritable.
   */
  const conn = $derived(picusTabsStore.connectionOf(tab));

  /**
   * Does this engine answer for plans at all?
   *
   * Read off the descriptor rather than branched on the engine's name — that is the
   * whole point of the capability matrix. False while the descriptors are still
   * loading, so the pane appears a moment late rather than appearing and refusing.
   *
   * Declared **above** `paneTabs` and not beside the rest of the plan code: the
   * argument to `$derived(...)` is an ordinary expression evaluated where it is
   * written, so a `const` referenced from it and declared further down is a
   * use-before-declaration error, not a stylistic quibble.
   */
  const canExplain = $derived(picusProvidersStore.capabilities(conn?.dialect)?.explain ?? false);
  $effect(() => void picusProvidersStore.load());

  const paneTabs = $derived<TabItem[]>([
    { id: 'results', label: 'Rows' },
    { id: 'messages', label: 'Messages', badge: errorCount || undefined },
    // Only where the engine has plans at all: a capability the engine lacks must be
    // absent, not present and failing.
    ...(canExplain ? [{ id: 'plan', label: 'Plan' } satisfies TabItem] : []),
  ]);

  const gridColumns = $derived<DataGridColumn[]>(
    (result?.columns ?? []).map((c) => ({
      id: c.name,
      label: c.name,
      hint: c.type,
      type: /NUMBER|INT|NUMERIC|DECIMAL/i.test(c.type) ? 'number' : 'text',
      width: 180,
    })),
  );

  // ── The plan of the statement, beside the rows it would produce ──────────────
  //
  // `canExplain` and `conn` are declared above, next to the tab strip that reads
  // them.

  /**
   * The plan of the statement this tab is pointing at.
   *
   * Its own store, keyed by the same tab: a plan is about a *statement*, so it
   * survives the result being closed and exists for a statement that has never been
   * run. Nothing here reaches into the grid's state and nothing there reaches into
   * this.
   */
  const planState = $derived(tab ? picusPlanStore.read(tab.id) : null);

  /** Ask the server what it *would* do. Nothing is executed. */
  function explain() {
    if (!tab || !conn) return;
    queryStore.setPane(tab.id, 'plan');
    void picusPlanStore.explain(tab.id, conn.id, conn.dialect);
  }

  /**
   * Ask the server what it *did* — which means running the statement.
   *
   * Confirmed rather than fired, and the confirmation names the consequence. The
   * backend refuses to measure anything that is not a read, so this can never be a
   * write; it can still be the four-minute report the user was only curious about.
   */
  let confirmMeasure = $state(false);
  function measure() {
    confirmMeasure = false;
    if (!tab || !conn) return;
    queryStore.setPane(tab.id, 'plan');
    void picusPlanStore.measure(tab.id, conn.id, conn.dialect);
  }

  /**
   * Which table these rows are from, when that is answerable.
   *
   * A relation tab knows. A query tab is told, by the backend's parser, from the
   * statement that **ran** — never from the tab's text, which is a scratchpad
   * holding several statements and therefore reads from all of them at once. That
   * was the bug: an ordinary single-table query in a tab that also held an older
   * one reported itself as a join, and its rows were refused editing and refused to
   * open their large objects.
   *
   * Empty is a real answer, and {@link sourceReason} says which one.
   */
  const source = $derived(tabState?.source ?? null);
  const sourceTable = $derived(tab?.table ?? (source && !source.isView ? source.relation : ''));
  /** The backend's own sentence about why there is no editable source, if any. */
  const sourceReason = $derived(source?.reason ?? '');

  /**
   * Can these rows be edited, and if not, why?
   *
   * Asked here and again inside the store before anything is written. One function,
   * two callers, is what keeps the button and the write from disagreeing about
   * whether a row is addressable.
   */
  const editable = $derived(
    editability(
      sourceTable,
      sourceTable ? schemaStore.table(sourceTable)?.columns ?? null : null,
      result?.columns ?? [],
      conn?.readOnly ?? true,
      sourceReason,
    ),
  );

  /**
   * Point the edit store at whatever result is on screen.
   *
   * Pending edits belong to the rows they were made on, and a re-run may return a
   * different set entirely — so they are discarded when the result changes rather
   * than reapplied to rows that only happen to be at the same index.
   */
  $effect(() => {
    resultEditStore.bind(result?.resultId ?? '');
  });

  /**
   * Everything the row counter cannot fit, for the pointer that asks.
   *
   * Three separate facts, and which of them apply depends on the result: the total
   * may be the planner's guess rather than a count, the window may still be
   * filling, and while it is, sorting and the per-column filters stand down —
   * over a window they would order and hide a fraction of the rows while looking
   * like they had done all of them.
   */
  const partialNote = $derived.by(() => {
    if (!result) return '';
    const lines = [
      result.approximate
        ? `Estimated by the planner${result.counting ? ' — counting the exact number now' : ''}.`
        : `${result.total.toLocaleString()} row(s) in the result.`,
    ];
    if (!result.complete) {
      lines.push(
        `${result.loaded.toLocaleString()} loaded; the rest arrives as you scroll.`,
        'Sorting and the per-column filters wait until the whole result is loaded.',
      );
    }
    return lines.join(' ');
  });

  /** Columns whose value was not fetched — their cells hold a size. */
  const masked = $derived(new Set(result?.maskedColumns ?? []));

  /** The large object being read, when one is open. */
  let opened = $state<{ column: string; keys: Record<string, string | null> } | null>(null);

  /**
   * Read one masked value.
   *
   * The row is addressed by the key it was **read** with, exactly as an edit is —
   * and for the same reason the masking is only ever applied to a relation that has
   * one. A row that has scrolled out of memory cannot be addressed and says so
   * rather than fetching something arbitrary.
   */
  function reveal(rowIndex: number, column: string) {
    if (!result) return;
    // Without a key there is nothing to fetch the value *by*, and opening the
    // viewer anyway would put a dialog on screen whose only job is to fail. The
    // reason is the one `editability` already worked out, which names the actual
    // obstacle — no key on the table, the key not selected, more than one source —
    // instead of leaving a click that appears to do nothing at all.
    if (!editable.keys.length) {
      toastStore.show(`This value cannot be opened. ${editable.reason}`, 'warning');
      return;
    }
    const row = result.rowAt(rowIndex);
    if (!row) {
      toastStore.show('That row is no longer loaded — scroll back to it and try again.', 'warning');
      return;
    }
    const keys: Record<string, string | null> = {};
    for (const name of editable.keys) {
      const at = result.columns.findIndex((c) => c.name.toUpperCase() === name.toUpperCase());
      const value = at < 0 ? null : row[at];
      keys[name] = value === null || value === undefined ? null : String(value);
    }
    opened = { column, keys };
  }

  /**
   * The grid's cell type is wider than a SQL cell's, and the gap is narrowed here
   * rather than widened everywhere else: a row that is present has no `undefined`
   * cells, and the driver reports booleans as text.
   */
  function asCell(value: DataGridValue): CellValue {
    if (value === undefined) return null;
    return typeof value === 'boolean' ? String(value) : value;
  }

  /** Prefill the generator with this result's table and columns. */
  function toGenerator() {
    if (!result?.columns.length) return;
    // MOCK: the real bridge carries the result's table identity from the server.
    dmlStore.setTable(dmlStore.table);
    picusTabsStore.openGenerate();
    picusUiStore.showSection('generate');
    toastStore.show('Generator prefilled from this result.', 'success');
  }
</script>

{#if !tab || !tabState}
  <div class="qr">
    <BottomPanelHeader title="Results" onClose={() => picusUiStore.closeBottom()} />
    <StateBlock tone="info" fill={false} label="Open a query tab to run a statement." />
  </div>
{:else}
  <div class="qr">
    <!-- The panel's own header — the dock no longer supplies one. The pane switch
         lives inside it rather than on a second row, which is the row this
         arrangement saves. -->
    <BottomPanelHeader title="Results" onClose={() => picusUiStore.closeBottom()}>
      <Tabs
        items={paneTabs}
        value={tabState.pane}
        variant="underline"
        size="sm"
        ariaLabel="Result pane"
        onSelect={(id) => queryStore.setPane(tab.id, id as ResultPane)}
      />
      {#snippet actions()}
      {#if tabState.running}
        <span class="qr-stats"><Spinner size={11} /> running…</span>
      {:else if result}
        <!-- The total is the server's ESTIMATE until the background count lands,
             and carries a `~` for exactly as long as that is true. Precision the
             product does not have must not be implied by the way it is printed.

             "Still filling" is said HERE, in four words, and not in the full-width
             notice this used to put above the grid. That notice was forty words of
             permanent chrome explaining the sorting rules to someone who had not
             asked to sort, and it cost a whole band of a panel that was already
             giving the rows less height than its own headers. The sentence is
             still available — it is the tooltip — and the count it was built
             around is now next to the count it qualifies. -->
        <span class="qr-stats" use:tooltip={partialNote}>
          {result.complete
            ? formatRowTotal(result)
            : `${result.loaded.toLocaleString()} of ${formatRowTotal(result)}`} rows
          · {formatElapsed(tabState.elapsedMs ?? result.elapsedMs)}
        </span>
      {:else if tabState.affected !== null}
        <!-- A write has no result to read a time off, which is why the tab keeps
             one: "how long did that take" is asked about an UPDATE at least as
             often as about a SELECT. -->
        <span class="qr-stats">
          {tabState.affected.toLocaleString()} rows affected{tabState.elapsedMs !== null
            ? ` · ${formatElapsed(tabState.elapsedMs)}`
            : ''}
        </span>
      {:else if tabState.elapsedMs !== null}
        <span class="qr-stats">{formatElapsed(tabState.elapsedMs)}</span>
      {/if}
      {#if result}
        <!-- Said before it is needed. "Can I change this?" is asked by
             double-clicking a cell, and a grid that simply did nothing would read
             as broken rather than as protecting a table with no key. -->
        <span
          class="qr-edit"
          class:qr-edit-on={editable.ok}
          use:tooltip={{
            content: editable.ok
              ? `Double-click a cell to change it. Keyed on ${editable.keys.join(', ')}. Nothing is written until you press Store.`
              : editable.reason,
          }}
        >
          {editable.ok ? 'editable' : 'read-only rows'}
        </span>
      {/if}
      {#if canExplain}
        <!-- Two buttons, never one with a modifier: the first asks the server what
             it would do, the second makes it do it. That difference is the whole
             feature, and a flag on a single control would hide it. -->
        <Button
          variant="icon"
          size="xs"
          tooltip={'Explain — ask the server how it would run this statement. Nothing is executed.'}
          ariaLabel="Explain the statement"
          disabled={planState?.running ?? false}
          onclick={explain}
        >
          {#snippet iconStart()}<Network size={13} />{/snippet}
        </Button>
        <Button
          variant="icon"
          size="xs"
          tooltip={'Analyze — RUNS the statement and reports the real times and row counts.'}
          ariaLabel="Analyze the statement (runs it)"
          disabled={planState?.running ?? false}
          onclick={() => (confirmMeasure = true)}
        >
          {#snippet iconStart()}<Gauge size={13} />{/snippet}
        </Button>
      {/if}
      <Button
        variant="icon"
        size="xs"
        tooltip={'Generate DML from this result'}
        ariaLabel="Generate DML from this result"
        disabled={!result}
        onclick={toGenerator}
      >
        {#snippet iconStart()}<FormInput size={13} />{/snippet}
      </Button>
      <ResultExportButton
        {result}
        dialect={conn?.dialect ?? 'postgres'}
        table={sourceTable}
      />
      {/snippet}
    </BottomPanelHeader>

    <div class="qr-body">
      {#if tabState.pane === 'results'}
        {#if tabState.error}
          <StateBlock tone="error" label={tabState.error} />
        {:else if result}
          <div class="qr-grid">
            <ResultEditBar onStore={() => void resultEditStore.storeActive()} />
            <DataGrid
              columns={gridColumns}
              source={result ?? undefined}
              filterable
              editable={editable.ok}
              onEditCell={(rowIndex, columnIndex) => {
                const column = result?.columns[columnIndex];
                if (column) resultEditStore.begin(rowIndex, column.name);
              }}
              ariaLabel="Query results"
            >
              {#snippet cell({ value, rowIndex, columnIndex })}
                {@const name = result?.columns[columnIndex]?.name ?? ''}
                {@const cellValue = asCell(value)}
                <ResultCell
                  value={cellValue}
                  masked={masked.has(name)}
                  onReveal={() => reveal(rowIndex, name)}
                  edited={resultEditStore.edited(rowIndex, name)}
                  editing={resultEditStore.editing?.rowIndex === rowIndex
                    && resultEditStore.editing?.column === name}
                  onCommit={(next) => resultEditStore.change(rowIndex, name, cellValue, next)}
                  onCancel={() => resultEditStore.cancel()}
                />
              {/snippet}
            </DataGrid>
          </div>
        {:else if tabState.affected !== null}
          <!-- A write has no rows to show, and an empty grid would suggest it
               returned none rather than that it returns none. -->
          <StateBlock
            tone="success"
            label={`${tabState.affected.toLocaleString()} row(s) affected. This statement returns no rows.`}
          />
        {:else if tabState.hasRun}
          <StateBlock tone="info" label="The statement completed and returned no rows." />
        {:else}
          <StateBlock tone="info" label="Run the query to see its rows." />
        {/if}
      {:else if tabState.pane === 'plan'}
        {#if planState?.running}
          <StateBlock tone="loading">
            {#snippet spinner()}<Spinner size={14} />{/snippet}
            <span>
              {planState.measuring
                ? 'Running the statement to measure it…'
                : 'Asking the server for the plan…'}
            </span>
          </StateBlock>
        {:else if planState?.error}
          <StateBlock tone="error" label={planState.error} />
        {:else if planState?.plan}
          <QueryPlanView plan={planState.plan} sql={planState.sql} />
        {:else}
          <StateBlock
            tone="info"
            label="Explain shows how the server would run the statement the caret is in. Analyze runs it and reports what actually happened."
          />
        {/if}
      {:else}
        <div class="qr-log">
          {#each tabState.messages as msg, i (i)}
            <div class="qr-log-line" class:qr-log-error={msg.level === 'error'}>
              <span class="qr-log-time">{msg.time}</span>
              <span>{msg.text}</span>
            </div>
          {:else}
            <p class="qr-log-empty">No message yet.</p>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if confirmMeasure}
  <!-- The consequence, before it happens. Analyze is not a display option of
       Explain: it executes the statement, and on a report that takes minutes the
       difference is the user's afternoon. -->
  <ConfirmModal
    title="Analyze runs the statement"
    message="Measuring a plan means executing the statement and reporting what really happened."
    detail="Only a read can be measured — anything else is refused. A slow statement will take as long as it takes; Cancel on this connection stops it."
    variant="warning"
    confirmLabel="Run and measure"
    onConfirm={measure}
    onCancel={() => (confirmMeasure = false)}
  />
{/if}

{#if opened && conn}
  <LobViewerModal
    connectionId={conn.id}
    table={sourceTable}
    column={opened.column}
    keys={opened.keys}
    onClose={() => (opened = null)}
  />
{/if}

<style>
  .qr { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; height: 100%; }

  .qr-body { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .qr-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .qr-grid { display: flex; flex-direction: column; min-height: 0; min-width: 0; }

  /* Quiet by default and accented when it is an affordance: "read-only rows" is a
     fact, "editable" is an invitation, and they should not weigh the same. */
  .qr-edit {
    flex-shrink: 0;
    padding: 1px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-disabled);
    cursor: help;
    white-space: nowrap;
  }
  .qr-edit-on {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
  }

  .qr-stats {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .qr-log { padding: 6px 0; overflow: auto; width: 100%; }
  .qr-log-line {
    display: flex;
    gap: 10px;
    padding: 1px 12px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.7;
  }
  .qr-log-time { color: var(--text-disabled); flex-shrink: 0; }
  .qr-log-error { color: var(--error); }
  .qr-log-empty { padding: 8px 12px; font-size: var(--font-size-xs); color: var(--text-disabled); font-style: italic; }
</style>
