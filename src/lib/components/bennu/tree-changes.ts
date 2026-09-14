/**
 * What a `tree-changed` event asks of the Project tree — the pure half of the tree sync
 * (`stores/bennu/tree-sync.svelte.ts`).
 *
 * Three questions, each a function of its arguments so it can be tested without a window:
 * does the tree need reloading, is there something the user should be told, and does what is on
 * screen still match a fresh listing of the disk.
 */

import type { TreeChange, TreeChanged } from '$lib/ipc/bennu/tree-watch';
import type { TreeNode } from '$lib/types/bennu';

/** The files a project model is parsed from. */
const MANIFESTS = ['pom.xml', 'Cargo.toml'];

/** Items named in a notice before it says "and N more". */
const LISTED = 3;

function baseName(path: string): string {
  return path.split('/').pop() ?? path;
}

function isTopLevel(path: string): boolean {
  return !path.includes('/');
}

/** Forward slashes, no trailing slash — the form every tree path and expansion id is keyed by. */
function canonRoot(root: string): string {
  return root.replace(/\\/g, '/').replace(/\/+$/, '');
}

/** Whether a burst touched a build manifest — the file the project's own name, modules and JDK
 *  come out of.
 *
 *  A truncated or rescanned burst counts: the paths are capped or incomplete, so "not in the list"
 *  is not "did not change", and re-reading one manifest is cheaper than being wrong about it. */
export function manifestChanged(payload: TreeChanged): boolean {
  if (payload.truncated || payload.rescan) return true;
  return payload.paths.some((p) => MANIFESTS.includes(baseName(p)));
}

/** Whether the tree has to be re-listed: the listing changed, events may have been lost, or a
 *  `.gitignore` moved which rows are dimmed. A manifest edit alone does not — it changes the
 *  model, not the rows. A payload without `structural` is from a backend that did not classify,
 *  and is treated as structural. */
export function needsTreeReload(payload: TreeChanged): boolean {
  return payload.structural !== false
    || payload.rescan === true
    || payload.truncated
    || payload.paths.some((p) => baseName(p) === '.gitignore');
}

/** One entry of a structure notice. */
export interface RenamedEntry { from: string; to: string }

/** What is worth telling the user about a burst. */
export interface StructureNotice {
  added: string[];
  removed: string[];
  renamed: RenamedEntry[];
  /** A module moved, appeared, or a `pom.xml` changed alongside: the index is likely stale. */
  rebuildIndex: boolean;
}

export function isEmptyNotice(n: StructureNotice): boolean {
  return n.added.length === 0 && n.removed.length === 0 && n.renamed.length === 0;
}

/**
 * The notice for a burst, or `null` when there is nothing the user would care to hear.
 *
 * Only directories, and only the ones that change the shape of the project: a folder added or
 * removed directly under the root, a top-level folder renamed, a module (a directory holding a
 * `pom.xml`) added or renamed anywhere. Files never notify — every save is a structural event to
 * an IDE that writes through a temporary file.
 */
export function structureNotice(payload: TreeChanged): StructureNotice | null {
  const notice: StructureNotice = { added: [], removed: [], renamed: [], rebuildIndex: false };
  for (const c of payload.changes ?? []) {
    if (!c.is_dir) continue;
    if (c.kind === 'added') {
      if (isTopLevel(c.path) || c.module) notice.added.push(c.path);
      if (c.module) notice.rebuildIndex = true;
    } else if (c.kind === 'removed') {
      if (isTopLevel(c.path)) notice.removed.push(c.path);
    } else {
      if (isTopLevel(c.path) || isTopLevel(c.from) || c.module) notice.renamed.push({ from: c.from, to: c.path });
      if (c.module) notice.rebuildIndex = true;
    }
  }
  if (isEmptyNotice(notice)) return null;
  if (payload.paths.some((p) => baseName(p) === 'pom.xml')) notice.rebuildIndex = true;
  return notice;
}

/** Two notices for the same root as one: a bulk operation that spans several bursts is one
 *  notification. An entry added then removed (or the reverse) cancels, and renames chain. */
