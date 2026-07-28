<script lang="ts">
  /**
   * Bottom dock — Consistency (default), Output, Changes.
   *
   * Consistency comes first because it is the panel Picus is judged on: the
   * whole product exists to stop a change from landing in one engine's scripts and not the
   * other. Output is the running log of what the tool actually did — queries,
   * scans, writes. Changes is the pending write set: which files a generation
   * would touch, before it touches them.
   */
  import { untrack } from 'svelte';
  import { TriangleAlert, Terminal, GitCompare, RefreshCw, EyeOff, Eye } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FindingList from './FindingList.svelte';
  import PatchDiffCard from '../generate/PatchDiffCard.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { consistencyStore, type FindingGrouping } from '$lib/stores/picus/consistency.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore, type BottomTab } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';

  let filterQuery = $state('');
  $effect(() => consistencyStore.setFilter(filterQuery));

  /**
   * Build the preview when the Changes tab is the one being looked at.
   *
   * It reads disk, so it is not run on every keystroke of the form — only when
   * somebody is actually looking at what would be written, or when the write
   * action asks for it. `ensurePreview` is self-guarding, so this effect can
   * simply watch the payload key and fire: a landed preview does not re-trigger
   * it, and neither does a failed one.
   */
  $effect(() => {
    if (picusUiStore.bottomTab !== 'changes' || !dmlStore.generated) return;
    void dmlStore.previewKey;
    untrack(() => void dmlStore.ensurePreview());
  });

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
      badge: dmlStore.previewFiles.length || undefined,
    },
  ]);

  /** The destination a previewed file belongs to — for its dialect and role chips. */
  function targetFor(path: string) {
    return dmlStore.targets.find((t) => t.file === path) ?? null;
  }

  const groupOptions = [
    { value: 'severity', label: 'Group by severity' },
    // Folder rather than "branch": there is no branch level any more, and the
    // folder is where a finding's engine and role actually come from.
    { value: 'folder', label: 'Group by folder' },
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
          disabled={consistencyStore.running || !picusProjectStore.attached}
          onclick={() => void picusProjectStore.analyze()}
        >
          {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
        </Button>
      {:else if picusUiStore.bottomTab === 'changes'}
        <Button
          variant="icon"
          size="xs"
          tooltip={'Re-read the destinations from disk and recompute the patch'}
          ariaLabel="Rebuild the preview"
          disabled={dmlStore.previewing || !dmlStore.generated}
          onclick={() => void dmlStore.rebuildPreview()}
        >
          {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
        </Button>
      {/if}
    {/snippet}
  </BottomPanelHeader>

  <div class="bd-body">
    {#if picusUiStore.bottomTab === 'consistency'}
      {#if !picusProjectStore.attached}
        <StateBlock
          tone="info"
          fill={false}
          label="No repository attached to this connection — there is nothing to check."
        />
      {:else if consistencyStore.running && !consistencyStore.hasRun}
        <!-- Only the FIRST pass gets a block: a re-check keeps the previous report
             on screen with a quiet marker in the header, because blanking a report
             you were reading is worse than a stale one for two seconds. -->
        <StateBlock tone="loading">
          {#snippet spinner()}<Spinner size={14} />{/snippet}
          <span>Checking the repository…</span>
        </StateBlock>
      {:else}
        {#if consistencyStore.running}
          <div class="bd-rechecking">
            <Spinner size={11} />
            <span>Re-checking — the report below is the previous pass.</span>
          </div>
        {/if}
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
              <!-- `~` where the number was the planner's estimate at the time: a
                   history line is read long after the count could have settled it. -->
              <span class="bd-log-meta">
                {entry.approximate ? '~' : ''}{entry.rowCount.toLocaleString()} rows · {entry.elapsedMs} ms
              </span>
            </div>
          {/each}
        {/if}
      </div>

    {:else}
      <div class="bd-changes">
        {#if dmlStore.applyError}
          <!-- The backend's refusal, word for word: it names the file that moved,
               which is the only part that tells the user what to do next. -->
          <Alert variant="error" title="Nothing was written" text={dmlStore.applyError}>
            {#snippet actions()}
              <Button variant="secondary" size="xs" onclick={() => void dmlStore.rebuildPreview()}>
                Read the files again
              </Button>
            {/snippet}
          </Alert>
        {/if}

        {#if !dmlStore.generated}
          <StateBlock
            tone="info"
            fill={false}
            label="No pending change. Generate from the DML tab to see which files would be touched."
          />
        {:else if dmlStore.previewError}
          <Alert
            variant="error"
            title="The patch could not be computed"
            text={dmlStore.previewError}
          />
        {:else if dmlStore.previewing && !dmlStore.previewFiles.length}
          <StateBlock tone="loading">
            {#snippet spinner()}<Spinner size={14} />{/snippet}
            <span>Reading the destinations…</span>
          </StateBlock>
        {:else if !dmlStore.previewFiles.length}
          <StateBlock
            tone="info"
            fill={false}
            label="Nothing to write — no destination is enabled, or none of them would change."
          />
        {:else}
          {#if !dmlStore.previewFresh}
            <!-- The generation moved after this patch was computed. Writing it is
                 refused rather than silently re-planned, so say so here. -->
            <Alert
              variant="warning"
              compact
              text="The generation changed after this patch was computed — it is out of date and will not be written as it stands. Rebuild it to see what would land now."
            >
              {#snippet actions()}
                <Button variant="secondary" size="xs" onclick={() => void dmlStore.rebuildPreview()}>
                  Rebuild
                </Button>
              {/snippet}
            </Alert>
          {/if}
          <p class="bd-changes-note">
            {dmlStore.changedFiles.length} file{dmlStore.changedFiles.length === 1 ? '' : 's'} would be
            written, exactly as shown below — this is the backend's own output, not a rendering of it.
            Encoding and line endings stay as they are, and every original is copied to
            <code>.arbor/backup</code> first. Nothing is written until you confirm.
          </p>
          {#each dmlStore.previewFiles as file (file.path)}
            <PatchDiffCard {file} target={targetFor(file.path)} />
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

  /* A re-check in flight, stated without taking the report away. */
  .bd-rechecking {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-muted);
  }

  .bd-changes { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; }
  .bd-changes-note {
    font-size: 11.5px;
    line-height: 1.55;
    color: var(--text-muted);
    max-width: 90ch;
  }
  .bd-changes-note code { font-family: var(--font-code); font-size: 11px; }
</style>
