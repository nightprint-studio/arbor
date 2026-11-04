<script lang="ts">
  /**
   * BennuWorkspaceManagerModal — Bennu's wiring of the shared workspace manager (`shared/internal`): the board,
   * the settings panel, the keyboard and the confirmations are the widget's; this file says what each action does
   * to `workspacesStore`. Adding a project routes through the store, which switches to the target workspace first
   * so the add goes through projectStore's live open/index path.
   */
  import WorkspaceManagerModal from '$lib/components/shared/internal/WorkspaceManagerModal.svelte';
  import type { WorkspaceManagerItem } from '$lib/components/shared/internal/workspace-manager';
  import { workspacesStore, wsColorVar, WS_COLOR_COUNT } from '$lib/stores/bennu/workspaces.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const items = $derived<WorkspaceManagerItem[]>(
    workspacesStore.workspaces.map((w) => ({ id: w.id, name: w.name, colorIdx: w.color_idx, projects: w.projects.map((p) => ({ root: p.root })) })),
  );
</script>

<WorkspaceManagerModal
  workspaces={items}
  activeId={workspacesStore.activeId}
  colorCount={WS_COLOR_COUNT}
  colorVar={wsColorVar}
  {onClose}
  onCreate={() => workspacesStore.create('New workspace')}
  onSwitchTo={(id) => workspacesStore.switchTo(id)}
  onRename={(id, name) => workspacesStore.rename(id, name)}
  onColor={(id, idx) => workspacesStore.setColor(id, idx)}
  onDelete={(id) => void workspacesStore.remove(id)}
  onAddProject={(id, dir) => workspacesStore.addProjectTo(id, dir)}
  onRemoveProject={(id, root) => workspacesStore.removeProjectFrom(id, root)}
  onOpenProject={(id, root) => void workspacesStore.switchToProject(id, root)}
  onMoveProject={(from, root, to, index) => workspacesStore.moveProject(from, root, to, index)}
  onMoveWorkspace={(id, index) => workspacesStore.moveWorkspace(id, index)}
/>
