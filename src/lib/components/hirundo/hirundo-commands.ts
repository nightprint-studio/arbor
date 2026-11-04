/**
 * The verbs of the window, once. The keyboard handler, the palette, the toolbar and the context menus all
 * call these, so a verb is one thing with one behaviour — not four slightly different ones.
 */
import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { curlExport, requestGet } from '$lib/ipc/hirundo';
import { hirundoDetected, pathToTemplate } from '$lib/stores/hirundo/detected.svelte';
import { hirundoRunner } from '$lib/stores/hirundo/runner.svelte';
import { hirundoSse } from '$lib/stores/hirundo/sse.svelte';
import { hirundoStore, type RequestTab } from '$lib/stores/hirundo/workspace.svelte';
import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
import { blankSoapRequest, isStreamRequest } from './request-model';
import { missingRequired } from './editor/completions';

function tab(): RequestTab | null {
  return hirundoStore.active;
}

async function copy(text: string, done: string) {
  try {
    await navigator.clipboard.writeText(text);
    toastStore.show(done, 'success');
  } catch {
    toastStore.show('Could not reach the clipboard.', 'error');
  }
}

/** The endpoint Bennu detected in the code that this request is for (same method, same path), if any. */
export function detectedEndpointOf(t: RequestTab) {
  const path = pathOf(t.request.url);
  return hirundoDetected.groups
    .flatMap((g) => g.endpoints)
    .find((e) => pathToTemplate(e.path) === path && (e.methods.length === 0 || e.methods.some((m) => m.toUpperCase() === t.request.method))) ?? null;
}

/** The detected endpoint of a request together with the Bennu project it was found in. */
export function detectedMatchOf(t: RequestTab) {
  const path = pathOf(t.request.url);
  for (const g of hirundoDetected.groups) {
    const endpoint = g.endpoints.find((e) => pathToTemplate(e.path) === path && (e.methods.length === 0 || e.methods.some((m) => m.toUpperCase() === t.request.method)));
    if (endpoint) return { root: g.root, endpoint };
  }
  return null;
}

/** The file of a saved request tab, or `null` (with a hint) when there is none yet. */
function tabFile(id: string | null): string | null {
  const t = hirundoStore.tabs.find((x) => x.id === id);
  if (!t?.path || !hirundoStore.project) {
    toastStore.show('Save the request first: its file is its address.', 'info');
    return null;
  }
  return `${hirundoStore.project.replace(/[\\/]+$/, '')}/.hirundo/collections/${t.path}`;
}

/** The path part of a request URL: no scheme, host or `{{baseUrl}}` prefix, no query. */
export function pathOf(url: string): string {
  const noQuery = url.split('?')[0];
  const noHost = noQuery.replace(/^\{\{[^}]+\}\}/, '').replace(/^[a-z]+:\/\/[^/]+/i, '');
  return noHost.startsWith('/') ? noHost : `/${noHost}`;
}

