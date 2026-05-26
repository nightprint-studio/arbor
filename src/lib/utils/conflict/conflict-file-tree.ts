// ── Conflict file tree ──────────────────────────────────────────────────────
//
// Builds a nested directory tree from a flat list of conflicted file paths
// and flattens it back to a render-friendly row list driven by an
// `expanded: Set<string>` (caller-owned). Single-child directory chains are
// collapsed into one row (`a/b/c/Main.java` → one row for `a/b/c`) so deep
// hierarchies stay compact. Leaves (files) are never collapsed.
//
// Used by both merge-mode (`conflictedFiles`) and stash-blocking mode
// (`blockingFiles`) — they pass different inputs but want the same flat/tree
// switch in the sidebar.
//
// The single-child chain collapsing is delegated to the shared helper in
// `$lib/utils/file-tree/compact-middle-dirs` so all file-style trees in the
// app collapse identically. Conflict trees always compact (independent of
// the global `appearance.compact_file_tree_dirs` toggle) — conflict lists
// are typically short and the compactness is unambiguously helpful.

import { compactMiddleDirs, type CompactAccessors } from '../file-tree/compact-middle-dirs';

export type ConflictTreeNode = {
  name: string;
  fullPath: string;
  children: Map<string, ConflictTreeNode>;
  sortedChildren: ConflictTreeNode[];
  /** Set when this leaf represents an actual conflicted file. */
  filePath?: string;
};

export type ConflictTreeRow =
  | { kind: 'dir';  depth: number; name: string; fullPath: string; hasChildren: boolean }
  | { kind: 'file'; depth: number; name: string; path: string };

const ACCESSORS: CompactAccessors<ConflictTreeNode> = {
  isDir:       (n) => !n.filePath,
  getName:     (n) => n.name,
  setName:     (n, name) => { n.name = name; },
  getChildren: (n) => n.sortedChildren,
  setChildren: (n, kids) => { n.sortedChildren = kids; },
};

function sortNodes(a: ConflictTreeNode, b: ConflictTreeNode): number {
  const aIsDir = !a.filePath;
  const bIsDir = !b.filePath;
  if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
  return a.name.localeCompare(b.name);
}

export function buildConflictTree(files: { path: string }[]): ConflictTreeNode {
  const root: ConflictTreeNode = { name: '', fullPath: '', children: new Map(), sortedChildren: [] };
  for (const f of files) {
    const parts = f.path.split('/');
    let node = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      if (!node.children.has(part)) {
        node.children.set(part, {
          name: part,
          fullPath: parts.slice(0, i + 1).join('/'),
          children: new Map(),
          sortedChildren: [],
        });
      }
      node = node.children.get(part)!;
    }
    node.filePath = f.path;
  }

  // Seed sortedChildren from the Maps, then compact + re-sort. Building once
  // (seed → compact → sort) keeps the per-render Tree iteration cheap.
  const seed = (n: ConflictTreeNode) => {
    n.sortedChildren = [...n.children.values()];
    for (const c of n.sortedChildren) seed(c);
  };
  seed(root);

  root.sortedChildren = compactMiddleDirs(root.sortedChildren, ACCESSORS);

  const reSort = (n: ConflictTreeNode) => {
    n.sortedChildren.sort(sortNodes);
    for (const c of n.sortedChildren) reSort(c);
  };
  reSort(root);

  return root;
}

/** Flatten the tree to a list of dir / file rows, honoring per-directory
 *  expansion state. `depth` drives the left indent in the template. */
export function flattenConflictTree(root: ConflictTreeNode, expanded: Set<string>): ConflictTreeRow[] {
  const rows: ConflictTreeRow[] = [];
  const visit = (nodes: ConflictTreeNode[], depth: number) => {
    for (const n of nodes) {
      if (n.filePath) {
        rows.push({ kind: 'file', depth, name: n.name, path: n.filePath });
      } else {
        rows.push({
          kind: 'dir', depth,
          name: n.name, fullPath: n.fullPath,
          hasChildren: n.sortedChildren.length > 0,
        });
        if (expanded.has(n.fullPath)) visit(n.sortedChildren, depth + 1);
      }
    }
  };
  visit(root.sortedChildren, 0);
  return rows;
}

/** Returns a new Set with `path` toggled. The caller assigns it back to its
 *  `$state` Set so Svelte sees the change. */
export function toggleTreeDir(expanded: Set<string>, path: string): Set<string> {
  const s = new Set(expanded);
  if (s.has(path)) s.delete(path); else s.add(path);
  return s;
}
