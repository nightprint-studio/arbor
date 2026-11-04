/** Gallery scene overrides and extra scenes for the "hirundo" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';
import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';

const noop = () => {};

/** A project with a request open and answered, so the window and the modals show real content. */
async function seedHirundo() {
  await hirundoStore.openProject('/Users/dev/gestionale');
  await hirundoStore.openRequest('bandi/lista-bandi.toml', false);
  await hirundoStore.send();
}

export const hirundoOverrides: Record<string, SceneOverride> = {
  'hirundo/env/EnvironmentsModal': { props: { onClose: noop }, setup: seedHirundo, settle: 900 },
  'hirundo/modals/CurlModal': { props: { onClose: noop }, setup: seedHirundo, settle: 700 },
  'hirundo/modals/VariablesModal': { props: { onClose: noop }, setup: seedHirundo, settle: 900 },
  'hirundo/modals/RequestSettingsModal': { props: { onClose: noop }, setup: seedHirundo, settle: 700 },
  'hirundo/modals/GenerateCodeModal': { props: { onClose: noop }, setup: seedHirundo, settle: 900 },
  'hirundo/modals/DiffModal': { props: { onClose: noop }, setup: async () => { await seedHirundo(); await hirundoStore.send(); }, settle: 900 },
  'hirundo/modals/MetaModal': { props: { onClose: noop, path: 'bandi' }, setup: seedHirundo, settle: 900 },
  'hirundo/modals/ProjectPickerModal': { props: { onClose: noop }, setup: seedHirundo, settle: 700 },
  'hirundo/modals/OpenApiImportModal': { props: { onClose: noop }, setup: seedHirundo, settle: 700 },
  'hirundo/modals/SoapImportModal': { props: { onClose: noop }, setup: seedHirundo, settle: 700 },
  'hirundo/modals/HttpImportModal': { props: { onClose: noop }, setup: seedHirundo, settle: 700 },
  'hirundo/modals/NewCollectionModal': { props: { onCreate: noop, onClose: noop }, setup: seedHirundo, settle: 600 },
  'hirundo/modals/CreateVariableModal': { props: { name: 'baseUrl', requestPath: null, onClose: noop }, setup: seedHirundo, settle: 600 },
  'hirundo/editor/MethodSelect': { props: { value: 'POST', onchange: noop }, setup: seedHirundo, settle: 300 },
  'hirundo/modals/NewProjectModal': { props: { onClose: noop }, setup: seedHirundo, settle: 600 },
  'hirundo/modals/WorkspacesModal': { props: { onClose: noop }, setup: seedHirundo, settle: 900 },
  'hirundo/modals/ScheduleModal': { props: { onClose: noop, id: null }, setup: seedHirundo, settle: 900 },
};

export const hirundoExtras: ExtraScene[] = [
  {
    id: 'hirundo/HirundoShell',
    load: () => import('$lib/components/hirundo/HirundoShell.svelte'),
    setup: seedHirundo,
    settle: 1400,
  },
];
