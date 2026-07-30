<script lang="ts">
  /**
   * Picus footer — the IntelliJ-style status strip.
   *
   * Left: the active connection (colour + schema), its engine and the database
   * version the version table reports.
   * Right: how long the result on screen is, the open file's encoding and line
   * ending, the open-findings counter
   * (a button — it reveals the Consistency dock), the project path and its
   * counters, then the shared feedback badges injected by the window.
   *
   * Everything here is either an at-a-glance fact or a shortcut to the panel
   * that explains it; nothing is decorative.
   */
  import { FolderTree, TriangleAlert, CheckCircle2, Files } from 'lucide-svelte';
  import type { Snippet } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusConnectionPill from '../PicusConnectionPill.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { consistencyStore } from '$lib/stores/picus/consistency.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { formatRowTotal, picusResultsStore } from '$lib/stores/picus/result.svelte';

  let { footerExtra }: { footerExtra?: Snippet } = $props();

  const conn = $derived(picusTabsStore.activeConnection);
  const project = $derived(picusProjectStore.project);

  /** The encoding badge only makes sense while a file tab is open. */
  const openFile = $derived.by(() => {
    const tab = picusTabsStore.active;
    if (tab?.kind !== 'file' || !tab.file) return null;
    return picusProjectStore.fileByPath(tab.file);
  });

  /**
   * How long the result on screen is.
   *
   * This is where a table's row count lives now that browsing data is a
   * continuous scroll: a page selector used to carry it, and an infinite
   * scrollbar carries nothing. `~` while it is the planner's estimate.
   */
  const result = $derived(picusResultsStore.forOwner(picusTabsStore.activeId));

  const blocking = $derived(consistencyStore.blockingCount);
  const review = $derived(consistencyStore.reviewCount);
  const checking = $derived(consistencyStore.running);
</script>

<div class="pf">
  {#if conn}
    <!-- The connection, once.
         The engine and the installed version used to be spelled out again right
         here, three chips after the toolbar one row up had already said both —
         which put `appalti_local` on screen four times and `PostgreSQL` three,
         and taught the eye to skip all of them. The bar above owns the tab's
         binding; this one owns the window, so it keeps the name and hands the
         rest to the pill's tooltip. -->
    <PicusConnectionPill
      connection={conn}
      density="status"
      onclick={() => picusUiStore.showSection('connections')}
    />
  {:else}
    <span class="pf-item pf-muted">No connection</span>
  {/if}

  <span class="pf-spacer"></span>

  {#if result}
    <span
      class="pf-item"
      use:tooltip={{
        content: result.approximate
          ? `Estimated by the planner${result.counting ? ' — counting the exact number now' : ''}`
          : 'Counted on the server',
        description: `${result.loaded.toLocaleString()} row(s) loaded so far`,
      }}
    >
      {formatRowTotal(result)} rows
    </span>
    <span class="pf-sep"></span>
  {/if}

  {#if openFile}
    <EncodingPill
      encoding={openFile.encoding}
      expected={openFile.expectedEncoding}
      eol={openFile.eol}
      compact
    />
    <span class="pf-sep"></span>
  {/if}

  <!-- Findings counter: the single click that gets you to what is wrong.
       Checking a real repository takes about a second, so the working state has to
       be visible HERE — the dock that shows it in detail is often closed, and a
       counter that silently reads "Consistent" mid-analysis is a lie with a
       plausible face. -->
  <button
    class="pf-item pf-btn"
    class:pf-bad={blocking > 0 && !checking}
    class:pf-ok={blocking === 0 && review === 0 && !checking}
    onclick={() => picusUiStore.showBottom('consistency')}
    use:tooltip={{
      content: checking
        ? 'Checking the repository…'
        : blocking > 0
          ? `${blocking} blocking · ${review} to check`
          : review > 0 ? `${review} finding(s) worth checking` : 'No consistency problems',
      description: consistencyStore.lastRunAt ? `Last checked at ${consistencyStore.lastRunAt}` : 'Never checked',
    }}
  >
    {#if checking}
      <Spinner size={11} /> Checking…
    {:else if blocking > 0}
      <TriangleAlert size={12} /> {blocking} blocking
      {#if review > 0}<span class="pf-sub">· {review}</span>{/if}
    {:else if review > 0}
      <TriangleAlert size={12} /> {review} to check
    {:else if consistencyStore.hasRun}
      <CheckCircle2 size={12} /> Consistent
    {:else}
      <TriangleAlert size={12} /> Not checked
    {/if}
  </button>

  {#if project}
    <span class="pf-sep"></span>
    <span class="pf-item" use:tooltip={`${picusProjectStore.folderCount} folders · ${picusProjectStore.fileCount} files`}>
      <Files size={12} /> {picusProjectStore.fileCount}
    </span>
    <span class="pf-sep"></span>
    <span class="pf-item pf-path" use:tooltip={project.root}>
      <FolderTree size={12} /> {project.root}
    </span>
  {/if}

  {#if footerExtra}
    <span class="pf-sep"></span>
    {@render footerExtra()}
  {/if}
</div>

<style>
  .pf {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    flex-shrink: 0;
    padding: 0 8px 0 6px;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-subtle);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    user-select: none;
  }
  .pf-item { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .pf-item :global(svg) { color: var(--text-disabled); }
  .pf-muted { color: var(--text-disabled); }
  .pf-spacer { flex: 1; }
  .pf-sep { width: 1px; height: 12px; background: var(--border-subtle); flex-shrink: 0; }
  .pf-sub { color: var(--text-disabled); }

  .pf-btn {
    background: none;
    border: none;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .pf-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .pf-bad, .pf-bad :global(svg) { color: var(--error); }
  .pf-ok :global(svg) { color: var(--success); }

  /* The project path is the first thing to give up room when the bar is tight. */
  .pf-path {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
  }
</style>
