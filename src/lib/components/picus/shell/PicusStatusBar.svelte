<script lang="ts">
  /**
   * Picus footer — the IntelliJ-style status strip.
   *
   * Left: the active connection (colour + schema), its engine and the database
   * version the version table reports.
   * Right: the open file's encoding and line ending, the open-findings counter
   * (a button — it reveals the Consistency dock), the project path and its
   * counters, then the shared feedback badges injected by the window.
   *
   * Everything here is either an at-a-glance fact or a shortcut to the panel
   * that explains it; nothing is decorative.
   */
  import { FolderTree, TriangleAlert, CheckCircle2, Files } from 'lucide-svelte';
  import type { Snippet } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusConnectionPill from '../PicusConnectionPill.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { consistencyStore } from '$lib/stores/picus/consistency.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { DIALECTS } from '$lib/types/picus';

  let { footerExtra }: { footerExtra?: Snippet } = $props();

  const conn = $derived(picusTabsStore.activeConnection);
  const project = $derived(picusProjectStore.project);

  /** The encoding badge only makes sense while a file tab is open. */
  const openFile = $derived.by(() => {
    const tab = picusTabsStore.active;
    if (tab?.kind !== 'file' || !tab.file) return null;
    return picusProjectStore.fileByPath(tab.file);
  });

  const blocking = $derived(consistencyStore.blockingCount);
  const review = $derived(consistencyStore.reviewCount);
</script>

<div class="pf">
  {#if conn}
    <PicusConnectionPill connection={conn} density="status" onclick={() => picusUiStore.showSection('connections')} />
    <span class="pf-sep"></span>
    <span class="pf-item" use:tooltip={`${DIALECTS[conn.dialect].label} · ${conn.host}`}>
      {DIALECTS[conn.dialect].short}
    </span>
    <span class="pf-sep"></span>
    <span class="pf-item" use:tooltip={'Application version stamped in the version table'}>
      db {conn.dbVersion}
    </span>
  {:else}
    <span class="pf-item pf-muted">No connection</span>
  {/if}

  <span class="pf-spacer"></span>

  {#if openFile}
    <EncodingPill
      encoding={openFile.encoding}
      expected={openFile.expectedEncoding}
      eol={openFile.eol}
      compact
    />
    <span class="pf-sep"></span>
  {/if}

  <!-- Findings counter: the single click that gets you to what is wrong. -->
  <button
    class="pf-item pf-btn"
    class:pf-bad={blocking > 0}
    class:pf-ok={blocking === 0 && review === 0}
    onclick={() => picusUiStore.showBottom('consistency')}
    use:tooltip={{
      content: blocking > 0
        ? `${blocking} blocking · ${review} to check`
        : review > 0 ? `${review} finding(s) worth checking` : 'No consistency problems',
      description: consistencyStore.lastRunAt ? `Last checked at ${consistencyStore.lastRunAt}` : 'Never checked',
    }}
  >
    {#if blocking > 0}
      <TriangleAlert size={12} /> {blocking} blocking
      {#if review > 0}<span class="pf-sub">· {review}</span>{/if}
    {:else if review > 0}
      <TriangleAlert size={12} /> {review} to check
    {:else}
      <CheckCircle2 size={12} /> Consistent
    {/if}
  </button>

  {#if project}
    <span class="pf-sep"></span>
    <span class="pf-item" use:tooltip={`${picusProjectStore.branches.length} branches · ${picusProjectStore.fileCount} files`}>
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
    font-size: 11px;
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
    font-size: 10.5px;
  }
</style>
