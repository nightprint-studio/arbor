<script lang="ts">
  /**
   * Query editor — write SQL, run it against the tab's connection, read the
   * result.
   *
   * The bar above the editor never lets you forget which database you are on:
   * name, colour, schema@host, dialect, and a lock when the session refuses
   * writes. Results and server messages are two panes of the same footer, so a
   * failed statement's reason is one click from the grid that didn't fill.
   *
   * From a result you can jump straight into the generator with those columns —
   * the bridge between querying a database and writing scripts for it.
   */
  import { Play, Square, FormInput, Download, Lock, History } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import { sqlLanguage } from '../picus-sql-language';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import type { PicusTab } from '$lib/types/picus';

  interface Props {
    tab: PicusTab;
  }

  let { tab }: Props = $props();

  const conn = $derived(picusTabsStore.activeConnection);
  // `read` is pure; materialising the record is a write, so it happens in an
  // effect (a write during `$derived` evaluation is a Svelte 5 hard error).
  $effect(() => { queryStore.ensure(tab.id); });
  const state = $derived(queryStore.read(tab.id));
  const language = $derived(sqlLanguage(conn?.dialect));

  const paneTabs: TabItem[] = [
    { id: 'results', label: 'Results' },
    { id: 'messages', label: 'Messages' },
  ];

  const gridColumns = $derived<DataGridColumn[]>(
    (state.result?.columns ?? []).map((c) => ({
      id: c.name,
      label: c.name,
      hint: c.type,
      type: /NUMBER|INT|NUMERIC|DECIMAL/i.test(c.type) ? 'number' : 'text',
      width: 180,
    })),
  );

  /** Prefill the generator with this result's table and columns. */
  function toGenerator() {
    const first = state.result?.columns[0];
    if (!first) return;
    // MOCK: the real bridge carries the result's table identity from the server.
    dmlStore.setTable(dmlStore.table);
    picusTabsStore.openGenerate();
    picusUiStore.showSection('generate');
    toastStore.show('Generator prefilled from this result.', 'success');
  }
</script>

<div class="qv">
  <!-- Which database this tab talks to — always visible, never inferred. -->
  <div class="qv-bar">
    {#if conn}
      <span class="qv-dot" style:background={connectionColorVar(conn)}></span>
      <span class="qv-name">{conn.name}</span>
      <span class="qv-host">{conn.schema}@{conn.host}</span>
      <PicusDialectChip dialect={conn.dialect} />
      <span class="qv-spacer"></span>
      <Badge variant="tone" tone="neutral" size="sm" label={`db ${conn.dbVersion}`} />
      {#if conn.readOnly}
        <span class="qv-ro" use:tooltip={'The backend refuses write statements on this connection'}>
          <Lock size={11} /> read-only
        </span>
      {/if}
      <Button
        variant="primary"
        size="xs"
        disabled={state.running}
        tooltip={{ content: 'Run the statement under the cursor', shortcut: 'Ctrl+Enter' }}
        onclick={() => queryStore.run(tab.id, conn.id)}
      >
        {#snippet iconStart()}<Play size={12} />{/snippet}
        Run
      </Button>
      {#if state.running}
        <Button variant="secondary" size="xs" onclick={() => queryStore.cancel(tab.id)}>
          {#snippet iconStart()}<Square size={12} />{/snippet}
          Cancel
        </Button>
      {/if}
    {:else}
      <span class="qv-none">This tab is not bound to a connection.</span>
    {/if}
  </div>

  <div class="qv-editor">
    <CodeEditor
      value={state.sql}
      {language}
      oninput={(v) => queryStore.setSql(tab.id, v)}
    />
  </div>

  <ResizablePanel direction="vertical" initialSize={260} minSize={120} maxSize={620} reverse>
    <div class="qv-result">
      <div class="qv-result-head">
        <Tabs
          items={paneTabs}
          value={state.pane}
          variant="underline"
          size="sm"
          ariaLabel="Result pane"
          onSelect={(id) => queryStore.setPane(tab.id, id as 'results' | 'messages')}
        />
        <span class="qv-spacer"></span>
        {#if state.running}
          <span class="qv-running"><Spinner size={11} /> running…</span>
        {:else if state.result}
          <span class="qv-stats">{state.result.rowCount} rows · {state.result.elapsedMs} ms</span>
        {/if}
        <Button
          variant="icon"
          size="xs"
          tooltip={'Generate DML from this result'}
          ariaLabel="Generate DML from this result"
          disabled={!state.result}
          onclick={toGenerator}
        >
          {#snippet iconStart()}<FormInput size={13} />{/snippet}
        </Button>
        <Button
          variant="icon"
          size="xs"
          title="Export CSV"
          ariaLabel="Export CSV"
          disabled={!state.result}
          onclick={() => toastStore.show('CSV export arrives with the driver milestone.', 'info')}
        >
          {#snippet iconStart()}<Download size={13} />{/snippet}
        </Button>
        <Button
          variant="icon"
          size="xs"
          title="Query history for this connection"
          ariaLabel="Query history"
          onclick={() => picusUiStore.showBottom('output')}
        >
          {#snippet iconStart()}<History size={13} />{/snippet}
        </Button>
      </div>

      <div class="qv-result-body">
        {#if state.pane === 'results'}
          {#if state.error}
            <StateBlock tone="error" label={state.error} />
          {:else if !state.result}
            <StateBlock tone="info" label="Run the query to see its rows." />
          {:else}
            <DataGrid
              columns={gridColumns}
              rows={state.result.rows}
              filterable
              ariaLabel="Query results"
            />
          {/if}
        {:else}
          <div class="qv-log">
            {#each state.messages as msg, i (i)}
              <div class="qv-log-line" class:qv-log-error={msg.level === 'error'}>
                <span class="qv-log-time">{msg.time}</span>
                <span>{msg.text}</span>
              </div>
            {:else}
              <p class="qv-log-empty">No message yet.</p>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </ResizablePanel>
</div>

<style>
  .qv { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }

  .qv-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    flex-shrink: 0;
    padding: 0 10px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .qv-dot { width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; }
  .qv-name { font-weight: 500; }
  .qv-host { color: var(--text-muted); font-family: var(--font-code); font-size: 10.5px; }
  .qv-none { color: var(--text-disabled); font-style: italic; }
  .qv-spacer { flex: 1; }
  .qv-ro {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--warning);
    font-size: 11px;
  }

  .qv-editor { flex: 1; min-height: 90px; display: flex; overflow: hidden; }
  .qv-editor > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .qv-result {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    border-top: 1px solid var(--border);
  }
  .qv-result-head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    flex-shrink: 0;
    padding: 0 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .qv-result-body { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .qv-result-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .qv-stats, .qv-running {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .qv-log { padding: 6px 0; overflow: auto; width: 100%; }
  .qv-log-line {
    display: flex;
    gap: 10px;
    padding: 1px 12px;
    font-family: var(--font-code);
    font-size: 11.5px;
    line-height: 1.7;
  }
  .qv-log-time { color: var(--text-disabled); flex-shrink: 0; }
  .qv-log-error { color: var(--error); }
  .qv-log-empty { padding: 8px 12px; font-size: 11.5px; color: var(--text-disabled); font-style: italic; }
</style>
