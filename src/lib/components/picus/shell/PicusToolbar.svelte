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
    Play, Square, Save, GitCompare, Download, Plus, FormInput, RefreshCw, Check,
    ListOrdered, Lock,
  } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import PicusConnectionPill from '../PicusConnectionPill.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import { picusEditorStore } from '$lib/stores/picus/editor.svelte';
  import { saveOpenScript } from '../save-script';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { connectionsStore, connectionColorVar } from '$lib/stores/picus/connections.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { formatElapsed, queryStore } from '$lib/stores/picus/query.svelte';
  import { picusResultsStore } from '$lib/stores/picus/result.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { schemaStore } from '$lib/stores/picus/schema.svelte';
  import { DML_OPERATION_LABELS, declaredEngine, folderEngine } from '$lib/types/picus';

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
  // How long the result is belongs to the STATUS BAR, in one place, for query
  // tabs and table tabs alike — a second copy here would have to be kept in step
  // with it, and the copy this toolbar used to draw came from the schema cache's
  // estimate rather than from the result on screen, so the two could disagree.
  // What stays is the statement's own timing, which is not a fact about length.
  const result = $derived(tab ? picusResultsStore.forOwner(tab.id) : null);
  const openFile = $derived(tab?.file ? picusProjectStore.fileByPath(tab.file) : null);
  /** The folder the open file lives in — where its engine and its role come from. */
  const openFolder = $derived(tab?.file ? picusProjectStore.folderOfFile(tab.file) : null);

  /** Tables and views have rows and a structure; sequences and triggers don't. */
  const hasSubviews = $derived(tab?.objectKind === 'table' || tab?.objectKind === 'view' || (kind === 'table' && !tab?.objectKind));
  /** DML is only ever written against a real table. */
  const isWritableTable = $derived(tab?.objectKind === 'table' || (kind === 'table' && !tab?.objectKind));

  function notYet(what: string) {
    toastStore.show(`${what} arrives with the backend milestone.`, 'info');
  }

  // ── Saving a script ─────────────────────────────────────────────────────────
  //
  // The buffer lives in the editor, which registers itself with `picusEditorStore`
  // — the same registration Ctrl+F already goes through. Reading it here rather
  // than threading the text up through the view keeps one owner of "what is in the
  // editor right now".
  const fileText = $derived(tab?.file ? picusProjectStore.textFor(tab.file) : null);
  const fileDirty = $derived(
    !!tab?.file && !!fileText && picusEditorStore.active?.getValue() !== fileText.text,
  );
  let saving = $state(false);

  /** The button's half of Save: the busy state. The verb itself is shared with Ctrl+S. */
  async function saveFile() {
    const path = tab?.file;
    if (!path || saving) return;
    saving = true;
    await saveOpenScript(path);
    saving = false;
  }

  /** Run, and reveal the answer — the dock is where every answer in this window is. */
  function runQuery(scope: 'statement' | 'buffer') {
    if (!tab || !conn) return;
    picusUiStore.showBottom('results');
    void queryStore.run(tab.id, conn.id, scope);
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
      tooltip={{
        content: 'Run the selection, or the statement under the cursor',
        shortcut: 'Ctrl+Enter',
      }}
      ariaLabel="Run"
      onclick={() => runQuery('statement')}
    >
      {#snippet iconStart()}<Play size={13} />{/snippet}
      Run
    </Button>
    <Button
      variant="icon"
      size="sm"
      disabled={queryState?.running}
      tooltip={{
        content: 'Run every statement in this tab, in order, stopping at the first failure',
        shortcut: 'Ctrl+Shift+Enter',
      }}
      ariaLabel="Run all"
      onclick={() => runQuery('buffer')}
    >
      {#snippet iconStart()}<ListOrdered size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="sm"
      disabled={!queryState?.running && !result?.counting}
      tooltip={{ content: 'Cancel the running query, or the row count behind it', shortcut: 'Ctrl+Shift+C' }}
      ariaLabel="Cancel"
      onclick={() => { if (tab && conn) void queryStore.cancel(tab.id, conn.id); }}
    >
      {#snippet iconStart()}<Square size={13} />{/snippet}
    </Button>
    <Button variant="icon" size="sm" title="Save script" ariaLabel="Save script" onclick={() => notYet('Saving a query')}>
      {#snippet iconStart()}<Save size={14} />{/snippet}
    </Button>

    <span class="ptb-spacer"></span>
    <!-- Which database this tab talks to — always visible, never inferred. This
         used to be a second bar of its own under the editor, which meant two Run
         buttons and two Cancels for one action; what was worth keeping from it was
         the identity, so it moved here. -->
    {#if conn}
      <div class="ptb-info">
        <!-- The engine and the installed version; the host and schema are in the
             pill's own tooltip beside them. `public@localhost:5432/postgres`
             used to be spelled out here, next to a chip already saying
             PostgreSQL and a pill already saying which connection this is —
             three ways of naming one database, on one row, none of them the
             thing you look at this bar to check. -->
        <PicusDialectChip engine={conn.dialect} />
        {#if conn.dbVersion}
          <Badge variant="tone" tone="neutral" size="sm" label={`db ${conn.dbVersion}`} />
        {/if}
        {#if conn.readOnly}
          <span class="ptb-ro" use:tooltip={'The backend refuses write statements on this connection'}>
            <Lock size={11} /> read-only
          </span>
        {/if}
        <!-- Said where the button that started it is. A statement in flight showed
             nothing at all up here — Run merely went grey — while the only sign of
             life was a small spinner in the dock's own header, which is a
             different panel and often a closed one. -->
        {#if queryState?.running}
          <span class="ptb-dot">·</span>
          <span class="ptb-running"><Spinner size={11} /> running…</span>
        {:else if result}
          <span class="ptb-dot">·</span>
          <span>{formatElapsed(result.elapsedMs)}</span>
        {/if}
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
    <Dropdown items={connectionMenu} position="fixed" direction="down" width="280px">
      {#snippet trigger({ open, toggle })}
        <PicusConnectionPill connection={conn} density="toolbar" {open} onclick={toggle} />
      {/snippet}
    </Dropdown>

  {:else if kind === 'file'}
    <Button
      variant="ghost"
      size="sm"
      disabled={!fileDirty || saving}
      tooltip={{
        content: 'Write this file back in its own encoding and line endings, then re-check the repository',
        shortcut: 'Ctrl+S',
      }}
      ariaLabel="Save"
      onclick={() => void saveFile()}
    >
      {#snippet iconStart()}
        {#if saving}<Spinner size={13} />{:else}<Save size={14} />{/if}
      {/snippet}
      Save
    </Button>
    <Button variant="icon" size="sm" title="Compare with the other engine's version of this change" ariaLabel="Compare with the other engine's version" onclick={() => notYet('Cross-engine comparison')}>
      {#snippet iconStart()}<GitCompare size={14} />{/snippet}
    </Button>
    <!-- No magnifier: Ctrl+F opens the editor's own panel, and a button that
         duplicates a binding everyone already has is a button that has to be kept
         in step with it. -->

    <span class="ptb-spacer"></span>
    <!-- The file's own facts, which used to be a SECOND bar under this one:
         path, engine, encoding, line ending, and whether the encoding has drifted
         from what the folder declares. Two rows of chrome above an editor, saying
         things that fit on one. -->
    {#if openFile}
      <span class="ptb-path" use:tooltip={openFile.path}>{openFile.path}</span>
    {/if}
    {#if openFolder}
      <!-- Which folder decides this file's engine, and whether that folder says so
           itself or inherits it. The chip is the shortest honest answer to "what
           dialect am I editing", which has no global answer in Picus. -->
      <div class="ptb-info">
        <PicusDialectChip
          engine={folderEngine(openFolder.node)}
          inherited={declaredEngine(openFolder.node) === null}
          from={openFolder.dialectFrom ?? ''}
        />
        <PicusRoleChip
          role={openFolder.node.effectiveRole}
          inherited={openFolder.node.role === null}
          from={openFolder.roleFrom ?? ''}
        />
        <span class="ptb-dot">·</span>
      </div>
    {/if}
    {#if openFile}
      <div class="ptb-info">
        <span>{(openFile.size / 1024).toFixed(1)} KB</span>
        <span class="ptb-dot">·</span>
        <EncodingPill
          encoding={fileText?.encoding ?? openFile.encoding}
          expected={openFile.expectedEncoding}
          eol={fileText?.eol ?? openFile.eol}
          compact
        />
      </div>
    {/if}

  {:else if kind === 'inventory'}
    <Button
      variant="icon"
      size="sm"
      tooltip={{ content: 'Re-index and re-check the repository', shortcut: 'Ctrl+Shift+K' }}
      ariaLabel="Re-index the repository"
      disabled={!picusProjectStore.attached || picusProjectStore.analyzing}
      onclick={() => void picusProjectStore.analyze()}
    >
      {#snippet iconStart()}<RefreshCw size={14} />{/snippet}
    </Button>
    <span class="ptb-spacer"></span>
    <div class="ptb-info">
      <span>{picusProjectStore.inventory.length} objects</span>
      <span class="ptb-dot">·</span>
      <span>{picusProjectStore.folderCount} folders</span>
    </div>
  {/if}
</div>

<style>
  /* A floating strip, not a band welded to the card's edges.
     It is elevated because it is furniture rather than document — but it used to
     run wall to wall, so the grey met the card's rounded corners and the whole
     top of the window read as one grey mass. A few pixels of `bg-base` around it
     is what makes it a toolbar sitting on a page instead of a stripe painted
     across one; same reason the panels themselves float inside the rails. */
  .ptb {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 32px;
    flex-shrink: 0;
    margin: 3px 6px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
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
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .ptb-dot { color: var(--text-disabled); }
  .ptb-running { display: inline-flex; align-items: center; gap: 5px; color: var(--text-secondary); }
  /* Elided from the left: the tail of a script path is what identifies it, and the
     head is the same six folders on every file in the repository. */
  .ptb-path {
    min-width: 0;
    max-width: 46ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
  }
  .ptb-ro {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--warning);
  }
</style>
