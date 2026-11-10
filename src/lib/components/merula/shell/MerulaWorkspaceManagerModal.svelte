<script lang="ts">
  /**
   * MerulaWorkspaceManagerModal — merula's wiring of the shared workspace manager (`shared/internal`),
   * the board Bennu uses: the widget owns the board, the settings panel, the keyboard and the
   * confirmations; this says what each action does to `workspaceStore`.
   */
  import WorkspaceManagerModal from '$lib/components/shared/internal/WorkspaceManagerModal.svelte';
  import type { WorkspaceManagerItem } from '$lib/components/shared/internal/workspace-manager';
  import { WS_COLOR_COUNT, wsColorVar } from '$lib/utils/workspace-colors';
  import { projectStore } from '../stores/project.svelte';
  import { workspaceStore } from '../stores/workspace.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const items = $derived<WorkspaceManagerItem[]>(
    workspaceStore.workspaces.map((w) => ({
      id: w.id, name: w.name, colorIdx: w.color_idx,
      projects: w.project_paths.map((root) => ({ root })),
    })),
  );
</script>

<WorkspaceManagerModal
  workspaces={items}
  activeId={workspaceStore.activeWorkspace ?? workspaceStore.workspaces[0]?.id ?? ''}
  colorCount={WS_COLOR_COUNT}
  colorVar={wsColorVar}
  {onClose}
  onCreate={() => workspaceStore.createWorkspace('New workspace')}
  onSwitchTo={(id) => workspaceStore.setActiveWorkspace(id)}
  onRename={(id, name) => workspaceStore.renameWorkspace(id, name)}
  onColor={(id, idx) => workspaceStore.setWorkspaceColor(id, idx)}
  onDelete={(id) => workspaceStore.deleteWorkspace(id)}
  onAddProject={(id, dir) => workspaceStore.addProjectToWorkspace(id, dir)}
  onRemoveProject={(id, root) => workspaceStore.removeProjectFromWorkspace(id, root)}
  onOpenProject={(id, root) => { workspaceStore.setActiveWorkspace(id); void projectStore.open(root).catch(() => {}); }}
  onMoveProject={(from, root, to, index) => workspaceStore.moveProject(from, root, to, index)}
  onMoveWorkspace={(id, index) => workspaceStore.moveWorkspace(id, index)}
/>
