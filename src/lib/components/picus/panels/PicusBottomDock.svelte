<script lang="ts">
  /**
   * Bottom dock — Consistency (default), Output, Changes.
   *
   * Consistency comes first because it is the panel Picus is judged on: the
   * whole product exists to stop a change from landing in one branch and not the
   * other. Output is the running log of what the tool actually did — queries,
   * scans, writes. Changes is the pending write set: which files a generation
   * would touch, before it touches them.
   */
  import { TriangleAlert, Terminal, GitCompare, RefreshCw, EyeOff, Eye } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import FindingList from './FindingList.svelte';
  import PatchDiffCard from '../generate/PatchDiffCard.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { consistencyStore, type FindingGrouping } from '$lib/stores/picus/consistency.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusUiStore, type BottomTab } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';

  let filterQuery = $state('');
  $effect(() => consistencyStore.setFilter(filterQuery));

  const tabs = $derived<TabItem[]>([
    {
      id: 'consistency',
      label: 'Consistency',
      icon: TriangleAlert,
      iconSize: 13,
      badge: consistencyStore.totalCount || undefined,
    },
    { id: 'output', label: 'Output', icon: Terminal, iconSize: 13 },
    {
      id: 'changes',
      label: 'Changes',
      icon: GitCompare,
      iconSize: 13,
      badge: dmlStore.generated ? dmlStore.enabledTargets.length : undefined,
    },
  ]);

  const groupOptions = [
    { value: 'severity', label: 'Group by severity' },
    { value: 'branch', label: 'Group by branch' },
    { value: 'file', label: 'Group by file' },
  ];

  /** Query history for the active connection — the "what did I run on staging" view. */
  const history = $derived(
    connectionsStore.activeId ? queryStore.historyFor(connectionsStore.activeId) : [],
  );
</script>

<div class="bd">
  <BottomPanelHeader onClose={() => picusUiStore.closeBottom()}>
    <Tabs
      items={tabs}
      value={picusUiStore.bottomTab}
      variant="underline"
      size="sm"
      ariaLabel="Bottom panel"
      onSelect={(id) => picusUiStore.setBottomTab(id as BottomTab)}
    />

    {#snippet actions()}
      {#if picusUiStore.bottomTab === 'consistency'}
        <div class="bd-filter">
          <SearchBar bind:query={filterQuery} showRegex={false} placeholder="Filter findings" ariaLabel="Filter findings" />
        </div>
        <Select
          value={consistencyStore.grouping}
          options={groupOptions}
          narrow
          onchange={(v) => consistencyStore.setGrouping(v as FindingGrouping)}
        />
        <Button
          variant="icon"
          size="xs"
          tooltip={consistencyStore.showSuppressed
            ? 'Hide findings silenced by a declared suppression'
            : `Show ${consistencyStore.suppressedCount} suppressed finding(s)`}
          ariaLabel="Toggle suppressed findings"
          onclick={() => consistencyStore.toggleSuppressed()}
        >
          {#snippet iconStart()}
            {#if consistencyStore.showSuppressed}<Eye size={13} />{:else}<EyeOff size={13} />{/if}
          {/snippet}
        </Button>
        <Button
          variant="icon"
          size="xs"
          tooltip={{ content: 'Re-run the consistency check', shortcut: 'Ctrl+Shift+K' }}
          ariaLabel="Re-run the consistency check"
          disabled={consistencyStore.running}
          onclick={() => consistencyStore.run()}
        >
          {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
        </Button>
      {/if}
    {/snippet}
  </BottomPanelHeader>

  <div class="bd-body">
    {#if picusUiStore.bottomTab === 'consistency'}
      {#if consistencyStore.running}
        <StateBlock tone="loading">
          {#snippet spinner()}<Spinner size={14} />{/snippet}
          <span>Running the rules over the project…</span>
        </StateBlock>
      {:else}
        <FindingList />
      {/if}

    {:else if picusUiStore.bottomTab === 'output'}
      <div class="bd-output">
        <div class="bd-output-head">
          <span>Query history</span>
          {#if connectionsStore.active}
            <Badge variant="tone" tone="neutral" size="sm" label={connectionsStore.active.name} />
          {/if}
        </div>
        {#if !history.length}
          <StateBlock tone="info" fill={false} label="Nothing has run on this connection yet." />
        {:else}
          {#each history as entry (entry.id)}
            <div class="bd-log">
              <span class="bd-log-time">{entry.at}</span>
              <span class="bd-log-sql">{entry.sql.replace(/\s+/g, ' ').slice(0, 140)}</span>
              <span class="bd-log-meta">{entry.rowCount} rows · {entry.elapsedMs} ms</span>
            </div>
          {/each}
        {/if}
      </div>

    {:else}
      <div class="bd-changes">
        {#if !dmlStore.generated}
          <StateBlock
            tone="info"
            fill={false}
            label="No pending change. Generate from the DML tab to see which files would be touched."
          />
        {:else}
          <p class="bd-changes-note">
            {dmlStore.enabledTargets.length} file{dmlStore.enabledTargets.length === 1 ? '' : 's'} would be written.
            Encoding and line endings stay as they are, and every original is copied to
            <code>.arbor/backup</code> first. Nothing is written until you confirm.
          </p>
          {#each dmlStore.enabledTargets as target (target.id)}
            <PatchDiffCard {target} sql={dmlStore.sqlFor(target)} />
          {/each}
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .bd { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .bd-body { flex: 1; min-height: 0; overflow: auto; }

  .bd-filter { width: 180px; }

  .bd-output { display: flex; flex-direction: column; }
  .bd-output-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .bd-log {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 3px 12px;
    font-family: var(--font-code);
    font-size: 11.5px;
    line-height: 1.6;
  }
  .bd-log:hover { background: var(--bg-hover); }
  .bd-log-time { color: var(--text-disabled); flex-shrink: 0; }
  .bd-log-sql { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bd-log-meta { color: var(--text-muted); flex-shrink: 0; }

  .bd-changes { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; }
  .bd-changes-note {
    font-size: 11.5px;
    line-height: 1.55;
    color: var(--text-muted);
    max-width: 90ch;
  }
  .bd-changes-note code { font-family: var(--font-code); font-size: 11px; }
</style>
