<script lang="ts">
  /**
   * HirundoWorkspaceSwitcher — Hirundo's wiring of the shared title-bar switcher (the one Bennu uses): the
   * widget draws it; this says what each action does to Hirundo's workspaces.
   */
  import { FolderOpen } from 'lucide-svelte';
  import WorkspaceSwitcher from '$lib/components/shared/internal/WorkspaceSwitcher.svelte';
  import type { WorkspaceManagerItem } from '$lib/components/shared/internal/workspace-manager';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { wsColor } from '../request-model';

  const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const items = $derived<WorkspaceManagerItem[]>(
    hirundoStore.workspaces.map((w) => ({ id: w.id, name: w.name, colorIdx: w.color_idx, projects: w.projects.map((root) => ({ root, name: base(root) })) })),
  );
</script>

<WorkspaceSwitcher
  workspaces={items}
  activeId={hirundoStore.config.active_workspace}
  activeRoot={hirundoStore.project}
  projectName={hirundoStore.project ? base(hirundoStore.project) : 'No project'}
  colorVar={wsColor}
  extraActions={[{ id: 'new-project', label: 'New project…', icon: FolderOpen, onclick: () => hirundoUiStore.open({ kind: 'new-project' }) }]}
  onSwitchWorkspace={(id) => void hirundoStore.switchWorkspace(id)}
  onSwitchProject={(id, root) => void hirundoStore.switchToProject(id, root)}
  onAddProject={() => hirundoUiStore.open({ kind: 'project-picker', workspaceId: hirundoStore.activeWorkspace?.id })}
  onNewWorkspace={() => void hirundoStore.createWorkspace('New workspace').then(() => hirundoUiStore.open({ kind: 'workspaces' }))}
  onManage={() => hirundoUiStore.open({ kind: 'workspaces' })}
/>
