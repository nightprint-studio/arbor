<script lang="ts">
  /**
   * The consistency report — one panel of the bottom dock, and its own header.
   *
   * Split out of `PicusBottomDock` when the dock stopped being a tabbed container:
   * each rail button opens its own panel, so the panel owns its title and its
   * actions rather than having them assembled by whatever was hosting it. That is
   * the Corvus arrangement, and it is a row of chrome cheaper — a dock header with
   * a tab strip in it, plus each panel's own bar underneath, was two.
   */
  import { RefreshCw, EyeOff, Eye } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import FindingList from './FindingList.svelte';
  import { findingsToText } from './finding-text';
  import { consistencyStore, type FindingGrouping } from '$lib/stores/picus/consistency.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';

  let filterQuery = $state('');
  $effect(() => consistencyStore.setFilter(filterQuery));

  const groupOptions = [
    { value: 'severity', label: 'Group by severity' },
    // Folder rather than "branch": there is no branch level any more, and the
    // folder is where a finding's engine and role actually come from.
    { value: 'folder', label: 'Group by folder' },
    { value: 'file', label: 'Group by file' },
  ];

  /**
   * What a copied report says it is.
   *
   * Names the repository and the filter, because the copy almost always ends up
   * somewhere the reader cannot see this panel — a ticket, a message — and
   * "12 findings" with no idea which twelve is a paste that has to be explained
   * in the next line anyway.
   */
  function copyHeading(): string {
    const count = consistencyStore.visible.length;
    const project = picusProjectStore.project?.name ?? 'this project';
    const filter = consistencyStore.filter.trim();
    const scope = filter ? ` matching “${filter}”` : '';
    return `Picus — ${count} finding${count === 1 ? '' : 's'}${scope} in ${project}`;
  }
</script>

<div class="cp">
  <BottomPanelHeader
    title="Consistency"
    count={consistencyStore.totalCount}
    onClose={() => picusUiStore.closeBottom()}
  >
    {#snippet actions()}
      <div class="cp-filter">
        <SearchBar
          bind:query={filterQuery}
          showRegex={false}
          placeholder="Filter findings"
          ariaLabel="Filter findings"
        />
      </div>
      <Select
        value={consistencyStore.grouping}
        options={groupOptions}
        narrow
        onchange={(v) => consistencyStore.setGrouping(v as FindingGrouping)}
      />
      <!-- The list as it stands on screen, filter and grouping included — a pasted
           excerpt that looks like the whole report is worse than one that says what
           it is, so the heading names the count and the filter. -->
      <CopyButton
        value={() => findingsToText(consistencyStore.visible, copyHeading())}
        title="Copy the findings shown"
        toastSuccess="Findings copied."
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
    {/snippet}
  </BottomPanelHeader>

  <div class="cp-body">
    {#if !picusProjectStore.attached}
      <StateBlock
        tone="info"
        fill={false}
        label="No repository attached to this connection — there is nothing to check."
      />
    {:else if consistencyStore.running && !consistencyStore.hasRun}
      <!-- Only the FIRST pass gets a block: a re-check keeps the previous report on
           screen with a quiet marker in the header, because blanking a report you
           were reading is worse than a stale one for two seconds. -->
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Checking the repository…</span>
      </StateBlock>
    {:else}
      {#if consistencyStore.running}
        <div class="cp-rechecking">
          <Spinner size={11} />
          <span>Re-checking — the report below is the previous pass.</span>
        </div>
      {/if}
      <FindingList />
    {/if}
  </div>
</div>

<style>
  .cp { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .cp-body { flex: 1; min-height: 0; overflow: auto; }
  .cp-filter { width: 180px; }

  /* A re-check in flight, stated without taking the report away. */
  .cp-rechecking {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
</style>
