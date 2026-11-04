<script lang="ts">
  /**
   * Every overlay the window can put in front, and the catalogue that reaches them: the command palette,
   * the request finder (Ctrl+P), the dialogs, the confirmation a protected environment asks for, the docs.
   *
   * Each dialog is rendered from the `dialog` slot of the ui store rather than from the control that opens
   * it — most have several ways in (a button, the palette, a shortcut) and must not depend on which.
   */
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import PromptModal from '$lib/components/shared/PromptModal.svelte';
  import CommandPaletteShell, { type PaletteSection } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { folderCreate, collectionCreate, requestSave } from '$lib/ipc/hirundo';
  import { fuzzyMatch } from '$lib/utils/fuzzy';
  import { paletteIcon } from '$lib/utils/plugin-icons';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import HirundoDocsPanel from './HirundoDocsPanel.svelte';
  import EnvironmentsModal from './env/EnvironmentsModal.svelte';
  import CurlModal from './modals/CurlModal.svelte';
  import DiffModal from './modals/DiffModal.svelte';
  import GenerateCodeModal from './modals/GenerateCodeModal.svelte';
  import MetaModal from './modals/MetaModal.svelte';
  import ProjectPickerModal from './modals/ProjectPickerModal.svelte';
  import ScheduleModal from './modals/ScheduleModal.svelte';
  import SoapImportModal from './modals/SoapImportModal.svelte';
  import OpenApiImportModal from './modals/OpenApiImportModal.svelte';
  import HttpImportModal from './modals/HttpImportModal.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { httpExport } from '$lib/ipc/hirundo';
  import { findContainer, requestsUnder } from './request-model';

  /** Every request under a collection or folder, written to `file` as a .http text. */
  async function exportHttp(path: string, file: string) {
    const project = hirundoStore.project;
    const container = findContainer(hirundoStore.tree, path);
    if (!project || !container) return;
    try {
      const paths = requestsUnder(container.children).map((r) => r.path);
      await httpExport(project, paths, file);
      toastStore.show(`${paths.length} request${paths.length === 1 ? '' : 's'} written to ${file}.`, 'success');
    } catch (e) { toastStore.show(String(e), 'error'); }
  }
  import WorkspacesModal from './modals/WorkspacesModal.svelte';
  import { hirundoRunner } from '$lib/stores/hirundo/runner.svelte';
  import RequestSettingsModal from './modals/RequestSettingsModal.svelte';
  import VariablesModal from './modals/VariablesModal.svelte';
  import { buildHirundoPalette, type HirundoPaletteActions } from './hirundo-palette';
  import { hirundoCommands as c } from './hirundo-commands';
  import { cloneRequest, collectionOf, flattenRequests, methodShort } from './request-model';

  let paletteQuery = $state('');
  let searchQuery = $state('');

  /** Close the palette, then act: an action that opens a dialog must not mount it under the palette. */
  function run(fn: () => void) {
    hirundoUiStore.closePalette();
    queueMicrotask(fn);
  }

  const actions: HirundoPaletteActions = {
    run,
    send: c.send, cancel: c.cancel, save: c.save, duplicate: c.duplicate, newRequest: c.newRequest,
    newCollection: () => hirundoUiStore.open({ kind: 'new-collection' }),
    curl: c.curl, copyCurl: () => void c.copyCurl(true), formatBody: c.formatBody, bulkEdit: c.toggleBulk,
    variables: c.variables, requestSettings: c.requestSettings, diff: c.diff, closeTab: c.closeTab,
    findRequest: c.findRequest,
    openRequest: (path) => void hirundoStore.openRequest(path, false),
    environments: c.environments,
    switchEnvironment: (id) => void hirundoStore.selectEnvironment(id),
    openProject: c.openProject,
    openRecent: (p) => void hirundoStore.openProject(p),
    showSection: (id) => hirundoUiStore.selectSection(id),
    toggleSidebar: () => hirundoUiStore.toggleSidebar(),
    openDocs: (s) => hirundoUiStore.openDocs(s),
    showRight: (id) => hirundoUiStore.showRight(id),
    newSoapRequest: c.newSoapRequest,
    importSoap: c.importSoap,
    importOpenApi: c.importOpenApi,
    importHttp: c.importHttp,
    startStream: c.startStream,
    exportHttp: c.exportHttp,
    toggleBottom: () => hirundoUiStore.toggleBottom(),
    askImber: () => void c.askImber(),
    goToController: () => void c.goToController(),
    copyLink: c.copyLink,
    saveAs: c.saveAs,
    workspaces: c.workspaces,
    switchWorkspace: (id) => void hirundoStore.switchWorkspace(id),
    newSchedule: () => c.schedule(null),
    runCollection: (path) => c.runContainer(path),
  };

  const requests = $derived(flattenRequests(hirundoStore.tree));

  const paletteSections = $derived<PaletteSection[]>(
    buildHirundoPalette(paletteQuery, {
      projectOpen: !!hirundoStore.project,
      hasRequest: !!hirundoStore.active,
      sending: !!hirundoStore.active?.sending,
      requests,
      environments: hirundoStore.envs.environments.map((e) => ({ id: e.id, name: e.name })),
      recents: hirundoStore.recents.filter((p) => p !== hirundoStore.project),
      workspaces: hirundoStore.workspaces.filter((w) => w.id !== hirundoStore.config.active_workspace).map((w) => ({ id: w.id, name: w.name })),
      collections: hirundoStore.tree.map((t) => ({ path: t.path, name: t.name })),
    }, actions),
  );

  /** Ctrl+P: only requests, ranked; Enter opens one as a tab that stays. */
  const searchSections = $derived<PaletteSection[]>([
    {
      id: 'requests',
      label: 'Requests',
      items: requests
        .map((r) => ({ r, m: fuzzyMatch(`${r.name} ${r.where}`, searchQuery) }))
        .filter((x) => x.m)
        .sort((a, b) => (b.m?.score ?? 0) - (a.m?.score ?? 0))
        .slice(0, 60)
        .map(({ r }) => ({
          id: r.path, title: r.name, subtitle: `${methodShort(r.method)} · ${r.where}`, icon: 'Send',
          action: () => { hirundoUiStore.closeSearch(); void hirundoStore.openRequest(r.path, false); },
        })),
    },
  ]);

  const dialog = $derived(hirundoUiStore.dialog);
  const close = () => hirundoUiStore.closeDialog();

  async function createCollection(name: string) {
    close();
    const project = hirundoStore.project;
    if (!project) return;
    try {
      await collectionCreate(project, name);
      await hirundoStore.refreshTree();
    } catch (e) { toastStore.show(String(e), 'error'); }
  }

  async function createFolder(parent: string, name: string) {
    close();
    const project = hirundoStore.project;
    if (!project) return;
    try {
      await folderCreate(project, parent, name);
      await hirundoStore.refreshTree();
    } catch (e) { toastStore.show(String(e), 'error'); }
  }

  /** Save as: a copy under a new name, next to the original (or in the first collection), opened in its own tab. */
  async function saveAs(name: string) {
    close();
    const tab = hirundoStore.active;
    const project = hirundoStore.project;
    if (!tab || !project) return;
    const parent = tab.path ? tab.path.split('/').slice(0, -1).join('/') : tab.parent ?? hirundoStore.tree[0]?.path;
    if (!parent) { hirundoUiStore.open({ kind: 'new-collection' }); return; }
    try {
      const copy = { ...cloneRequest(tab.request), name };
      const path = await requestSave(project, copy, { parent });
      await hirundoStore.refreshTree();
      await hirundoStore.openRequest(path, false);
    } catch (e) { toastStore.show(String(e), 'error'); }
  }

  async function confirmDelete(path: string) {
    close();
    await hirundoStore.deleteNode(path);
  }
