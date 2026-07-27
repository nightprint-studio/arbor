<script lang="ts">
  /**
   * Picus toolbar — the contextual strip under the tab bar.
   *
   * Its contents depend entirely on what the active tab is: the generator gets a
   * source switch and Generate/Write, a query gets Run/Cancel and the connection
   * it runs against, a table gets its sub-views, a file gets Save/Diff. The two
   * constants are the right-hand info cluster (what the tab currently amounts
   * to) and, for anything bound to a database, the connection selector.
   *
   * Rebinding a query tab to another connection is an explicit act with a
   * visible control, never a hidden global mode.
   */
  import {
    Play, Square, Save, GitCompare, Download, Plus, FormInput, RefreshCw, Check, Search,
  } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import PicusConnectionPill from '../PicusConnectionPill.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { DML_OPERATION_LABELS } from '$lib/types/picus';

  interface Props {
    /** Bubbles up to the shell, which owns the confirm dialog. */
    onGenerate: () => void;
    onWrite: () => void;
  }

  let { onGenerate, onWrite }: Props = $props();

  const tab = $derived(picusTabsStore.active);
  const kind = $derived(tab?.kind ?? null);
  const conn = $derived(picusTabsStore.activeConnection);

  // ── Connection selector (tabs bound to a database) ──────────────────────────
  const connectionMenu = $derived<DropdownItem[]>(
    connectionsStore.connections.map((c) => ({
      kind: 'item',
      id: c.id,
      label: c.name,
      subtitle: `${c.alias} · ${c.schema}`,
      iconColor: connectionColorVar(c),
      active: c.id === conn?.id,
      onclick: () => {
        if (tab) picusTabsStore.setTabConnection(tab.id, c.id);
      },
    })),
  );

  const tableTabs: TabItem[] = [
    { id: 'data', label: 'Data' },
    { id: 'structure', label: 'Structure' },
    { id: 'ddl', label: 'DDL' },
  ];

  const queryState = $derived(tab ? queryStore.read(tab.id) : null);
  // The server's row ESTIMATE, not a count — a toolbar label is not worth scanning
  // a large table for. `null` when the server has none, so "unknown" and "empty"
  // never render the same.
  const tableRowCount = $derived(
    tab?.table ? (schemaStore.relation(tab.table)?.estimatedRows ?? null) : null,
  );
  const openFile = $derived(tab?.file ? picusProjectStore.fileByPath(tab.file) : null);

  /** Tables and views have rows and a structure; sequences and triggers don't. */
  const hasSubviews = $derived(tab?.objectKind === 'table' || tab?.objectKind === 'view' || (kind === 'table' && !tab?.objectKind));
  /** DML is only ever written against a real table. */
  const isWritableTable = $derived(tab?.objectKind === 'table' || (kind === 'table' && !tab?.objectKind));

  function notYet(what: string) {
    toastStore.show(`${what} arrives with the backend milestone.`, 'info');
  }
</script>

