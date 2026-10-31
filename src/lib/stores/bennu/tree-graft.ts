/**
 * The project tree is fetched a folder at a time: the first answer lists the top level (and follows a chain of
 * single-folder directories to its end), and a folder's children arrive when it is opened. These are the pure
 * operations on a tree that is only partly known — grafting a fetched folder in, and asking what is still missing.
 *
 * Paths are compared with forward slashes and no trailing one, the way the store canonicalises them.
 */
import type { TreeNode } from '$lib/types/bennu';

const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '');

/** The node at `path`, or null. */
export function findNode(tree: TreeNode | null | undefined, path: string): TreeNode | null {
  if (!tree) return null;
  const want = norm(path);
  const walk = (n: TreeNode): TreeNode | null => {
    const here = norm(n.path);
    if (here === want) return n;
    if (!n.is_dir || !want.startsWith(here + '/')) return null;
    for (const c of n.children) {
      const hit = walk(c);
      if (hit) return hit;
    }
    return null;
  };
  return walk(tree);
}

/**
 * A copy of `tree` in which the directory at `dirPath` has the children of `fresh` (the answer for that folder) and
 * is no longer `unloaded`. Only the nodes on the way down are copied; the rest keep their identity, so what draws
 * from them does not redraw. The same tree when `dirPath` is not in it.
 */
export function graftChildren(tree: TreeNode, dirPath: string, fresh: TreeNode): TreeNode {
  const want = norm(dirPath);
  const walk = (n: TreeNode): TreeNode => {
    const here = norm(n.path);
    if (here === want) return { ...n, children: fresh.children, unloaded: false };
    if (!n.is_dir || !want.startsWith(here + '/')) return n;
    let changed = false;
    const kids = n.children.map((c) => {
      const next = walk(c);
      if (next !== c) changed = true;
      return next;
    });
    return changed ? { ...n, children: kids } : n;
  };
  return walk(tree);
}

/** Whether any directory in the tree still has children that were not listed. */
export function hasUnloaded(tree: TreeNode | null | undefined): boolean {
  if (!tree) return false;
  const walk = (n: TreeNode): boolean => !!n.unloaded || n.children.some(walk);
  return walk(tree);
}

/**
 * The first directory on the way from the root to `path` (the path itself included, when it is a directory) that has
 * to be fetched before the next step down can be drawn; null when the whole way is listed or `path` is not in the tree.
 * Fetching it and asking again walks a path down one folder at a time.
 */
export function nextUnloadedOnTheWay(tree: TreeNode | null | undefined, path: string): string | null {
  if (!tree) return null;
  const want = norm(path);
  let node: TreeNode | undefined = tree;
  while (node) {
    if (node.unloaded) return node.path;
    const here = norm(node.path);
    if (here === want) return null;
    node = node.is_dir && want.startsWith(here + '/') ? node.children.find((c) => {
      const p = norm(c.path);
      return p === want || want.startsWith(p + '/');
    }) : undefined;
  }
  return null;
}

/** The directories among `open` that are in the tree but not listed yet, shallowest first. */
export function openAndUnlisted(tree: TreeNode | null | undefined, open: Iterable<string>): string[] {
  const out: string[] = [];
  for (const p of open) {
    const node = findNode(tree, p);
    if (node?.is_dir && node.unloaded) out.push(node.path);
  }
  return out.sort((a, b) => a.split('/').length - b.split('/').length);
}
