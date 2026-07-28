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
  import { FormInput, Download } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { formatRowTotal, picusResultsStore } from '$lib/stores/picus/result.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';

  /**
   * The query tab being looked at, if the active tab is one.
   *
   * `null` for a generate or an inventory tab, and the panel then says so rather
   * than showing the last query's rows: a grid that outlived the tab it belongs
   * to is a grid nobody can tell the provenance of.
   */
  const tab = $derived(picusTabsStore.active?.kind === 'query' ? picusTabsStore.active : null);
  const state = $derived(tab ? queryStore.read(tab.id) : null);
  const result = $derived(tab ? picusResultsStore.forOwner(tab.id) : null);

  const paneTabs: TabItem[] = [
    { id: 'results', label: 'Results' },
    { id: 'messages', label: 'Messages' },
  ];

  const gridColumns = $derived<DataGridColumn[]>(
    (result?.columns ?? []).map((c) => ({
      id: c.name,
      label: c.name,
      hint: c.type,
      type: /NUMBER|INT|NUMERIC|DECIMAL/i.test(c.type) ? 'number' : 'text',
      width: 180,
    })),
  );

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

{#if !tab || !state}
  <StateBlock tone="info" fill={false} label="Open a query tab to run a statement." />
{:else}
  <div class="qr">
    <div class="qr-head">
      <Tabs
        items={paneTabs}
        value={state.pane}
        variant="underline"
        size="sm"
        ariaLabel="Result pane"
        onSelect={(id) => queryStore.setPane(tab.id, id as 'results' | 'messages')}
      />
      <span class="qr-spacer"></span>
      {#if state.running}
        <span class="qr-stats"><Spinner size={11} /> running…</span>
      {:else if result}
        <!-- The total is the server's ESTIMATE until the background count lands,
             and carries a `~` for exactly as long as that is true. Precision the
             product does not have must not be implied by the way it is printed. -->
        <span
          class="qr-stats"
          use:tooltip={result.approximate
            ? `Estimated by the planner${result.counting ? ' — counting the exact number now' : ''}. ${result.loaded.toLocaleString()} row(s) loaded.`
            : `${result.loaded.toLocaleString()} of ${result.total.toLocaleString()} row(s) loaded.`}
        >
          {formatRowTotal(result)} rows · {result.elapsedMs} ms
        </span>
      {:else if state.affected !== null}
        <span class="qr-stats">{state.affected.toLocaleString()} rows affected</span>
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
      <Button
        variant="icon"
        size="xs"
        title="Export CSV"
        ariaLabel="Export CSV"
        disabled={!result}
        onclick={() => toastStore.show('CSV export arrives with the driver milestone.', 'info')}
      >
        {#snippet iconStart()}<Download size={13} />{/snippet}
      </Button>
    </div>

    <div class="qr-body">
      {#if state.pane === 'results'}
        {#if state.error}
          <StateBlock tone="error" label={state.error} />
        {:else if result}
          <div class="qr-grid">
            {#if !result.complete}
              <!-- Said on the Results tab, not only in Messages: landing here is
                   exactly when believing you are looking at the whole thing is
                   expensive. The counter climbs as windows arrive and the notice
                   leaves of its own accord once there is nothing left to say. -->
              <div class="qr-cap">
                <Alert variant="info" compact>
                  Showing {result.loaded.toLocaleString()} of {formatRowTotal(result)} rows — the
                  rest arrives as you scroll. Sorting and the per-column filters wait until the
                  whole result is loaded: over a window they would order and hide a fraction of it
                  while looking like they had done all of it.
                </Alert>
              </div>
            {/if}
            <DataGrid
              columns={gridColumns}
              source={result ?? undefined}
              filterable
              ariaLabel="Query results"
            />
          </div>
        {:else if state.affected !== null}
          <!-- A write has no rows to show, and an empty grid would suggest it
               returned none rather than that it returns none. -->
          <StateBlock
            tone="success"
            label={`${state.affected.toLocaleString()} row(s) affected. This statement returns no rows.`}
          />
        {:else if state.hasRun}
          <StateBlock tone="info" label="The statement completed and returned no rows." />
        {:else}
          <StateBlock tone="info" label="Run the query to see its rows." />
        {/if}
      {:else}
        <div class="qr-log">
          {#each state.messages as msg, i (i)}
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

<style>
  .qr { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; height: 100%; }

  .qr-head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    flex-shrink: 0;
    padding: 0 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .qr-spacer { flex: 1; }
  .qr-body { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .qr-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  /* The "still filling" notice sits above the grid and does not scroll with it. */
  .qr-grid { display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .qr-cap { flex-shrink: 0; padding: 6px 8px 0; }

  .qr-stats {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .qr-log { padding: 6px 0; overflow: auto; width: 100%; }
  .qr-log-line {
    display: flex;
    gap: 10px;
    padding: 1px 12px;
    font-family: var(--font-code);
    font-size: 11.5px;
    line-height: 1.7;
  }
  .qr-log-time { color: var(--text-disabled); flex-shrink: 0; }
  .qr-log-error { color: var(--error); }
  .qr-log-empty { padding: 8px 12px; font-size: 11.5px; color: var(--text-disabled); font-style: italic; }
</style>
