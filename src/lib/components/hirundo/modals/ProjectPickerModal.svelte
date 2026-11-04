<script lang="ts">
  /**
   * ProjectPickerModal — which project the window works on. Recent projects first, filtered as you type
   * (↑↓ move, Enter opens), and "Browse…" opens Arbor's own folder picker. A project is any folder: its
   * collections live in a `.hirundo/` folder inside it, created on the first save. Opened from a workspace's
   * "Add project…", the chosen folder joins that workspace.
   */
  import { onMount } from 'svelte';
  import { FolderOpen, FolderSearch, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { fuzzyMatch } from '$lib/utils/fuzzy';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

  let { onClose, workspaceId }: { onClose: () => void; workspaceId?: string } = $props();

  let picking = $state(false);
  let index = $state(0);
  let query = $state('');
  let filterEl = $state<HTMLInputElement | null>(null);

  const workspaceName = $derived(workspaceId ? hirundoStore.workspaces.find((w) => w.id === workspaceId)?.name : undefined);
  const shown = $derived(hirundoStore.recents.filter((p) => fuzzyMatch(p, query) !== null));
  $effect(() => { if (index >= shown.length) index = 0; });
  onMount(() => filterEl?.focus());

  async function open(path: string) {
    onClose();
    if (workspaceId) await hirundoStore.addProjectTo(workspaceId, path);
    await hirundoStore.openProject(path);
  }

  function onKey(e: KeyboardEvent) {
    if (picking) return;
    const n = shown.length;
    if (e.key === 'ArrowDown' && n) { index = (index + 1) % n; e.preventDefault(); }
    else if (e.key === 'ArrowUp' && n) { index = (index - 1 + n) % n; e.preventDefault(); }
    else if (e.key === 'Enter' && n) { void open(shown[index]); e.preventDefault(); }
    else if (e.key === 'b' && (e.ctrlKey || e.metaKey)) { picking = true; e.preventDefault(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="620px" height="460px" ariaLabel="Open a project">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderOpen size={14} />
      <span class="modal-title">{workspaceName ? `Add a project to ${workspaceName}` : 'Open a project'}</span>
    </ModalHeader>
  {/snippet}

  <div class="pp">
    <div class="pp-filter"><FilterInput bind:value={query} bind:element={filterEl} placeholder="Filter recent projects…" ariaLabel="Filter recent projects" /></div>
    {#if hirundoStore.recents.length === 0}
      <EmptyState message="No recent projects" description="Browse to the folder of a project: its requests will be kept in a .hirundo folder inside it." />
    {:else if shown.length === 0}
      <EmptyState message="No project matches" compact />
    {:else}
      <div class="pp-list">
        {#each shown as path, i (path)}
          <SidebarItem selected={i === index} onclick={() => void open(path)}>
            {#snippet icon()}<FolderOpen size={14} />{/snippet}
            <span class="pp-name">{path.split(/[\\/]/).filter(Boolean).pop()}</span>
            <span class="pp-path">{path}</span>
            {#snippet actions()}<IconButton tooltip="Forget this project" size={20} onclick={(e) => { e.stopPropagation(); void hirundoStore.forgetProject(path); }}><X size={12} /></IconButton>{/snippet}
          </SidebarItem>
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter hint="to open the highlighted project">
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" tooltip={{ content: 'Choose a folder', shortcut: 'Ctrl+B' }} onclick={() => (picking = true)}>
        {#snippet iconStart()}<FolderSearch size={13} />{/snippet}
        Browse…
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal mode="folder" title="Choose the project folder" onConfirm={(p: string) => void open(p)} onCancel={() => (picking = false)} onClose={() => (picking = false)} />
{/if}

<style>
  .pp { display: flex; flex-direction: column; height: 100%; min-height: 0; padding: 10px 8px 8px; }
  .pp-filter { padding: 0 4px 8px; }
  .pp-list { overflow: auto; flex: 1; min-height: 0; }
  .pp-name { font-weight: 500; }
  .pp-path { margin-left: 8px; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
