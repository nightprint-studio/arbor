/**
 * Source roots render as **packages**, not as folder chains.
 *
 * `it/comune/gestionale_atti` is one name written three times as directories, and a
 * tree that shows it as three rows spends three levels of indentation saying nothing
 * — every Java project starts with two or three of these before the first thing you
 * actually want to click. IntelliJ collapses them into `it.comune.gestionale_atti`,
 * and so does this.
 *
 * The collapsing itself is the shared {@link compactMiddleDirs} — the same helper
 * three Corvus panels use for their file trees. The only thing that is Java's is
 * **where** it applies and **what joins the segments**: under a source root, with a
 * dot, because that is how the thing being named is written. Everywhere else the
 * project tree stays a plain folder tree, because everywhere else it is one.
 */

import { compactMiddleDirs } from '$lib/utils/file-tree/compact-middle-dirs';
import type { TreeNode } from '$lib/types/bennu';

/**
 * Directories whose contents are packages. Matched by path suffix, so every module of
 * a multi-module reactor gets its own — `web/src/main/java` and `core/src/main/java`
 * are both source roots, and nothing has to enumerate the modules.
 *
 * Resource roots are in here too: a `messages` bundle sits at
 * `src/main/resources/it/acme/messages`, which is the same name written the same way,
 * and IntelliJ shows it the same way.
 *
 * Deliberately NOT `src/main/webapp`: a web app's directories are paths — they are
 * what a URL is made of — and `WEB-INF.jsp.admin` would be a folder chain wearing a
 * package's clothes.
 */
const SOURCE_ROOT_SUFFIXES = [
  'src/main/java',
  'src/test/java',
  'src/main/resources',
  'src/test/resources',
  // Generated sources, which a legacy build puts under target/ and which are read far
  // more often than they are written.
  'target/generated-sources/annotations',
];

/** Whether `path` is a directory whose children are packages. */
export function isSourceRoot(path: string): boolean {
  const fwd = path.replace(/\\/g, '/').replace(/\/+$/, '');
  return SOURCE_ROOT_SUFFIXES.some((suffix) => fwd === suffix || fwd.endsWith('/' + suffix));
}

/**
 * Whether `path` names a directory **inside** a source root — a package rather than a
 * folder. The source root itself (`java`, `resources`) is not one: it is the container
 * the packages live in, which is why it keeps a folder's icon.
 */
export function isInPackageRoot(path: string): boolean {
  const fwd = path.replace(/\\/g, '/');
  return SOURCE_ROOT_SUFFIXES.some((suffix) => fwd.includes('/' + suffix + '/'));
}

/**
 * A copy of `node` and everything under it.
 *
 * Load-bearing, not defensive: {@link compactMiddleDirs} rewrites names and children
 * **in place**, and the nodes here belong to the store. Compacting them directly would
 * edit the source of truth, so the second time the derived value recomputed it would
 * compact the already-compacted names and produce `it.acme.it.acme.portal`.
 */
function clone(node: TreeNode): TreeNode {
  return { ...node, children: node.is_dir ? node.children.map(clone) : [] };
}

const DOT_ACCESSORS = {
  isDir: (n: TreeNode) => n.is_dir,
  getName: (n: TreeNode) => n.name,
  setName: (n: TreeNode, name: string) => { n.name = name; },
  getChildren: (n: TreeNode) => n.children,
  setChildren: (n: TreeNode, kids: TreeNode[]) => { n.children = kids; },
  separator: '.',
};

/**
 * The project tree with every source root's packages collapsed.
 *
 * Node **paths are untouched** — a collapsed row keeps the path of the deepest
 * directory it stands for, which is what the tree keys expansion and selection by, and
 * what opening a file resolves against. Only the label changes, which is the whole
 * point: the row means exactly what it meant before, it just says so in one line.
 *
 * Returns the input array itself when there is nothing to collapse, so a project with
 * no Java source root costs one walk and no allocation.
 */
export function packageTree(nodes: TreeNode[]): TreeNode[] {
  if (!nodes.some(containsSourceRoot)) return nodes;
  return nodes.map(compactUnderSourceRoots);
}

/** Whether this subtree holds a source root — the test that keeps the copy off every
 *  branch that has no packages in it. */
function containsSourceRoot(node: TreeNode): boolean {
  if (!node.is_dir) return false;
  return isSourceRoot(node.path) || node.children.some(containsSourceRoot);
}

function compactUnderSourceRoots(node: TreeNode): TreeNode {
  if (!node.is_dir) return node;

  if (isSourceRoot(node.path)) {
    // The root itself keeps its name (`java`); its children are the packages.
    const copy = clone(node);
    copy.children = compactMiddleDirs(copy.children, DOT_ACCESSORS);
    return copy;
  }

  if (!containsSourceRoot(node)) return node;
  return { ...node, children: node.children.map(compactUnderSourceRoots) };
}
