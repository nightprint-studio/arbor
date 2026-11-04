<script lang="ts">
  /**
   * WorkspacesModal — Hirundo's wiring of the shared workspace manager (the one Bennu uses too): the board, the
   * settings panel, the keyboard and the confirmations are the widget's; this says what each action does to
   * Hirundo's workspaces (kept in the profile, served by `hirundo-be`). A workspace is only a list of project
   * folders: deleting one never touches a project.
   */
  import WorkspaceManagerModal from '$lib/components/shared/internal/WorkspaceManagerModal.svelte';
  import type { WorkspaceManagerItem } from '$lib/components/shared/internal/workspace-manager';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { WS_COLOR_COUNT, wsColor } from '../request-model';

  let { onClose }: { onClose: () => void } = $props();

  const items = $derived<WorkspaceManagerItem[]>(
    hirundoStore.workspaces.map((w) => ({ id: w.id, name: w.name, colorIdx: w.color_idx, projects: w.projects.map((root) => ({ root })) })),
  );
  const find = (id: string) => hirundoStore.workspaces.find((w) => w.id === id);

  async function create() {
    await hirundoStore.createWorkspace('New workspace');
    return hirundoStore.config.active_workspace;
  }
</script>

<WorkspaceManagerModal
  workspaces={items}
  activeId={hirundoStore.config.active_workspace}
  colorCount={WS_COLOR_COUNT}
  colorVar={wsColor}
  {onClose}
  onCreate={create}
  onSwitchTo={(id) => hirundoStore.switchWorkspace(id)}
  onRename={(id, name) => { const w = find(id); if (w && name.trim()) void hirundoStore.updateWorkspace(id, name.trim(), w.color_idx); }}
  onColor={(id, idx) => { const w = find(id); if (w) void hirundoStore.updateWorkspace(id, w.name, idx); }}
  onDelete={(id) => void hirundoStore.deleteWorkspace(id)}
  onAddProject={(id, dir) => hirundoStore.addProjectTo(id, dir)}
  onRemoveProject={(id, root) => void hirundoStore.removeProjectFrom(id, root)}
  onOpenProject={(id, root) => void hirundoStore.switchToProject(id, root)}
  onMoveProject={(from, root, to, index) => hirundoStore.moveProject(from, root, to, index)}
  onMoveWorkspace={(id, index) => void hirundoStore.moveWorkspace(id, index)}
/>
