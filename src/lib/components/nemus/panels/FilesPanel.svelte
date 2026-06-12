<script lang="ts">
  /** Files panel — the open project's `.nemus` files, grouped by folder. Click
   *  opens the file as an editor tab; the New button scaffolds a starter file.
   *  Driven by the real `projectStore` (path-keyed). */
  import { FileMusic, BookLock, FolderOpen, Plus, Folder } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import type { NemusProjectFile } from '$lib/ipc/nemus';

  const files = $derived(projectStore.files);
  const rootFiles = $derived(files.filter(f => !f.rel.includes('/')));
  const folders = $derived.by(() => {
    const map = new Map<string, NemusProjectFile[]>();
    for (const f of files) {
      const slash = f.rel.lastIndexOf('/');
      if (slash === -1) continue;
      const dir = f.rel.slice(0, slash);
      const arr = map.get(dir) ?? [];
      arr.push(f); map.set(dir, arr);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  // Folder sections expanded by default.
  let open = $state<Record<string, boolean>>({});
  const isOpen = (dir: string) => open[dir] ?? true;
</script>

{#snippet fileRow(f: NemusProjectFile)}
  <SidebarItem selected={projectStore.activeFilePath === f.path} onclick={() => projectStore.openFile(f.path)}>
    {#snippet icon()}
      {#if f.library}<BookLock size={13} />{:else}<FileMusic size={13} />{/if}
    {/snippet}
    {f.name}
    {#snippet badges()}
      {#if f.library}<span use:tooltip={'Library — its tracks() output is ignored'}><Badge variant="tone" tone="neutral" size="sm" label="lib" /></span>{/if}
    {/snippet}
  </SidebarItem>
{/snippet}

<PanelShell title="Files" count={files.length}>
  {#snippet icon()}<FolderOpen size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn ps-btn-accent" onclick={() => projectActions.newFile()} use:tooltip={'New .nemus (Ctrl+N)'} aria-label="New file"><Plus size={14} /></button>
  {/snippet}

  {#if files.length === 0}
    <EmptyState message="No project open. Open a project (Ctrl+O) or create one (Ctrl+Shift+N)." />
  {:else}
    <div class="files">
      {#each rootFiles as f (f.path)}{@render fileRow(f)}{/each}

      {#each folders as [dir, dirFiles] (dir)}
        <SidebarSection
          label={dir + '/'}
          expanded={isOpen(dir)}
          onToggle={() => open = { ...open, [dir]: !isOpen(dir) }}
          badge={dirFiles.length}
        >
          {#snippet icon()}<Folder size={13} />{/snippet}
          {#each dirFiles as f (f.path)}{@render fileRow(f)}{/each}
        </SidebarSection>
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .files { padding: 4px 0; }
</style>
