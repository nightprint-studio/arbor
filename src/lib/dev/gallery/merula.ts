/** Gallery scene overrides and extra scenes for the "merula" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';

const noop = () => {};
const PROJECT = '/Users/dev/music/nobody-digs-anymore';

/** Open the fixture project in the merula stores, the way the window boot does. */
async function openProject() {
  const { projectStore } = await import('$lib/components/merula/stores/project.svelte');
  const { workspaceStore } = await import('$lib/components/merula/stores/workspace.svelte');
  const { configStore } = await import('$lib/components/merula/stores/config.svelte');
  await workspaceStore.load();
  await configStore.loadConfig?.();
  await projectStore.open(PROJECT);
}

export const merulaOverrides: Record<string, SceneOverride> = {
  'merula/shell/ChordProgModal': { props: { onClose: noop } },
  'merula/shell/EuclidGenModal': { props: { onClose: noop } },
  'merula/shell/ExportAllModal': { props: { onClose: noop }, setup: openProject },
  'merula/shell/ExportOptionsModal': { props: { onClose: noop }, setup: openProject },
  'merula/shell/MarkerRenameModal': { props: { marker: { id: 1, cycle: 8, label: 'Chorus' }, onClose: noop } },
  'merula/shell/MerulaSettingsModal': { props: { onClose: noop }, setup: openProject, settle: 700 },
  'merula/shell/MerulaShortcutsModal': { props: { onClose: noop } },
  'merula/shell/RecentProjectsModal': { props: { onClose: noop }, setup: openProject },
  'merula/shell/RenameFileModal': { props: { path: `${PROJECT}/intro.merula`, currentName: 'intro.merula', onClose: noop }, setup: openProject },
  'merula/shell/RenameProjectModal': { props: { onClose: noop }, setup: openProject },
  'merula/shell/MerulaWorkspaceManagerModal': { props: { onClose: noop }, setup: openProject },
};

export const merulaExtras: ExtraScene[] = [];
