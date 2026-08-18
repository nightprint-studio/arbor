<script lang="ts">
  /**
   * BennuWorkspaceSwitcher — the titlebar project/workspace switcher (Corvus-tree style).
   *
   * The trigger shows the active project name with the active workspace's colour monogram. The
   * dropdown is a two-level tree: each **workspace** is a header row (click to switch to it), with
   * its **member projects** nested underneath (click to switch straight to that project, switching
   * workspace first if needed). No "recent projects" here — recents live in the hamburger menu; the
   * switcher is purely workspace/project navigation, mirroring Corvus's WorkspaceDropdown.
   *
   * The folder-picker for Open / Add project lives in the parent titlebar (shared with the
   * hamburger + Ctrl+O), so those two actions are delegated up via `onOpenPicker`.
   */
  import { ChevronDown, FolderPlus, Plus, Layers, Check, FileCode2 } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { workspacesStore, wsColorVar } from '$lib/stores/bennu/workspaces.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  let { onOpenPicker }: { onOpenPicker: (mode: 'open' | 'add') => void } = $props();

  const projectName = $derived(projectStore.project?.name ?? 'No project');
  const activeRoot = $derived(projectStore.project?.root ?? null);

  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  /** The projects to list under a workspace: the ACTIVE workspace's come live from projectStore
   *  (real names + open state); an inactive workspace's come from its persisted member list
   *  (basename until it's opened). */
  function projectsOf(wsId: string): { root: string; name: string }[] {
    if (wsId === workspacesStore.activeId) {
      return projectStore.workspaceProjects;
    }
    const ws = workspacesStore.workspaces.find((w) => w.id === wsId);
    return ws ? ws.projects.map((p) => ({ root: p.root, name: basename(p.root) })) : [];
  }

  /**
   * Create a workspace and go straight to picking its first project.
   *
   * It used to create one called "New workspace" and open the manager — which left you in a modal
   * about *naming* something that had nothing in it, with the actual next step ("Add project…")
   * back in a different menu. A new workspace exists in order to hold a project, so the picker is
   * what should be in front of you; the name comes from the first project it gets, and the manager
   * is still there to change it.
   */
  async function newWorkspace(close: () => void) {
    close();
    await workspacesStore.create('');
    onOpenPicker('add');
  }
</script>

