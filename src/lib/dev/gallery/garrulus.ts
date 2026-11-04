/** Gallery scene overrides and extra scenes for the "garrulus" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';

const noop = () => {};

export const garrulusOverrides: Record<string, SceneOverride> = {
  'garrulus/OpenVaultModal': { props: { onClose: noop, onOpened: noop, currentRoot: '/Users/dev/notes/work' } },
  'garrulus/notes/CloseNoteModal': {
    props: { title: 'Sprint planning', path: 'meetings/sprint-planning.md', onSave: noop, onDiscard: noop, onCancel: noop },
  },
  'garrulus/notes/NewNoteModal': { props: { typeId: null, onClose: noop } },
  'garrulus/notes/RenameNoteModal': { props: { path: 'bugs/crash-on-start.md', onClose: noop } },
  'garrulus/notes/TypePickerModal': { props: { placeholder: 'New note of which type…', onPick: noop, onClose: noop } },
  'garrulus/notes/QuickSwitcherModal': { props: { onClose: noop } },
  'garrulus/shell/GarrulusCommitModal': { props: { onClose: noop } },
  'garrulus/shell/GarrulusRemoteModal': { props: { onClose: noop, vaultName: 'work', onChanged: noop } },
};

export const garrulusExtras: ExtraScene[] = [];
