/**
 * The verbs of a row in the collections tree, once: the context menu lists them, the keyboard (F2, Del)
 * reaches the same ones. What a row can do depends on what it is — a collection and a folder hold things,
 * a request is sent.
 */
import { Link, Upload, Copy, FilePlus2, FolderPlus, Pencil, Play, Settings2, Terminal, Trash2, FileText } from 'lucide-svelte';
import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
import { hirundoCommands } from '../hirundo-commands';
import type { TreeRow } from '../request-model';

export function menuFor(row: TreeRow): MenuItem[] {
  if (row.kind === 'request') {
    return [
      { id: 'open', label: 'Open', icon: FileText },
      { id: 'duplicate', label: 'Duplicate', icon: Copy },
      { id: 'rename', label: 'Rename…', icon: Pencil, shortcut: 'F2' },
      { id: 'sep1', label: '', separator: true },
      { id: 'copy-curl', label: 'Copy as cURL', icon: Terminal },
      { id: 'copy-path', label: 'Copy Path', icon: Link },
      { id: 'sep2', label: '', separator: true },
      { id: 'delete', label: 'Delete', icon: Trash2, danger: true, shortcut: 'Del' },
    ];
  }
  return [
    { id: 'new-request', label: 'New request', icon: FilePlus2 },
    { id: 'new-folder', label: 'New folder…', icon: FolderPlus },
    { id: 'sep1', label: '', separator: true },
    { id: 'run', label: 'Run…', icon: Play },
    { id: 'settings', label: row.kind === 'collection' ? 'Collection settings — auth, headers, variables…' : 'Folder settings — auth, headers, variables…', icon: Settings2 },
    { id: 'rename', label: 'Rename…', icon: Pencil, shortcut: 'F2' },
    { id: 'export-http', label: 'Export as a .http file…', icon: Upload },
    { id: 'copy-path', label: 'Copy Path', icon: Link },
    { id: 'sep2', label: '', separator: true },
    { id: 'delete', label: 'Delete', icon: Trash2, danger: true, shortcut: 'Del' },
  ];
}

/** Perform a verb on a row. Returns whether the id was one of the row's. */
export function perform(id: string, row: TreeRow): boolean {
  switch (id) {
    case 'open': void hirundoStore.openRequest(row.id, false); return true;
    case 'duplicate': {
      void hirundoStore.openRequest(row.id, false).then(() => {
        const t = hirundoStore.tabs.find((x) => x.path === row.id);
        if (t) void hirundoStore.duplicate(t);
      });
      return true;
    }
    case 'rename': hirundoUiStore.open({ kind: 'rename', path: row.id, name: row.name }); return true;
    case 'copy-curl': void hirundoCommands.copyCurlOfPath(row.id); return true;
    case 'copy-path': hirundoCommands.copyNodePath(row.id); return true;
    case 'delete': hirundoUiStore.open({ kind: 'delete', path: row.id, name: row.name }); return true;
    case 'new-request': hirundoStore.newRequest(row.id); return true;
    case 'new-folder': hirundoUiStore.open({ kind: 'new-folder', parent: row.id }); return true;
    case 'run': hirundoCommands.runContainer(row.id); return true;
    case 'export-http': hirundoCommands.exportHttp(row.id); return true;
    case 'settings': hirundoUiStore.open({ kind: 'meta', path: row.id }); return true;
    default: return false;
  }
}

/** The keys a focused row answers to. Returns whether the key was handled. */
export function onRowKey(row: TreeRow, e: KeyboardEvent): boolean {
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  if (e.key === 'F2') return perform('rename', row);
  if (e.key === 'Delete') return perform('delete', row);
  if (e.key === 'Enter' && row.kind === 'request') return perform('open', row);
  return false;
}
