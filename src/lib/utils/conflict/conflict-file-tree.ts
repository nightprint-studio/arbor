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
  const collapse = (n: ConflictTreeNode): ConflictTreeNode => {
    if (n.filePath) return n;
    if (n.children.size === 1 && !n.filePath) {
      const only = [...n.children.values()][0];
      if (!only.filePath) {
        const collapsed = collapse(only);
        return {
          ...collapsed,
          name: (n.name ? n.name + '/' : '') + collapsed.name,
        };
      }
    }
    return n;
  };
  const bakeSort = (n: ConflictTreeNode) => {
    const kids = [...n.children.values()].map(collapse);
    kids.sort((a, b) => {
      const aIsDir = !a.filePath;
      const bIsDir = !b.filePath;
      if (aIsDir !== bIsDir) return aIsDir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    n.sortedChildren = kids;
    for (const c of kids) bakeSort(c);
  };
  bakeSort(root);
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