<div class="ptb" role="toolbar" aria-label="Document actions" tabindex="-1">
  {#if kind === 'generate'}
    <!-- The source switch lives on the Source card only: one control, one home.
         Duplicating it here made the same choice reachable from two places that
         then had to be kept in step. -->
    <Button
      variant="ghost"
      size="sm"
      disabled={!dmlStore.canGenerate}
      tooltip={{ content: 'Generate the SQL for every enabled target', shortcut: 'Ctrl+G' }}
      ariaLabel="Generate"
      onclick={onGenerate}
    >
      {#snippet iconStart()}<Play size={13} />{/snippet}
      Generate
    </Button>
    <Button
      variant="ghost"
      size="sm"
      disabled={!dmlStore.generated || dmlStore.applied}
      tooltip={{ content: 'Write the generated SQL into the scripts', shortcut: 'Ctrl+Shift+W' }}
      ariaLabel="Write to scripts"
      onclick={onWrite}
    >
      {#snippet iconStart()}<Check size={13} />{/snippet}
      Write
    </Button>
    <Button variant="icon" size="sm" title="Export as .sql" ariaLabel="Export as .sql" onclick={() => notYet('Export')}>
      {#snippet iconStart()}<Download size={14} />{/snippet}
    </Button>

    <span class="ptb-spacer"></span>
    <div class="ptb-info">
      <span>{dmlStore.table}</span>
      <span class="ptb-dot">·</span>
      <span>{DML_OPERATION_LABELS[dmlStore.operation]}</span>
      <span class="ptb-dot">·</span>
      <span>{dmlStore.enabledTargets.length} of {dmlStore.targets.length} targets</span>
      <span class="ptb-dot">·</span>
      <span>{dmlStore.rows.length} row{dmlStore.rows.length === 1 ? '' : 's'}</span>
    </div>

  {:else if kind === 'query'}
    <Button
      variant="ghost"
      size="sm"
      disabled={queryState?.running}
      tooltip={{ content: 'Run the statement under the cursor', shortcut: 'Ctrl+Enter' }}
      ariaLabel="Run"
      onclick={() => { if (tab && conn) void queryStore.run(tab.id, conn.id); }}
    >
      {#snippet iconStart()}<Play size={13} />{/snippet}
      Run
    </Button>
    <Button
      variant="icon"
      size="sm"
      disabled={!queryState?.running}
      tooltip={{ content: 'Cancel the running query', shortcut: 'Ctrl+Shift+C' }}
      ariaLabel="Cancel"
      onclick={() => { if (tab && conn) void queryStore.cancel(tab.id, conn.id); }}
    >
      {#snippet iconStart()}<Square size={13} />{/snippet}
    </Button>
    <Button variant="icon" size="sm" title="Save script" ariaLabel="Save script" onclick={() => notYet('Saving a query')}>
      {#snippet iconStart()}<Save size={14} />{/snippet}
    </Button>

    <span class="ptb-spacer"></span>
    {#if queryState?.result}
      <div class="ptb-info">
        <span>{queryState.result.rowCount} rows</span>
        <span class="ptb-dot">·</span>
        <span>{queryState.result.elapsedMs} ms</span>
      </div>
    {/if}
    <Dropdown items={connectionMenu} position="fixed" direction="down" width="280px">
      {#snippet trigger({ open, toggle })}
        <PicusConnectionPill connection={conn} density="toolbar" {open} onclick={toggle} />
      {/snippet}
    </Dropdown>

  {:else if kind === 'table'}
    <!-- Sub-views exist for things with rows; a sequence or a trigger has only
         its properties, so the switch would be three tabs of nothing. -->
    {#if hasSubviews}
      <Tabs
        items={tableTabs}
        value={picusUiStore.tableSubview}
        variant="pill"
        size="sm"
        ariaLabel="Object view"
        onSelect={(id) => picusUiStore.setTableSubview(id as 'data' | 'structure' | 'ddl')}
      />
      <span class="ptb-sep"></span>
    {/if}
    {#if isWritableTable}
      <Button variant="icon" size="sm" title="New row" ariaLabel="New row" disabled={conn?.readOnly} onclick={() => notYet('Inline editing')}>
        {#snippet iconStart()}<Plus size={14} />{/snippet}
      </Button>
      <Button
        variant="icon"
        size="sm"
        tooltip={'Generate DML from this table — prefills the generator with its columns'}
        ariaLabel="Generate DML from this table"
        onclick={() => {
          if (tab?.table) dmlStore.setTable(tab.table);
          picusTabsStore.openGenerate();
          picusUiStore.showSection('generate');
        }}
      >
        {#snippet iconStart()}<FormInput size={14} />{/snippet}
      </Button>
    {/if}
    {#if hasSubviews}
      <Button variant="icon" size="sm" title="Export CSV" ariaLabel="Export CSV" onclick={() => notYet('CSV export')}>
        {#snippet iconStart()}<Download size={14} />{/snippet}
      </Button>
    {/if}
    <Button variant="icon" size="sm" title="Refresh the schema cache" ariaLabel="Refresh the schema cache" onclick={() => void schemaStore.refresh()}>
      {#snippet iconStart()}<RefreshCw size={14} />{/snippet}
    </Button>

    <span class="ptb-spacer"></span>
    {#if hasSubviews && tableRowCount != null}
      <div class="ptb-info"><span>~{tableRowCount.toLocaleString()} rows</span></div>
    {/if}
    <Dropdown items={connectionMenu} position="fixed" direction="down" width="280px">
      {#snippet trigger({ open, toggle })}
        <PicusConnectionPill connection={conn} density="toolbar" {open} onclick={toggle} />
      {/snippet}
    </Dropdown>

  {:else if kind === 'file'}
    <Button variant="icon" size="sm" title="Save (preserves encoding + line endings)" ariaLabel="Save" onclick={() => notYet('Saving')}>
      {#snippet iconStart()}<Save size={14} />{/snippet}
    </Button>
    <Button variant="icon" size="sm" title="Compare with the other branch" ariaLabel="Compare with the other branch" onclick={() => notYet('Branch comparison')}>
      {#snippet iconStart()}<GitCompare size={14} />{/snippet}
    </Button>
    <Button variant="icon" size="sm" title="Find in file" ariaLabel="Find in file" onclick={() => notYet('Find in file')}>
      {#snippet iconStart()}<Search size={14} />{/snippet}
    </Button>

    <span class="ptb-spacer"></span>
    {#if openFile}
      <div class="ptb-info">
        <span>{(openFile.size / 1024).toFixed(1)} KB</span>
        <span class="ptb-dot">·</span>
        <span>{openFile.encoding}</span>
        <span class="ptb-dot">·</span>
        <span>{openFile.eol}</span>
      </div>
    {/if}

  {:else if kind === 'inventory'}
    <Button variant="icon" size="sm" title="Re-index the project" ariaLabel="Re-index the project" onclick={() => notYet('Re-indexing')}>
      {#snippet iconStart()}<RefreshCw size={14} />{/snippet}
    </Button>
    <span class="ptb-spacer"></span>
    <div class="ptb-info">
      <span>{picusProjectStore.inventory.length} objects</span>
      <span class="ptb-dot">·</span>
      <span>{picusProjectStore.branches.length} branches</span>
    </div>
  {/if}
</div>

<style>
  .ptb {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 32px;
    flex-shrink: 0;
    padding: 0 8px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
  }
  .ptb-spacer { flex: 1; }
  .ptb-sep {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--border-subtle);
    flex-shrink: 0;
  }
  .ptb-info {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .ptb-dot { color: var(--text-disabled); }
</style>
