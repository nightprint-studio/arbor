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
   * ## What this file is, and what it is not
   *
   * It is the **shell**: which tab is being looked at, which pane is showing, and
   * the facts the header states about the statement. It renders none of the three
   * panes itself, and it owns no state belonging to one.
   *
   *  • `ResultRowsPane` — the grid and every decision about a *column*.
   *  • `ResultPlanPane` / `ResultPlanActions` — what the plan says, and how it is asked for.
   *  • `ResultMessagesPane` — what the statement said.
   *  • `ResultCellFileFlow` — a file going into, or a large object coming out of, one cell.
   *  • `ResultStatusBar` — the counts, the time, and whether the rows are writable.
   *
   * The grid is a **window onto a held cursor**, not a block of fetched rows: the
   * scrollbar is scaled to the result's length from the first frame and the rest
   * arrives as you approach it. That length starts as the planner's estimate and
   * is therefore marked `~` everywhere it appears, until the background count
   * replaces it with the real number.
   */
  import { Spline } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import BottomPanelFooter from '$lib/components/shared/ui/BottomPanelFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ResultExportButton from './ResultExportButton.svelte';
  import ResultStatusBar from './ResultStatusBar.svelte';
  import ResultRowsPane from './ResultRowsPane.svelte';
  import ResultPlanPane from './ResultPlanPane.svelte';
  import ResultPlanActions from './ResultPlanActions.svelte';
  import ResultMessagesPane from './ResultMessagesPane.svelte';
  import ResultCellFileFlow from './ResultCellFileFlow.svelte';
  import ColumnLineagePane from './ColumnLineagePane.svelte';
  import { originsFromLineage } from './column-lineage';
  import { picusLineageStore } from '$lib/stores/picus/lineage.svelte';
  import { isSessionOpen } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { queryStore, type ResultPane } from '$lib/stores/picus/query.svelte';
  import { picusPlanStore } from '$lib/stores/picus/plan.svelte';
  import { picusProvidersStore } from '$lib/stores/picus/providers.svelte';
  import { picusResultsStore } from '$lib/stores/picus/result.svelte';
  import { editability, resultEditStore } from '$lib/stores/picus/result-edit.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';

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
    // Always offered, unlike Plan: tracing reads the views' SQL rather than asking
    // the engine anything, so there is no capability it could be missing.
    { id: 'lineage', label: 'Lineage' },
  ]);

  // ── Where the columns really come from ──────────────────────────────────────

  const lineageState = $derived(tab ? picusLineageStore.read(tab.id) : null);

  /**
   * The trace, but only while it is about the result on screen.
   *
   * A new result makes the old chain stale, and a stale chain is worse than none —
   * it names tables for columns that may not exist any more. Read as a condition
   * rather than cleared by an effect: an effect keyed on the active tab would fire
   * when you merely *switched* to a tab and throw away a trace taken on it earlier.
   *
   * Stale is treated as absent, so the pane offers Trace again. Not re-run on the
   * user's behalf: tracing is a question they asked, and asking it again every time
   * they press Run would turn a deliberate action into a cost they never chose.
   */
  const freshLineage = $derived(
    lineageState?.lineage && lineageState.resultId === (result?.resultId ?? '')
      ? lineageState.lineage
      : null,
  );

  /**
   * Trace the statement the caret is in — **not** the tab's whole buffer.
   *
   * From `lastRun` — the statement that actually **produced these rows** — and not
   * from the caret. The distinction is the whole point: a lineage is a question
   * about the result on screen, and the caret has been free to move since the run.
   * Reading it from the caret asks the *buffer* a question that belongs to the
   * *result*, which is the same trap `lastRun` was created for when storing an
   * edited cell came back showing the first statement in the tab.
   * Passing the buffer was a real bug with a memorable symptom: the parser took the
   * first `SELECT` in the file, which on a scratchpad holding several queries was
   * some older one — a nine-way join whose `*` expanded to four hundred columns for
   * a view that has thirty-eight.
   */
  const ranSql = $derived.by(() => {
    const targets = tabState?.lastRun?.targets ?? [];
    // Exactly one. A run of several statements leaves one result on screen and no
    // way to say which of them these rows came from, so there is nothing honest to
    // trace.
    return targets.length === 1 ? targets[0].sql : '';
  });

  /** Nothing to trace without an open session and a statement that produced rows. */
  const canTrace = $derived(!!tab && isSessionOpen(conn) && !!result && !!ranSql);
  const traceReason = $derived(
    !isSessionOpen(conn)
      ? 'Connect this tab to a database to trace where its columns come from.'
      : !result
        ? 'Run a statement first — tracing follows the columns a result actually has.'
        : 'This tab ran more than one statement, so there is no single one these rows came from.',
  );

  /**
   * The traced colouring for the grid, or `null` while nothing has been traced.
   *
   * `visibleColumnCount` is recomputed here rather than passed down because the rows
   * pane owns that arithmetic and this needs the same number — one line of agreement
   * beats a prop that could drift.
   */
  const tracedOrigins = $derived(
    originsFromLineage(
      freshLineage,
      Math.max(0, (result?.columns.length ?? 0) - (result?.hiddenColumns?.length ?? 0)),
    ),
  );
  const traced = $derived(freshLineage ? tracedOrigins : null);


  function trace() {
    if (!tab || !conn || !ranSql) return;
    void picusLineageStore.trace(tab.id, conn.id, ranSql, result?.resultId ?? '');
  }

  /**
   * The plan of the statement this tab is pointing at.
   *
   * Its own store, keyed by the same tab: a plan is about a *statement*, so it
   * survives the result being closed and exists for a statement that has never been
   * run. Nothing here reaches into the grid's state and nothing there reaches into
   * this.
   */
  const planState = $derived(tab ? picusPlanStore.read(tab.id) : null);

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
   * The file flow, held so the grid's context menu can call into it.
   *
   * `bind:this` rather than a prop of callbacks: the three operations are verbs the
   * user has just chosen, and the component that owns their dialogs is the one that
   * should own how they start.
   *
   * Typed as the methods used rather than as the component — the convention
   * elsewhere in Arbor — so this states exactly what it depends on, and a method
   * being removed over there is an error here rather than a silent no-op.
   */
  type CellFileFlow = {
    reveal: (rowIndex: number, column: string) => void;
    replaceLob: (rowIndex: number, column: string) => void;
    loadText: (rowIndex: number, column: string) => void;
  };
  let files = $state<CellFileFlow | null>(null);
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
    <!-- No line under the header while the rows are showing: that pane floats its
         content — an inset legend, a grid held off the edge — so the gap already
         draws the boundary. The other panes butt straight against it and want the
         line. -->
    <BottomPanelHeader
      title="Results"
      divider={tabState.pane !== 'results'}
      onClose={() => picusUiStore.closeBottom()}
    >
      <Tabs
        items={paneTabs}
        value={tabState.pane}
        variant="underline"
        size="sm"
        ariaLabel="Result pane"
        onSelect={(id) => queryStore.setPane(tab.id, id as ResultPane)}
      />
      <!-- Verbs only. The counts and the time used to sit here beside them, which
           put "what happened" and "what you can do" in one strip and left the header
           with nothing to give once a fourth pane and a third action arrived. They
           are facts, and facts are in the footer. -->
      {#snippet actions()}
      {#if canExplain}
        <ResultPlanActions tabId={tab.id} {conn} busy={planState?.running ?? false} />
      {/if}
      <!-- Beside Explain and Analyze rather than anywhere else: all three ask
           something *about the statement that produced these rows*, and splitting
           them across the panel would make them read as different kinds of thing. -->
      <Button
        variant="icon"
        size="xs"
        tooltip={canTrace
          ? 'Trace where these columns come from — follow each one through the views to its table.'
          : traceReason}
        ariaLabel="Trace where these columns come from"
        disabled={!canTrace || (lineageState?.running ?? false)}
        onclick={() => { queryStore.setPane(tab.id, 'lineage'); trace(); }}
      >
        {#snippet iconStart()}<Spline size={13} />{/snippet}
      </Button>
      <!-- "Generate DML from this result" was here. Taken out rather than fixed: it
           never carried the result's identity across — it re-set the generator's
           table to whatever the generator already had and reported success — so what
           it actually offered was a button that opened a panel and lied about why.
           It comes back when the result knows which table it came from. -->
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
          <ResultRowsPane
            {result}
            {editable}
            {traced}
            onReveal={(row, column) => files?.reveal(row, column)}
            onReplaceLob={(row, column) => files?.replaceLob(row, column)}
            onLoadText={(row, column) => files?.loadText(row, column)}
          />
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
        <ResultPlanPane {planState} />
      {:else if tabState.pane === 'lineage'}
        <ColumnLineagePane
          state={{
            lineage: freshLineage,
            running: lineageState?.running ?? false,
            error: lineageState?.error ?? '',
            sql: lineageState?.sql ?? '',
            resultId: lineageState?.resultId ?? '',
          }}
          disabled={!canTrace}
          reason={traceReason}
          onTrace={() => void trace()}
        />
      {:else}
        <ResultMessagesPane messages={tabState.messages} />
      {/if}
    </div>

    <!-- The facts about this result, where a fact is looked for. Outside the pane
         switch on purpose: how many rows there are and how long they took is true
         whichever pane you are reading, and a strip that emptied when you opened the
         plan would read as the result having gone away. -->
    <BottomPanelFooter>
      <ResultStatusBar {tabState} {result} {editable} />
    </BottomPanelFooter>
  </div>
{/if}

<!-- Outside the tab check, and outside the pane switch, exactly where the dialogs it
     replaced were: a file picker or a large-object viewer opened from the grid must
     not be torn down because the user looked at the plan, or stepped onto another
     tab, while it was up. -->
<ResultCellFileFlow
  bind:this={files}
  tabId={tab?.id ?? ''}
  {conn}
  {result}
  {editable}
  {sourceTable}
/>

<style>
  .qr { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; height: 100%; }

  .qr-body { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .qr-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }
</style>