<Dropdown position="fixed" direction="down" width="300px">
  {#snippet trigger({ open, toggle })}
    <button
      class="btb-project"
      class:open
      onclick={toggle}
      use:tooltip={workspacesStore.active
        ? `Workspace: ${workspacesStore.activeName} — switch project / workspace`
        : 'Switch project'}
      aria-haspopup="menu"
      aria-expanded={open}
    >
      <Monogram
        name={projectName}
        size={22}
        color={workspacesStore.active ? wsColorVar(workspacesStore.active.color_idx) : undefined}
      />
      <span class="btb-project-name">{projectName}</span>
      {#if projectStore.isDemo}<span class="btb-demo">demo</span>{/if}
      <ChevronDown size={12} class="btb-project-chev" />
    </button>
  {/snippet}

  {#snippet children({ close })}
    <div class="ws-tree" role="menu">
      {#if workspacesStore.workspaces.length === 0}
        <div class="ws-none">No workspace yet — open a project to start one.</div>
      {/if}
      {#each workspacesStore.workspaces as w (w.id)}
        {@const isActiveWs = w.id === workspacesStore.activeId}
        {@const projects = projectsOf(w.id)}
        <button
          class="ws-head"
          class:active={isActiveWs}
          onclick={() => { close(); void workspacesStore.switchTo(w.id); }}
          role="menuitem"
        >
          <Monogram name={w.name || 'Workspace'} color={wsColorVar(w.color_idx)} size={18} />
          <span class="ws-head-name">{w.name || 'Workspace'}</span>
          {#if isActiveWs}<Check size={12} class="ws-head-check" />{/if}
          <span class="ws-head-count">{projects.length}</span>
        </button>
        {#each projects as p (p.root)}
          {@const isActiveProj = isActiveWs && p.root === activeRoot}
          <button
            class="ws-proj"
            class:active={isActiveProj}
            onclick={() => { close(); void workspacesStore.switchToProject(w.id, p.root); }}
            use:tooltip={p.root}
            role="menuitem"
          >
            <span class="ws-proj-rail"></span>
            <FileCode2 size={13} class="ws-proj-icon" />
            <span class="ws-proj-name">{p.name}</span>
            {#if isActiveProj}<Check size={11} class="ws-proj-check" />{/if}
          </button>
        {/each}
      {/each}
    </div>
  {/snippet}

  {#snippet footer({ close })}
    <button class="ws-foot" onclick={() => { close(); onOpenPicker('add'); }} role="menuitem">
      <FolderPlus size={13} /><span>Add project…</span>
    </button>
    <button class="ws-foot" onclick={() => void newWorkspace(close)} role="menuitem">
      <Plus size={13} /><span>New workspace…</span>
    </button>
    <button class="ws-foot" onclick={() => { close(); bennuUiStore.openWorkspaceManager(); }} role="menuitem">
      <Layers size={13} /><span>Manage workspaces…</span>
    </button>
  {/snippet}
</Dropdown>

<style>
  /* Trigger — matches the former inline .btb-project exactly (Corvus WorkspaceDropdown look). */
  .btb-project {
    display: inline-flex; align-items: center; gap: 8px;
    height: 30px; margin-left: 4px; padding: 0 8px 0 6px;
    background: transparent; border: 1px solid transparent;
    border-radius: var(--radius-sm); color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm); font-weight: 500; cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    -webkit-app-region: no-drag;
    max-width: 260px;
  }
  .btb-project:hover { background: var(--bg-hover); }
  .btb-project.open  { background: var(--bg-hover); border-color: var(--border-subtle); }
  .btb-project-name {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  :global(.btb-project .btb-project-chev)       { color: var(--text-muted); transition: color var(--transition-fast); }
  :global(.btb-project:hover .btb-project-chev) { color: var(--text-secondary); }
  .btb-demo {
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.4px; font-weight: 700;
    color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent);
    border-radius: var(--radius-sm); padding: 1px 5px;
  }

  /* Tree body */
  .ws-tree { padding: 4px; display: flex; flex-direction: column; }
  .ws-none { padding: 16px 12px; font-size: var(--font-size-xs); color: var(--text-muted); text-align: center; }

  .ws-head {
    display: flex; align-items: center; gap: 9px; width: 100%;
    padding: 7px 9px; background: transparent; border: none; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-primary); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm); font-weight: 500;
    transition: background var(--transition-fast);
  }
  .ws-head:hover { background: var(--bg-hover); }
  .ws-head.active { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .ws-head-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  :global(.ws-head .ws-head-check) { color: var(--accent); flex-shrink: 0; }
  .ws-head-count {
    font-size: var(--font-size-2xs); color: var(--text-muted); background: var(--bg-overlay);
    padding: 1px 7px; border-radius: 9px; font-variant-numeric: tabular-nums; flex-shrink: 0;
  }

  .ws-proj {
    display: flex; align-items: center; gap: 7px; width: 100%;
    padding: 5px 9px 5px 0; background: transparent; border: none; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-secondary); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ws-proj:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ws-proj.active { color: var(--text-primary); }
  /* Indent rail under the workspace monogram (mimics a tree connector). */
  .ws-proj-rail { flex: 0 0 18px; margin-left: 9px; align-self: stretch; border-left: 1px solid var(--border-subtle); }
  :global(.ws-proj .ws-proj-icon) { color: var(--text-muted); flex-shrink: 0; }
  .ws-proj-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  :global(.ws-proj .ws-proj-check) { color: var(--accent); flex-shrink: 0; }

  .ws-foot {
    display: flex; align-items: center; gap: 9px; width: 100%;
    padding: 7px 9px; background: transparent; border: none; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-secondary); cursor: pointer;
    font-size: var(--font-size-sm); font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ws-foot:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .ws-foot:disabled { opacity: 0.45; cursor: default; }
</style>
