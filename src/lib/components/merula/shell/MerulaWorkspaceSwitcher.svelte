<script lang="ts">
  /**
   * MerulaWorkspaceSwitcher — merula's wiring of the shared title-bar switcher (`shared/internal`), the
   * same widget Bennu uses: the widget draws it; this says what each action does to `workspaceStore`.
   * Merula-specific verbs (open / rename / recent projects) ride along as `extraActions`; the folder
   * picker for "Add project…" lives here because only this component needs it.
   */
  import { FolderOpen, FolderPen, Clock } from 'lucide-svelte';
  import WorkspaceSwitcher from '$lib/components/shared/internal/WorkspaceSwitcher.svelte';
  import type { WorkspaceManagerItem } from '$lib/components/shared/internal/workspace-manager';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { wsColorVar } from '$lib/utils/workspace-colors';
  import { merulaStore } from '../merula-store.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { workspaceStore } from '../stores/workspace.svelte';

  let { onRecent }: { onRecent: () => void } = $props();

  const basename = (p: string) => p.split(/[\/]/).filter(Boolean).pop() ?? p;

  const items = $derived<WorkspaceManagerItem[]>(
    workspaceStore.workspaces.map((w) => ({
      id: w.id, name: w.name, colorIdx: w.color_idx,
      projects: w.project_paths.map((root) => ({ root, name: basename(root) })),
    })),
  );

  const extraActions = $derived([
    { id: 'open', label: 'Open project…', icon: FolderOpen, onclick: () => projectActions.openProject() },
    { id: 'recent', label: 'Recent projects…', icon: Clock, onclick: onRecent },
    ...(projectStore.project
      ? [{ id: 'rename', label: 'Rename project…', icon: FolderPen, onclick: () => merulaStore.openRenameProject() }]
      : []),
  ]);

  /** Opening a project that belongs to another workspace moves the window into that workspace first. */
  function switchToProject(workspaceId: string, root: string) {
    workspaceStore.setActiveWorkspace(workspaceId);
    void projectStore.open(root).catch(() => {});
  }

  let pickerOpen = $state(false);

  /** A folder picked for the active workspace (one is made when there is none yet) and opened. */
  function addPicked(dir: string) {
    pickerOpen = false;
    const id = workspaceStore.activeWorkspace ?? workspaceStore.createWorkspace('Workspace');
    workspaceStore.addProjectToWorkspace(id, dir);
    switchToProject(id, dir);
  }

  /** A new workspace exists to hold a project, so the picker is what should be in front of you. */
  function newWorkspace() {
    workspaceStore.createWorkspace('New workspace');
    pickerOpen = true;
  }
</script>

<WorkspaceSwitcher
  workspaces={items}
  activeId={workspaceStore.activeWorkspace}
  activeRoot={projectStore.project?.path ?? null}
  projectName={projectStore.project?.name ?? 'No project'}
  colorVar={wsColorVar}
  {extraActions}
  onSwitchWorkspace={(id) => workspaceStore.setActiveWorkspace(id)}
  onSwitchProject={switchToProject}
  onAddProject={() => (pickerOpen = true)}
  onNewWorkspace={newWorkspace}
  onManage={() => merulaStore.openWorkspaces()}
/>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Add a merula project to the workspace"
    onConfirm={addPicked}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}