</script>

{#if hirundoUiStore.paletteOpen}
  <CommandPaletteShell
    onClose={() => hirundoUiStore.closePalette()}
    iconResolver={paletteIcon}
    sections={paletteSections}
    bind:query={paletteQuery}
    placeholder="Type a command or a request name…"
  />
{/if}

{#if hirundoUiStore.searchOpen}
  <CommandPaletteShell
    onClose={() => hirundoUiStore.closeSearch()}
    iconResolver={paletteIcon}
    sections={searchSections}
    bind:query={searchQuery}
    placeholder="Find a request…"
  />
{/if}

{#if dialog?.kind === 'environments'}<EnvironmentsModal onClose={close} />{/if}
{#if dialog?.kind === 'curl'}<CurlModal onClose={close} />{/if}
{#if dialog?.kind === 'variables'}<VariablesModal onClose={close} />{/if}
{#if dialog?.kind === 'request-settings'}<RequestSettingsModal onClose={close} />{/if}
{#if dialog?.kind === 'generate-code'}<GenerateCodeModal onClose={close} />{/if}
{#if dialog?.kind === 'diff'}<DiffModal onClose={close} />{/if}
{#if dialog?.kind === 'project-picker'}<ProjectPickerModal onClose={close} workspaceId={dialog.workspaceId} />{/if}
{#if dialog?.kind === 'http-import'}<HttpImportModal onClose={close} />{/if}
{#if dialog?.kind === 'http-export'}
  {@const path = dialog.path}
  <FileExplorerModal mode="save" title="Export as a .http file" extensions={['http']} initialFilename={`${path.split('/').pop()}.http`} onConfirm={(file: string) => { close(); void exportHttp(path, file); }} onCancel={close} onClose={close} />
{/if}
{#if dialog?.kind === 'openapi-import'}<OpenApiImportModal onClose={close} />{/if}
{#if dialog?.kind === 'soap-import'}<SoapImportModal onClose={close} />{/if}
{#if dialog?.kind === 'workspaces'}<WorkspacesModal onClose={close} />{/if}
{#if dialog?.kind === 'schedule'}<ScheduleModal id={dialog.id} onClose={close} />{/if}
{#if dialog?.kind === 'meta'}<MetaModal path={dialog.path} onClose={close} />{/if}

{#if dialog?.kind === 'new-collection'}
  <PromptModal title="New collection" label="Name of the collection" placeholder="Bandi" confirmLabel="Create" onConfirm={createCollection} onCancel={close} />
{/if}
{#if dialog?.kind === 'new-folder'}
  {@const parent = dialog.parent}
  <PromptModal title="New folder" label={`Name of the folder in ${collectionOf(parent)}`} confirmLabel="Create" onConfirm={(n) => void createFolder(parent, n)} onCancel={close} />
{/if}
{#if dialog?.kind === 'rename'}
  {@const path = dialog.path}
  <PromptModal title="Rename" label="New name" initialValue={dialog.name} confirmLabel="Rename" onConfirm={(n) => { close(); void hirundoStore.renameNode(path, n); }} onCancel={close} />
{/if}
{#if dialog?.kind === 'run'}
  {@const path = dialog.path}
  <ConfirmModal
    title={`Run against ${hirundoStore.activeEnv?.name ?? 'this environment'}?`}
    message={`${hirundoStore.activeEnvType?.name ?? 'This'} environment is protected. The run sends every request, including POST, PUT, PATCH and DELETE.`}
    variant="warning"
    confirmLabel="Run"
    onConfirm={() => { close(); void hirundoRunner.run(path, { confirmed: true }); }}
    onCancel={close}
  />
{/if}
{#if dialog?.kind === 'save-as'}
  <PromptModal title="Save as" label="Name of the copy" initialValue={`${hirundoStore.active?.request.name ?? ''} copy`} confirmLabel="Save" onConfirm={(n) => void saveAs(n)} onCancel={close} />
{/if}
{#if dialog?.kind === 'delete'}
  {@const path = dialog.path}
  <ConfirmModal
    title="Delete"
    message={`Delete “${dialog.name}”?`}
    detail={path.endsWith('.toml') ? 'The request file is removed from the project.' : 'Everything inside it is removed from the project.'}
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => void confirmDelete(path)}
    onCancel={close}
  />
{/if}

{#if hirundoStore.pending}
  {@const p = hirundoStore.pending}
  <ConfirmModal
    title={`Send to ${p.env?.name ?? 'a protected environment'}?`}
    message={`${p.method} ${p.url}`}
    detail={`${p.env?.type_name ?? 'This'} environment asks for a confirmation before a request that changes things.`}
    variant="warning"
    confirmLabel="Send"
    onConfirm={() => void hirundoStore.confirmPending()}
    onCancel={() => hirundoStore.dismissPending()}
  />
{/if}

{#if hirundoUiStore.docsOpen}
  <HirundoDocsPanel onClose={() => hirundoUiStore.closeDocs()} initialSection={hirundoUiStore.docsSection || 'getting-started'} />
{/if}
