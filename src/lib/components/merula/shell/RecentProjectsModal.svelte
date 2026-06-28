<script lang="ts">
  /**
   * Recent Projects — a proper Arbor modal (Modal + ModalHeader) listing the
   * recent merula projects (from the persisted workspace state), searchable and
   * keyboard-navigable. Picking one opens it via the project store.
   */
  import { FolderGit2, Search } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { workspaceStore } from '../stores/workspace.svelte';
  import { projectStore } from '../stores/project.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let query = $state('');

  /** Last path segment (forward- or back-slash). */
  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const recents = workspaceStore.recentProjects;
    if (!q) return recents;
    return recents.filter(p => p.toLowerCase().includes(q));
  });

  function pick(path: string) {
    onClose();
    void projectStore.open(path).catch(() => {});
  }
</script>

<Modal {onClose} size="md" ariaLabel="Recent projects">
  {#snippet header()}
    <ModalHeader title="Recent Projects" {onClose} />
  {/snippet}

  <div class="rp">
    <div class="rp-search">
      <Input bind:value={query} placeholder="Search recent projects…" size="md" autofocus>
        {#snippet iconStart()}<Search size={14} />{/snippet}
      </Input>
    </div>

    {#if filtered.length === 0}
      <EmptyState message={query ? 'No recent projects match your search.' : 'No recent projects yet.'} />
    {:else}
      <div class="rp-list">
        {#each filtered as path (path)}
          {@const current = path === projectStore.project?.path}
          <button class="rp-item" class:current onclick={() => pick(path)}>
            <span class="rp-icon"><FolderGit2 size={18} /></span>
            <span class="rp-body">
              <span class="rp-name">{basename(path)}{#if current}<span class="rp-open">open</span>{/if}</span>
              <span class="rp-audience">{path}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</Modal>

<style>
  .rp { display: flex; flex-direction: column; gap: 10px; }
  .rp-search { flex-shrink: 0; }
  .rp-list { display: flex; flex-direction: column; gap: 3px; }

  .rp-item {
    display: flex; align-items: center; gap: 11px;
    width: 100%; padding: 9px 10px;
    background: transparent; border: 1px solid transparent;
    border-radius: var(--radius-md); cursor: pointer; text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .rp-item:hover { background: var(--bg-hover); }
  .rp-item.current { border-color: var(--accent-subtle); }
  .rp-icon { display: flex; color: var(--accent); flex-shrink: 0; }
  .rp-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .rp-name { display: flex; align-items: center; gap: 7px; font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .rp-open {
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.4px;
    color: var(--accent); background: var(--accent-subtle);
    border-radius: var(--radius-sm); padding: 1px 5px;
  }
  .rp-audience { font-size: 11.5px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
