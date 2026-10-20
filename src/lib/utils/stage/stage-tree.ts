/**
 * The folder tree behind the staging panel's "tree" view: built from the flat list of changed
 * files, optionally compacted (`a/b/c` on one row), and the expand-state bookkeeping that keeps
 * a folder the user collapsed collapsed across refreshes.
 *
 * Pure — no stores, no DOM — so both Corvus and Bennu feed it the same entries and the rules
 * (folders first, then by name; what a folder's "files" are) live in one place.
 */
import { compactMiddleDirs } from '$lib/utils/file-tree/compact-middle-dirs';
import { pathsOf } from '$lib/ipc/corvus/stage';
import type { StatusEntry } from '$lib/types/corvus/git';

export interface StageTreeNode {
  name: string;
  fullPath: string;
  children: Map<string, StageTreeNode>;
  /** Sorted snapshot of `children.values()` — baked at build time so the template never
   *  allocates or sorts per render. Compact mode rewrites this, never `children`. */
  sortedChildren: StageTreeNode[];
  entry?: StatusEntry;
}

/** What a stage/unstage/discard acts on: the git paths to send, plus how to name the target in
 *  a toast. A file and a folder differ only in how these two are built. */
export interface StageTarget {
  paths: string[];
  label: string;
}

const isDir = (n: StageTreeNode) => n.children.size > 0;

export function sortNodes(a: StageTreeNode, b: StageTreeNode): number {
  const aD = isDir(a), bD = isDir(b);
  if (aD !== bD) return aD ? -1 : 1;
  return a.name.localeCompare(b.name);
}

const ACCESSORS = {
  isDir,
  getName:     (n: StageTreeNode) => n.name,
  setName:     (n: StageTreeNode, name: string) => { n.name = name; },
  getChildren: (n: StageTreeNode) => n.sortedChildren,
  setChildren: (n: StageTreeNode, kids: StageTreeNode[]) => { n.sortedChildren = kids; },
};

function bakeSort(n: StageTreeNode) {
  n.sortedChildren = [...n.children.values()].sort(sortNodes);
  for (const c of n.sortedChildren) bakeSort(c);
}

/** Build the tree for `entries`. With `compact`, single-child folder chains collapse into one
 *  row (IntelliJ "compact middle packages"). */
export function buildStageTree(entries: StatusEntry[], compact = false): StageTreeNode {
  const root: StageTreeNode = { name: '', fullPath: '', children: new Map(), sortedChildren: [] };
  for (const entry of entries) {
    const parts = entry.path.split('/');
    let node = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      if (!node.children.has(part)) {
        const fullPath = parts.slice(0, i + 1).join('/');
        node.children.set(part, { name: part, fullPath, children: new Map(), sortedChildren: [] });
      }
      node = node.children.get(part)!;
    }
    node.entry = entry;
  }
  bakeSort(root);
  if (compact) {
    root.sortedChildren = compactMiddleDirs(root.sortedChildren, ACCESSORS);
    // Joining names changes the alphabetical order the user sees, so sort again.
    const reSort = (n: StageTreeNode) => {
      n.sortedChildren.sort(sortNodes);
      n.sortedChildren.forEach(reSort);
    };
    reSort(root);
  }
  return root;
}

/** The entries under a folder, depth-first. Walks `children` — the map the tree was built
 *  from — because compact mode rewrites `sortedChildren` only. */
export function leafEntries(node: StageTreeNode): StatusEntry[] {
  const out: StatusEntry[] = [];
  const walk = (n: StageTreeNode) => {
    if (n.entry) out.push(n.entry);
    n.children.forEach(walk);
  };
  node.children.forEach(walk);
  return out;
}

export const basename = (p: string) => p.split('/').pop() ?? p;

/** A moved file is two paths and the tooltip says so — same `old → new` shape the diff chrome
 *  uses, so the same file reads the same everywhere. */
export const pathLabel = (e: StatusEntry) => (e.old_path ? `${e.old_path} → ${e.path}` : e.path);

export const fileTarget = (entry: StatusEntry): StageTarget =>
  ({ paths: pathsOf(entry), label: basename(entry.path) });

/** A rename contributes two paths but is still one file, so the count the user sees comes
 *  from the entries — never from `paths.length`. */
export function folderTarget(node: StageTreeNode): StageTarget & { count: number } {
  const entries = leafEntries(node);
  return {
    paths: entries.flatMap(pathsOf),
    label: `${entries.length} file${entries.length !== 1 ? 's' : ''} in ${node.name}`,
    count: entries.length,
  };
}

/** Every folder that appears as a collapsible row in the *rendered* tree. */
export function collectDirs(node: StageTreeNode, out = new Set<string>()): Set<string> {
  if (node.sortedChildren.length > 0 && node.fullPath) out.add(node.fullPath);
  node.sortedChildren.forEach((c) => collectDirs(c, out));
  return out;
}

/** Expand state after the tree changed: a folder never seen before opens, one the user already
 *  had a say about keeps its state, one that vanished is forgotten. */
export function reconcileExpanded(
  currentDirs: Set<string>,
  prevExpanded: Set<string>,
  knownDirs: Set<string>,
): Set<string> {
  const next = new Set<string>();
  for (const d of currentDirs) {
    if (!knownDirs.has(d) || prevExpanded.has(d)) next.add(d);
  }
  return next;
}
