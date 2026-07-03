<script lang="ts">
  /**
   * BennuIndexInspectorModal — a debug view of the project's semantic index:
   * headline stats (types / members / JDK / config-graph counts / ready state) from
   * `bennu_index_stats`, plus a searchable, virtualization-free list of the indexed
   * classes from `bennu_class_index`. Click a class to open it. Read-only.
   */
  import { Database, Box, RefreshCw, CircleCheckBig, Loader } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { indexStats as ipcStats, classIndex as ipcClasses } from '$lib/ipc/bennu';
  import type { IndexStats, ClassEntry } from '$lib/types/bennu';

  let { onClose }: { onClose: () => void } = $props();

  let stats = $state<IndexStats | null>(null);
  let classes = $state<ClassEntry[]>([]);
  let loading = $state(false);
  let query = $state('');

  async function load() {
    const root = projectStore.project?.root;
    if (!root) return;
    loading = true;
    try {
      const [s, c] = await Promise.all([ipcStats(root), ipcClasses(root)]);
      stats = s;
      classes = c;
    } catch {
      stats = null;
      classes = [];
    } finally {
      loading = false;
    }
  }
  $effect(() => { void load(); });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q ? classes.filter((c) => c.fqcn.toLowerCase().includes(q)) : classes;
    return list.slice(0, 500);
  });

  const cards = $derived(
    stats
      ? [
          { label: 'Types', value: stats.types },
          { label: 'Members', value: stats.members },
          { label: 'JDK', value: stats.jdk_version || '—' },
          { label: 'Jars', value: stats.jar_count },
          { label: 'Actions', value: stats.actions },
          { label: 'Beans', value: stats.beans },
          { label: 'Relations', value: stats.relations },
        ]
      : [],
  );

  function open(c: ClassEntry) {
    onClose();
    void projectStore.openFile(c.file).then(() => bennuUiStore.requestGoto(c.line));
  }
</script>

<Modal {onClose} width="720px" height="600px" padBody={false} ariaLabel="Index inspector">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Database size={14} />
      <span class="modal-title">Index inspector</span>
      {#if stats}
        <span class="hdr-state" class:ready={stats.ready}>
          {#if stats.ready}<CircleCheckBig size={12} /> ready{:else}<Loader size={12} /> building…{/if}
        </span>
      {/if}
      <button class="hdr-refresh" type="button" use:tooltip={'Refresh'} aria-label="Refresh" onclick={() => void load()}>
        <RefreshCw size={13} />
      </button>
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if !projectStore.project}
      <EmptyState message="Open a project to inspect its index." />
    {:else}
      <div class="stats">
        {#each cards as c (c.label)}
          <div class="stat"><span class="s-val">{c.value}</span><span class="s-label">{c.label}</span></div>
        {/each}
      </div>

      <div class="search"><Input bind:value={query} placeholder="Filter classes by name…" /></div>

      {#if loading && !classes.length}
        <div class="state"><Spinner size={13} /> Loading index…</div>
      {:else if filtered.length === 0}
        <div class="state muted">{query ? 'No classes match.' : 'No classes indexed.'}</div>
      {:else}
        <div class="list">
          {#each filtered as c (c.fqcn + c.file)}
            <button class="row" type="button" onclick={() => open(c)} title={c.file}>
              <Box size={12} />
              <span class="r-simple">{c.simple}</span>
              <span class="r-fqcn">{c.fqcn}</span>
            </button>
          {/each}
          {#if classes.length > filtered.length}
            <div class="more">Showing {filtered.length} of {classes.length}. Refine the filter to see more.</div>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .hdr-state { display: inline-flex; align-items: center; gap: 4px; font-size: 10.5px; color: var(--text-muted); }
  .hdr-state.ready { color: var(--success); }
  .hdr-refresh { display: inline-flex; margin-left: auto; background: transparent; border: none; color: var(--text-muted); cursor: pointer; padding: 2px; border-radius: var(--radius-sm); }
  .hdr-refresh:hover { color: var(--text-primary); background: var(--bg-hover); }

  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .stats { display: grid; grid-template-columns: repeat(7, 1fr); gap: 6px; padding: 14px 16px 10px; flex-shrink: 0; }
  .stat { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 8px 4px; background: var(--bg-elevated); border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .s-val { font-size: 15px; font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .s-label { font-size: 9px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }

  .search { padding: 0 16px 8px; flex-shrink: 0; }
  .state { display: flex; align-items: center; gap: 7px; padding: 14px 16px; font-size: 12px; color: var(--text-secondary); }
  .state.muted { color: var(--text-muted); }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 8px 10px; }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; padding: 4px 8px; background: transparent; border: none; border-radius: var(--radius-sm); cursor: pointer; font-family: var(--font-ui-sans); }
  .row:hover { background: var(--bg-hover); }
  .row :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .r-simple { font-size: 12px; color: var(--text-primary); font-weight: 500; flex-shrink: 0; }
  .r-fqcn { flex: 1; min-width: 0; font-size: 10.5px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .more { padding: 8px 10px; font-size: 10.5px; color: var(--text-muted); }
</style>