export function mergeNotices(a: StructureNotice, b: StructureNotice): StructureNotice {
  const union = (x: string[], y: string[]) => [...new Set([...x, ...y])];
  const added = union(a.added.filter((n) => !b.removed.includes(n)), b.added.filter((n) => !a.removed.includes(n)));
  const removed = union(a.removed.filter((n) => !b.added.includes(n)), b.removed.filter((n) => !a.added.includes(n)));
  const renamed = [...a.renamed];
  for (const r of b.renamed) {
    const i = renamed.findIndex((x) => x.to === r.from);
    if (i >= 0) {
      const origin = renamed[i].from;
      renamed.splice(i, 1);
      if (origin !== r.to) renamed.push({ from: origin, to: r.to });
    } else if (!renamed.some((x) => x.from === r.from && x.to === r.to)) {
      renamed.push(r);
    }
  }
  return { added, removed, renamed, rebuildIndex: a.rebuildIndex || b.rebuildIndex };
}

function listed(items: string[]): string {
  const shown = items.slice(0, LISTED).join(', ');
  return items.length > LISTED ? `${shown} and ${items.length - LISTED} more` : shown;
}

/** The notice as one line. `project` names the project when the window has more than one. */
export function describeNotice(n: StructureNotice, project?: string): string {
  const quote = (s: string) => `'${s}'`;
  const parts: string[] = [];
  if (n.added.length) parts.push(`added ${listed(n.added.map(quote))}`);
  if (n.removed.length) parts.push(`removed ${listed(n.removed.map(quote))}`);
  if (n.renamed.length) parts.push(`renamed ${listed(n.renamed.map((r) => `${quote(r.from)} → ${quote(r.to)}`))}`);
  const where = project ? ` in ${project}` : '';
  return `Project structure changed${where}: ${parts.join(', ')}`;
}

/**
 * The expansion ids a burst of directory renames moved, as `[old, new]` pairs.
 *
 * Expansion is keyed by path, so without this a renamed folder that was open reloads closed — and
 * with it every open folder inside it.
 */
export function renamedExpansion(ids: Iterable<string>, root: string, changes: TreeChange[]): Array<[string, string]> {
  const base = canonRoot(root);
  const moves = changes
    .filter((c): c is Extract<TreeChange, { kind: 'renamed' }> => c.kind === 'renamed' && c.is_dir)
    .map((c) => [`${base}/${c.from}`, `${base}/${c.path}`] as const);
  if (moves.length === 0) return [];
  const out: Array<[string, string]> = [];
  for (const id of ids) {
    const move = moves.find(([from]) => id === from || id.startsWith(`${from}/`));
    if (move) out.push([id, move[1] + id.slice(move[0].length)]);
  }
  return out;
}

/** The directories a reconciliation re-lists: the root, whose rows are always on screen, then the
 *  expanded folders of this project, at most `max` in all. */
export function reconcileTargets(root: string, expanded: Iterable<string>, max: number): string[] {
  const base = canonRoot(root);
  const out = new Set<string>([base]);
  for (const id of expanded) {
    if (out.size >= max) break;
    if (id.startsWith(`${base}/`)) out.add(id);
  }
  return [...out];
}

/** The node at `path` in a canonical tree. */
export function findNode(node: TreeNode | null | undefined, path: string): TreeNode | undefined {
  if (!node) return undefined;
  if (node.path === path) return node;
  if (!node.is_dir || !path.startsWith(node.path.endsWith('/') ? node.path : `${node.path}/`)) return undefined;
  for (const child of node.children) {
    const hit = findNode(child, path);
    if (hit) return hit;
  }
  return undefined;
}

/** Whether a fresh one-level listing disagrees with the children on screen. Names and kinds only —
 *  what is inside a child is the next listing's question. A directory the tree does not have at
 *  all disagrees. */
export function listingDiffers(current: TreeNode | undefined, fresh: TreeNode): boolean {
  if (!current) return true;
  const key = (n: TreeNode) => `${n.is_dir ? 'd' : 'f'}:${n.name}`;
  const a = current.children.map(key).sort();
  const b = fresh.children.map(key).sort();
  return a.length !== b.length || a.some((k, i) => k !== b[i]);
}
