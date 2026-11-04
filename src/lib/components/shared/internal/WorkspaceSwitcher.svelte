<script lang="ts">
  /**
   * WorkspaceSwitcher — the title bar's project/workspace switcher (Corvus-tree style), props-driven so every
   * product that has workspaces shows the same thing.
   *
   * The trigger shows the active project with the active workspace's colour monogram. The dropdown is a
   * two-level tree: each **workspace** a header row (click to switch to it) with its **projects** nested under
   * it (click to go straight to that project, switching workspace first). The footer carries the verbs; a
   * product adds its own with `extraActions` (listed first). The product's stores stay outside: this only
   * calls back.
   */
  import type { Component } from 'svelte';
  import { ChevronDown, FolderPlus, Plus, Layers, Check, FileCode2 } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { WorkspaceManagerItem } from './workspace-manager';

  interface Props {
    workspaces: WorkspaceManagerItem[];
    activeId: string | null;
    /** Root of the open project (marks its row). */
    activeRoot: string | null;
    projectName: string;
    colorVar: (idx: number) => string;
    /** Small tag next to the name (Bennu's "demo"). */
    tag?: string;
    onSwitchWorkspace: (id: string) => void;
    onSwitchProject: (workspaceId: string, root: string) => void;
    onAddProject: () => void;
    onNewWorkspace: () => void;
    onManage: () => void;
    /** Product verbs shown above the shared ones (e.g. "New project…"). */
    extraActions?: { id: string; label: string; icon?: Component<any>; onclick: () => void }[];
  }

  let {
    workspaces, activeId, activeRoot, projectName, colorVar, tag,
    onSwitchWorkspace, onSwitchProject, onAddProject, onNewWorkspace, onManage, extraActions = [],
  }: Props = $props();

  const active = $derived(workspaces.find((w) => w.id === activeId) ?? null);
  const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
</script>

<Dropdown position="fixed" direction="down" width="300px">
  {#snippet trigger({ open, toggle })}
    <button
      type="button"
      class="wsw-project"
      class:open
      onclick={toggle}
      use:tooltip={active ? `Workspace: ${active.name} — switch project / workspace` : 'Switch project'}
      aria-haspopup="menu"
      aria-expanded={open}
    >
      <Monogram name={projectName} size={22} color={active ? colorVar(active.colorIdx) : undefined} />
      <span class="wsw-project-name">{projectName}</span>
      {#if tag}<span class="wsw-tag">{tag}</span>{/if}
      <ChevronDown size={12} class="wsw-chev" />
    </button>
  {/snippet}

  {#snippet children({ close })}
    <div class="wsw-tree" role="menu">
      {#if workspaces.length === 0}
        <div class="wsw-none">No workspace yet — open a project to start one.</div>
      {/if}
      {#each workspaces as w (w.id)}
        {@const isActiveWs = w.id === activeId}
        <button type="button" class="wsw-head" class:active={isActiveWs} onclick={() => { close(); onSwitchWorkspace(w.id); }} role="menuitem">
          <Monogram name={w.name || 'Workspace'} color={colorVar(w.colorIdx)} size={18} />
          <span class="wsw-head-name">{w.name || 'Workspace'}</span>
          {#if isActiveWs}<Check size={12} class="wsw-head-check" />{/if}
          <span class="wsw-head-count">{w.projects.length}</span>
        </button>
        {#each w.projects as p (p.root)}
          {@const isActiveProj = isActiveWs && p.root === activeRoot}
          <button type="button" class="wsw-proj" class:active={isActiveProj} onclick={() => { close(); onSwitchProject(w.id, p.root); }} use:tooltip={p.root} role="menuitem">
            <span class="wsw-proj-rail"></span>
            <FileCode2 size={13} class="wsw-proj-icon" />
            <span class="wsw-proj-name">{p.name ?? base(p.root)}</span>
            {#if isActiveProj}<Check size={11} class="wsw-proj-check" />{/if}
          </button>
        {/each}
      {/each}
    </div>
  {/snippet}

  {#snippet footer({ close })}
    {#each extraActions as a (a.id)}
      {@const Icon = a.icon ?? Plus}
      <button type="button" class="wsw-foot" onclick={() => { close(); a.onclick(); }} role="menuitem"><Icon size={13} /><span>{a.label}</span></button>
    {/each}
    <button type="button" class="wsw-foot" onclick={() => { close(); onAddProject(); }} role="menuitem"><FolderPlus size={13} /><span>Add project…</span></button>
    <button type="button" class="wsw-foot" onclick={() => { close(); onNewWorkspace(); }} role="menuitem"><Plus size={13} /><span>New workspace…</span></button>
    <button type="button" class="wsw-foot" onclick={() => { close(); onManage(); }} role="menuitem"><Layers size={13} /><span>Manage workspaces…</span></button>
  {/snippet}
</Dropdown>

<style>
  .wsw-project {
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
  .wsw-project:hover { background: var(--bg-hover); }
  .wsw-project.open { background: var(--bg-hover); border-color: var(--border-subtle); }
  .wsw-project-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  :global(.wsw-project .wsw-chev) { color: var(--text-muted); transition: color var(--transition-fast); }
  :global(.wsw-project:hover .wsw-chev) { color: var(--text-secondary); }
  .wsw-tag {
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.4px; font-weight: 700;
    color: var(--warning); background: color-mix(in srgb, var(--warning) 18%, transparent);
    border-radius: var(--radius-sm); padding: 1px 5px;
  }

  .wsw-tree { padding: 4px; display: flex; flex-direction: column; }
  .wsw-none { padding: 16px 12px; font-size: var(--font-size-xs); color: var(--text-muted); text-align: center; }

  .wsw-head {
    display: flex; align-items: center; gap: 9px; width: 100%;
    padding: 7px 9px; background: transparent; border: none; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-primary); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm); font-weight: 500;
    transition: background var(--transition-fast);
  }
  .wsw-head:hover { background: var(--bg-hover); }
  .wsw-head.active { background: color-mix(in srgb, var(--accent) 8%, transparent); }
  .wsw-head-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  :global(.wsw-head .wsw-head-check) { color: var(--accent); flex-shrink: 0; }
  .wsw-head-count {
    font-size: var(--font-size-2xs); color: var(--text-muted); background: var(--bg-overlay);
    padding: 1px 7px; border-radius: 9px; font-variant-numeric: tabular-nums; flex-shrink: 0;
  }

  .wsw-proj {
    display: flex; align-items: center; gap: 7px; width: 100%;
    padding: 5px 9px 5px 0; background: transparent; border: none; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-secondary); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .wsw-proj:hover { background: var(--bg-hover); color: var(--text-primary); }
  .wsw-proj.active { color: var(--text-primary); }
  .wsw-proj-rail { flex: 0 0 18px; margin-left: 9px; align-self: stretch; border-left: 1px solid var(--border-subtle); }
  :global(.wsw-proj .wsw-proj-icon) { color: var(--text-muted); flex-shrink: 0; }
  .wsw-proj-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  :global(.wsw-proj .wsw-proj-check) { color: var(--accent); flex-shrink: 0; }

  .wsw-foot {
    display: flex; align-items: center; gap: 9px; width: 100%;
    padding: 7px 9px; background: transparent; border: none; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-secondary); cursor: pointer;
    font-size: var(--font-size-sm); font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .wsw-foot:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
</style>
