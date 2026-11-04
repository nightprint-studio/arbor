<script lang="ts">
  /**
   * BennuWorkspaceSwitcher — Bennu's wiring of the shared title-bar switcher (`shared/internal`): the widget
   * draws it; this says what each action does to `workspacesStore`. The folder picker for Add project lives
   * in the parent title bar (shared with the hamburger + Ctrl+O), so it is delegated up via `onOpenPicker`.
   */
  import WorkspaceSwitcher from '$lib/components/shared/internal/WorkspaceSwitcher.svelte';
  import type { WorkspaceManagerItem } from '$lib/components/shared/internal/workspace-manager';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { workspacesStore, wsColorVar } from '$lib/stores/bennu/workspaces.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  let { onOpenPicker }: { onOpenPicker: (mode: 'open' | 'add') => void } = $props();

  const items = $derived<WorkspaceManagerItem[]>(
    workspacesStore.workspaces.map((w) => ({
      id: w.id, name: w.name, colorIdx: w.color_idx,
      projects: workspacesStore.allProjects().filter((p) => p.workspaceId === w.id).map((p) => ({ root: p.root, name: p.name })),
    })),
  );

  /** A new workspace exists to hold a project, so the picker is what should be in front of you; the name comes from the first project. */
  async function newWorkspace() {
    await workspacesStore.create('');
    onOpenPicker('add');
  }
</script>

<WorkspaceSwitcher
  workspaces={items}
  activeId={workspacesStore.activeId}
  activeRoot={projectStore.project?.root ?? null}
  projectName={projectStore.project?.name ?? 'No project'}
  colorVar={wsColorVar}
  tag={projectStore.isDemo ? 'demo' : undefined}
  onSwitchWorkspace={(id) => void workspacesStore.switchTo(id)}
  onSwitchProject={(id, root) => void workspacesStore.switchToProject(id, root)}
  onAddProject={() => onOpenPicker('add')}
  onNewWorkspace={() => void newWorkspace()}
  onManage={() => bennuUiStore.openWorkspaceManager()}
/>
