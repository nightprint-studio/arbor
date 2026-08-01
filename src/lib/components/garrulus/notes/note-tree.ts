/**
 * The vault's folders and notes, as a tree the shared `Tree` widget can draw.
 *
 * Pure: paths in, nodes out. No store, no IPC, no Svelte — which is what makes
 * the shape of the tree something you can reason about without a running vault,
 * and what keeps `NoteTree.svelte` about rows and keys rather than about
 * bookkeeping.
 */

import type { CatalogueNote } from '$lib/stores/garrulus/notes.svelte';

/** A folder of the vault. Its `count` is every note beneath it, not just the
 *  ones it holds directly — a collapsed `diario` saying `3` when it contains 143
 *  notes would be worse than saying nothing. */
export interface NoteFolderNode {
  kind: 'folder';
  /** The folder's vault-relative path — unique across the tree, so it is the id. */
  id: string;
  name: string;
  path: string;
  count: number;
  children: NoteTreeNode[];
}

/** A note of the vault. */
export interface NoteNode {
  kind: 'note';
  /** The note's path, or `uid:<id>` for one that cannot be addressed by path. */
  id: string;
  /** The title, which is what the sidebar shows and what the filter matches. */
  name: string;
  /** Vault-relative path, or `null` — see `notePathOfId`. */
  path: string | null;
  note: CatalogueNote;
  /** Never present. Declared so `Tree`'s `getChildren` is total over the union. */
  children?: undefined;
}

export type NoteTreeNode = NoteFolderNode | NoteNode;

/** Folders before notes, then by name — the order every file tree in the suite
 *  uses, with numeric collation so `2026-07-9` sorts before `2026-07-10`. */
function compareNodes(a: NoteTreeNode, b: NoteTreeNode): number {
  if (a.kind !== b.kind) return a.kind === 'folder' ? -1 : 1;
  return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
}

function noteNode(note: CatalogueNote): NoteNode {
  return {
    kind: 'note',
    id: note.path ?? `uid:${note.id}`,
    name: note.title,
    path: note.path,
    note,
  };
}

/** Sort in place, depth-first, and total each folder's subtree count. */
function settle(folder: NoteFolderNode): number {
  let count = 0;
  for (const child of folder.children) {
    count += child.kind === 'folder' ? settle(child) : 1;
  }
  folder.children.sort(compareNodes);
  folder.count = count;
  return count;
}

/**
 * Build the vault tree.
 *
 * Notes whose id is a frontmatter `uid` have no path to file them under, so they
 * land at the top level rather than being dropped: a note the sidebar refuses to
 * mention is a note the user will conclude is gone.
 */
export function buildNoteTree(notes: CatalogueNote[]): NoteTreeNode[] {
  const root: NoteFolderNode = { kind: 'folder', id: '', name: '', path: '', count: 0, children: [] };
  const folders = new Map<string, NoteFolderNode>([['', root]]);
  const unfiled: NoteNode[] = [];

  for (const note of notes) {
    if (!note.path) {
      unfiled.push(noteNode(note));
      continue;
    }
    const segments = note.path.split('/');
    segments.pop(); // the file itself
    let dir = '';
    let parent = root;
    for (const segment of segments) {
      dir = dir ? `${dir}/${segment}` : segment;
      let folder = folders.get(dir);
      if (!folder) {
        folder = { kind: 'folder', id: dir, name: segment, path: dir, count: 0, children: [] };
        folders.set(dir, folder);
        parent.children.push(folder);
      }
      parent = folder;
    }
    parent.children.push(noteNode(note));
  }

  settle(root);
  unfiled.sort(compareNodes);
  return [...root.children, ...unfiled];
}

/** Every folder id in the tree — what "expand all" needs. */
export function allFolderIds(nodes: NoteTreeNode[]): string[] {
  const out: string[] = [];
  const walk = (node: NoteTreeNode) => {
    if (node.kind !== 'folder') return;
    out.push(node.id);
    for (const child of node.children) walk(child);
  };
  for (const node of nodes) walk(node);
  return out;
}

/** The folder ids on the way to a note — what "reveal this note" must expand. */
export function ancestorFolderIds(path: string): string[] {
  const segments = path.split('/');
  segments.pop();
  const out: string[] = [];
  let dir = '';
  for (const segment of segments) {
    dir = dir ? `${dir}/${segment}` : segment;
    out.push(dir);
  }
  return out;
}
