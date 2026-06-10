<script lang="ts">
  /**
   * Recent Projects — a proper Arbor modal (Modal + ModalHeader) listing the
   * recent grove projects, searchable, keyboard-navigable. Mocked: picking a
   * project just closes the modal. Reuses Arbor's modal shell for consistency.
   */
  import { FolderGit2, Search } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { RECENT_PROJECTS, MOCK_PROJECT } from '../mock/data';

  let { onClose }: { onClose: () => void } = $props();

  let query = $state('');
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return RECENT_PROJECTS;
    return RECENT_PROJECTS.filter(p => p.name.toLowerCase().includes(q) || p.audience.toLowerCase().includes(q));
  });

  function pick(_id: string) { onClose(); /* mock: would switch project */ }
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
      <EmptyState message="No recent projects match your search." />
    {:else}
      <div class="rp-list">
        {#each filtered as p (p.id)}
          <button class="rp-item" class:current={p.id === MOCK_PROJECT.id} onclick={() => pick(p.id)}>
            <span class="rp-icon"><FolderGit2 size={18} /></span>
            <span class="rp-body">
              <span class="rp-name">{p.name}{#if p.id === MOCK_PROJECT.id}<span class="rp-open">open</span>{/if}</span>
              <span class="rp-audience">{p.audience}</span>
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
