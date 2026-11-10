/**
 * The data side of `ProjectTree`: directory listings fetched lazily from the generic filesystem
 * commands and folded into the node shape the shared `Tree` draws. No product knowledge — a
 * product narrows what is shown through `entryFilter`.
 */

import { fsReadDir, type FsEntry } from '$lib/ipc/fs';

/** What a listing returns: a filesystem entry plus what only a product's own source can know. */
export interface ProjectEntry extends FsEntry {
  /** Present in the project but ignored by git — drawn faintest. */
  ignored?: boolean;
  /** A dot-entry or otherwise hidden — drawn faint. */
  hidden?: boolean;
  /** Product payload carried onto the node (a root tag, a build unit, …). */
  data?: unknown;
}

export interface ProjectNode {
  path: string;
  name: string;
  /** Shown instead of `name` (a collapsed package chain `it.acme.portal`); the path still identifies the row. */
  label?: string;
  is_dir: boolean;
  ignored?: boolean;
  hidden?: boolean;
  data?: unknown;
  /** Listed children (empty until the folder is expanded and fetched). */
  children: ProjectNode[];
  /** A folder whose listing has not been fetched yet — it shows a chevron all the same. */
  unloaded: boolean;
}

export type Listing = Record<string, ProjectEntry[]>;

/** Lets a product reshape one folder's children (collapse package chains, hide a marker file…). */
export type TransformChildren = (nodes: ProjectNode[], dir: string) => ProjectNode[];

/** Folders first, then names, case-insensitive and numeric-aware (`take2` before `take10`). */
export function sortEntries<T extends FsEntry>(entries: T[]): T[] {
  return [...entries].sort((a, b) =>
    a.is_dir === b.is_dir
      ? a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true })
      : a.is_dir ? -1 : 1);
}

/** List one folder, filtered and sorted. A folder that cannot be read lists as empty. */
export async function listDir(path: string, entryFilter?: (e: FsEntry) => boolean): Promise<ProjectEntry[]> {
  try {
    const all = await fsReadDir(path, false);
    return sortEntries(entryFilter ? all.filter(entryFilter) : all);
  } catch {
    return [];
  }
}

/** Fold the fetched listings into a tree rooted at `root` (its children, not the root itself). */
export function buildNodes(root: string, listing: Listing, transform?: TransformChildren): ProjectNode[] {
  const build = (dir: string): ProjectNode[] => {
    const nodes = (listing[dir] ?? []).map((e): ProjectNode => {
      const known = e.is_dir && listing[e.path] !== undefined;
      return {
        path: e.path,
        name: e.name,
        is_dir: e.is_dir,
        ignored: e.ignored,
        hidden: e.hidden,
        data: e.data,
        children: known ? build(e.path) : [],
        unloaded: e.is_dir && !known,
      };
    });
    return transform ? transform(nodes, dir) : nodes;
  };
  return build(root);
}

/** Last path segment's parent. */
export function parentDir(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i <= 0 ? path : path.slice(0, i);
}

/** `dir` + `name` with the separator `dir` already uses. */
export function joinPath(dir: string, name: string): string {
  const sep = dir.includes('\\') && !dir.includes('/') ? '\\' : '/';
  return dir.endsWith('/') || dir.endsWith('\\') ? dir + name : dir + sep + name;
}

/** Ancestor folders of `path` strictly below `root`, outermost first — what to open to reveal it. */
export function ancestorsBelow(root: string, path: string): string[] {
  const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '');
  const r = norm(root);
  const p = norm(path);
  if (!p.startsWith(r + '/')) return [];
  const parts = p.slice(r.length + 1).split('/').slice(0, -1);
  const out: string[] = [];
  let acc = root;
  for (const part of parts) { acc = joinPath(acc, part); out.push(acc); }
  return out;
}
