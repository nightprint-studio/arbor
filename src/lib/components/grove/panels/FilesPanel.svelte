<script lang="ts">
  /** Files panel — the project's `.grove` files, grouped by folder. Click opens
   *  the file as an editor tab. */
  import { FileMusic, BookLock, FolderOpen, Plus, Folder } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_PROJECT } from '../mock/data';
  import type { GroveFile } from '../mock/types';

  const rootFiles = $derived(MOCK_PROJECT.files.filter(f => !f.path.includes('/')));
  const folders = $derived.by(() => {
    const map = new Map<string, GroveFile[]>();
    for (const f of MOCK_PROJECT.files) {
      const slash = f.path.lastIndexOf('/');
      if (slash === -1) continue;
      const dir = f.path.slice(0, slash);
      const arr = map.get(dir) ?? [];
      arr.push(f); map.set(dir, arr);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  // Folder sections expanded by default.
  let open = $state<Record<string, boolean>>({});
  const isOpen = (dir: string) => open[dir] ?? true;
</script>

{#snippet fileRow(f: GroveFile)}
  <SidebarItem selected={groveStore.activeFileId === f.id} onclick={() => groveStore.openFile(f.id)}>
    {#snippet icon()}
      {#if f.library}<BookLock size={13} />{:else}<FileMusic size={13} />{/if}
    {/snippet}
    {f.name}
    {#snippet badges()}
      {#if f.library}<span use:tooltip={'Library — its tracks() output is ignored'}><Badge variant="tone" tone="neutral" size="sm" label="lib" /></span>{/if}
    {/snippet}
  </SidebarItem>
{/snippet}

<PanelShell title="Files" count={MOCK_PROJECT.files.length}>
  {#snippet icon()}<FolderOpen size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn ps-btn-accent" use:tooltip={'New .grove'} aria-label="New file"><Plus size={14} /></button>
  {/snippet}

  <div class="files">
    {#each rootFiles as f (f.id)}{@render fileRow(f)}{/each}

    {#each folders as [dir, files] (dir)}
      <SidebarSection
        label={dir + '/'}
        expanded={isOpen(dir)}
        onToggle={() => open = { ...open, [dir]: !isOpen(dir) }}
        badge={files.length}
      >
        {#snippet icon()}<Folder size={13} />{/snippet}
        {#each files as f (f.id)}{@render fileRow(f)}{/each}
      </SidebarSection>
    {/each}
  </div>
</PanelShell>

<style>
  .files { padding: 4px 0; }
</style>
