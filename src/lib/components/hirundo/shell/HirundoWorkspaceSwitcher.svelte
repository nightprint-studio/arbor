<script lang="ts">
  /**
   * HirundoWorkspaceSwitcher — the title bar's project/workspace switcher, the same shape as Bennu's and
   * Corvus's: the trigger shows the project with its workspace's colour; the menu is a two-level tree, each
   * **workspace** a header (click to switch to it) with its **projects** nested under it (click to go
   * straight to that project, switching workspace first). Add / new / manage live in the footer.
   */
  import { ChevronDown, Check, FolderPlus, Layers, Plus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { wsColor } from '../request-model';

  const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const projectName = $derived(hirundoStore.project ? base(hirundoStore.project) : 'No project');
  const active = $derived(hirundoStore.activeWorkspace);
</script>

<Dropdown position="fixed" direction="down" width="320px">
  {#snippet trigger({ open, toggle })}
    <Button variant="ghost" size="sm" ariaExpanded={open} ariaHaspopup="menu" tooltip={active ? `Workspace ${active.name} — switch project or workspace` : 'Open a project'} onclick={toggle}>
      {#snippet iconStart()}<Monogram name={projectName} size={20} color={active ? wsColor(active.color_idx) : undefined} />{/snippet}
      {projectName}
      <ChevronDown size={12} />
    </Button>
  {/snippet}

  {#snippet children({ close })}
    <div class="ws">
      {#if hirundoStore.workspaces.length === 0}
        <EmptyState message="No workspace yet" description="Open a project to start one." compact />
      {/if}
      {#each hirundoStore.workspaces as w (w.id)}
        {@const isActive = w.id === hirundoStore.config.active_workspace}
        <SidebarItem selected={isActive} onclick={() => { close(); void hirundoStore.switchWorkspace(w.id); }}>
          {#snippet icon()}<Monogram name={w.name || 'Workspace'} size={18} color={wsColor(w.color_idx)} />{/snippet}
          <b>{w.name || 'Workspace'}</b>
          {#snippet badges()}{#if isActive}<Check size={12} />{/if}<span class="ws-count">{w.projects.length}</span>{/snippet}
        </SidebarItem>
        {#each w.projects as root (root)}
          <SidebarItem indent={1} selected={isActive && root === hirundoStore.project} onclick={() => { close(); void hirundoStore.switchToProject(w.id, root); }}>
            <span class="ws-proj" use:tooltip={root}>{base(root)}</span>
          </SidebarItem>
        {/each}
      {/each}
    </div>
  {/snippet}

  {#snippet footer({ close })}
    <div class="ws-foot">
      <Button variant="ghost" size="sm" block onclick={() => { close(); hirundoUiStore.open({ kind: 'project-picker', workspaceId: active?.id }); }}>{#snippet iconStart()}<FolderPlus size={13} />{/snippet}Add project…</Button>
      <Button variant="ghost" size="sm" block onclick={() => { close(); void hirundoStore.createWorkspace('New workspace').then(() => hirundoUiStore.open({ kind: 'workspaces' })); }}>{#snippet iconStart()}<Plus size={13} />{/snippet}New workspace…</Button>
      <Button variant="ghost" size="sm" block onclick={() => { close(); hirundoUiStore.open({ kind: 'workspaces' }); }}>{#snippet iconStart()}<Layers size={13} />{/snippet}Manage workspaces…</Button>
    </div>
  {/snippet}
</Dropdown>

<style>
  .ws { display: flex; flex-direction: column; padding: 4px; max-height: 360px; overflow: auto; }
  .ws-count { font-size: var(--font-size-xs); color: var(--text-muted); }
  .ws-proj { font-size: var(--font-size-sm); }
  .ws-foot { display: flex; flex-direction: column; padding: 4px; }
</style>