export const hirundoCommands = {
  /** Send; first ask when the request still has fields marked required that are empty. */
  send() {
    const t = tab();
    if (!t) return;
    const missing = missingRequired(t.request);
    if (missing.length) hirundoUiStore.open({ kind: 'confirm-send', missing });
    else if (isStreamRequest(t.request)) hirundoCommands.startStream();
    else void hirundoStore.send();
  },
  cancel: () => void hirundoStore.cancel(),

  /** An unsaved request goes into the first collection; with none, the user is asked to make one. */
  save() {
    const t = tab();
    if (!t) return;
    if (!t.path && !(t.parent ?? hirundoStore.tree[0]?.path)) hirundoUiStore.open({ kind: 'new-collection' });
    else void hirundoStore.save(t);
  },
  saveAs: () => hirundoUiStore.open({ kind: 'save-as' }),
  duplicate() {
    const t = tab();
    if (t) void hirundoStore.duplicate(t);
  },
  newRequest: () => hirundoStore.newRequest(hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? null),
  newCollection: () => hirundoUiStore.open({ kind: 'new-collection' }),
  newSoapRequest: () => hirundoStore.newRequest(hirundoStore.active?.parent ?? hirundoStore.tree[0]?.path ?? null, blankSoapRequest()),
  importSoap: () => hirundoUiStore.open({ kind: 'soap-import' }),
  importOpenApi: () => hirundoUiStore.open({ kind: 'openapi-import' }),
  startStream() {
    const t = tab();
    if (!t) return;
    hirundoUiStore.showRight('stream');
    void hirundoSse.start(t);
  },
  importHttp: () => hirundoUiStore.open({ kind: 'http-import' }),
  exportHttp: (path: string) => hirundoUiStore.open({ kind: 'http-export', path }),
  closeTab() {
    const t = tab();
    if (t) hirundoStore.closeTabs('one', t.id);
  },
  /** Copy the file (request) or folder (collection) behind a row of the collections tree. */
  copyNodePath(path: string) {
    const project = hirundoStore.project;
    if (!project) return;
    void copy(`${project.replace(/[\\/]+$/, '')}/.hirundo/collections/${path}`, 'Path copied.');
  },
  closeOtherTabs: () => hirundoStore.closeTabs('others'),
  closeTabsToRight: () => hirundoStore.closeTabs('right'),
  closeAllTabs: () => hirundoStore.closeTabs('all'),
  closeSavedTabs: () => hirundoStore.closeTabs('saved'),
  moveTabLeft: () => hirundoStore.moveActive(-1),
  moveTabRight: () => hirundoStore.moveActive(1),
  renameTab(id: string | null = hirundoStore.activeId) {
    const t = hirundoStore.tabs.find((x) => x.id === id);
    if (t) hirundoUiStore.open({ kind: 'rename-tab', id: t.id, name: t.request.name });
  },
  /** Copy the file of a request tab (the clicked one, or the active one). */
  copyTabPath(id: string | null = hirundoStore.activeId) {
    const file = tabFile(id);
    if (file) void copy(file, 'Path of the request file copied.');
  },
  async revealTab(id: string | null = hirundoStore.activeId) {
    const file = tabFile(id);
    if (!file) return;
    try { await revealItemInDir(file); } catch (e) { toastStore.show(String(e), 'error'); }
  },

  /** Pretty-print a JSON body; anything that is not JSON is left alone, and said so. */
  formatBody() {
    const t = tab();
    if (!t) return;
    if (t.request.body.kind !== 'json') {
      toastStore.show('Only a JSON body can be formatted.', 'info');
      return;
    }
    try {
      t.request.body.text = JSON.stringify(JSON.parse(t.request.body.text), null, 2);
      hirundoStore.touch(t);
    } catch (e) {
      toastStore.show(`The body is not valid JSON: ${e instanceof Error ? e.message : e}`, 'warning');
    }
  },

  curl: () => hirundoUiStore.open({ kind: 'curl' }),
  async copyCurl(resolved = true) {
    const t = tab();
    if (!t || !hirundoStore.project) return;
    try {
      await copy(await curlExport(hirundoStore.target(t), resolved), resolved ? 'Copied as cURL.' : 'Copied as cURL, variables kept.');
    } catch (e) {
      toastStore.show(String(e), 'error');
    }
  },

  /** Copy any request of the tree as cURL, opened or not. */
  async copyCurlOfPath(path: string) {
    const project = hirundoStore.project;
    if (!project) return;
    try {
      const request = await requestGet(project, path);
      await copy(await curlExport({ project, request, path }, true), 'Copied as cURL.');
    } catch (e) {
      toastStore.show(String(e), 'error');
    }
  },

  /** The request's file, for a chat or a commit message: there is no deep link to a request, the file is the address. */
  copyLink() {
    const t = tab();
    if (!t?.path || !hirundoStore.project) {
      toastStore.show('Save the request first: its file is its address.', 'info');
      return;
    }
    const root = hirundoStore.project.replace(/[\\/]+$/, '');
    void copy(`${root}/.hirundo/collections/${t.path}`, 'Path of the request file copied.');
  },

  /** The controller of a request, when Bennu knows it: opens its source file in the default editor. */
  async goToController() {
    const t = tab();
    if (!t) return;
    if (!hirundoDetected.groups.length) await hirundoDetected.refresh();
    const hit = detectedEndpointOf(t);
    if (!hit) {
      toastStore.show('Bennu has no endpoint for this URL in the chosen projects.', 'info');
      return;
    }
    try { await openPath(hit.file); } catch (e) { toastStore.show(String(e), 'error'); }
  },

  /** Run a collection or folder; a protected environment is asked about once, up front. */
  runContainer(path: string) {
    hirundoUiStore.showRight('runner');
    if (hirundoStore.activeEnvType?.confirm_mutating) hirundoUiStore.open({ kind: 'run', path });
    else void hirundoRunner.run(path, { confirmed: false });
  },

  /** Keep the answer next to the request: appended to its Docs as an example. */
  saveResponseAsExample() {
    const t = tab();
    if (!t?.response) {
      toastStore.show('Send the request first.', 'info');
      return;
    }
    const r = t.response;
    const example = `\n\n### Example — ${r.status} ${r.status_text}\n\n\`\`\`\n${r.body.slice(0, 20000)}\n\`\`\`\n`;
    t.request.docs = (t.request.docs + example).trimStart();
    hirundoStore.touch(t);
    toastStore.show('Saved in the Docs tab. Save the request to keep it.', 'success');
  },

  toggleBulk: () => hirundoUiStore.toggleBulk(),
  variables: () => hirundoUiStore.open({ kind: 'variables' }),
  requestSettings: () => hirundoUiStore.open({ kind: 'request-settings' }),
  diff: () => hirundoUiStore.open({ kind: 'diff' }),
  environments: () => hirundoUiStore.open({ kind: 'environments' }),
  workspaces: () => hirundoUiStore.open({ kind: 'workspaces' }),
  findRequest: () => hirundoUiStore.openSearch(),
  /** The folder picker straight away: the project is the folder you choose. */
  openFolder: () => hirundoUiStore.open({ kind: 'open-folder' }),
  newProject: () => hirundoUiStore.open({ kind: 'new-project' }),
  openProject: () => hirundoUiStore.open({ kind: 'project-picker' }),
  schedule: (id: string | null = null) => hirundoUiStore.open({ kind: 'schedule', id }),
};
