/** Gallery scene overrides and extra scenes for the "garrulus" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';

const noop = () => {};

export const garrulusOverrides: Record<string, SceneOverride> = {
  'garrulus/OpenVaultModal': { props: { onClose: noop, onOpened: noop, currentRoot: '/Users/dev/notes/work' } },
  'garrulus/notes/CloseNoteModal': {
    props: { title: 'Sprint planning', path: 'meetings/sprint-planning.md', onSave: noop, onDiscard: noop, onCancel: noop },
  },
  'garrulus/shell/GarrulusCommitModal': { props: { onClose: noop } },
  'garrulus/shell/GarrulusRemoteModal': { props: { onClose: noop, vaultName: 'work', onChanged: noop } },
};

export const garrulusExtras: ExtraScene[] = [];
