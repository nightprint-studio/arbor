// Questions the resolved tree answers, derived once from `DependencyTree`.
//
// The tree is a list of nodes per module; what the Dependencies panel wants is per *artifact*:
// "who brought this in" and "which versions does the project see". Both are the same walk — an
// artifact's occurrences, each with the chain of ancestors that led to it — so this builds one
// index and the panel only looks things up.

import type { DependencyTree, ModuleTree, TreeNode } from '$lib/ipc/bennu/deps';

/** `group:name` — what two versions of one library share. */
export function nodeCoord(n: { group: string; name: string }): string {
  return n.group ? `${n.group}:${n.name}` : n.name;
}

/** One place an artifact appears in a module's tree. */
export interface Occurrence {
  module: string;
  node: TreeNode;
  /** Ancestors from the module's direct dependency down to the immediate parent; empty when the
   *  module declares it itself. Each is a `group:name` with its version. */
  chain: { coord: string; version: string }[];
}

/** An artifact and every version of it the tree mentions. */
export interface ArtifactUsage {
  coord: string;
  /** Distinct versions, the one Maven kept in some module first. */
  versions: string[];
  occurrences: Occurrence[];
  /** Some module sees two different versions of it — the tree dropped one for the other. */
  conflicted: boolean;
}

export interface TreeIndex {
  artifacts: Map<string, ArtifactUsage>;
  /** Artifacts with more than one version in a single module, most-requested first. */
  conflicts: ArtifactUsage[];
}

function chainOf(tree: ModuleTree, node: TreeNode): Occurrence['chain'] {
  const chain: Occurrence['chain'] = [];
  let at = node.parent;
  // Bounded by the tree's own size: a malformed parent loop must not hang the panel.
  for (let guard = 0; at != null && guard < tree.nodes.length; guard++) {
    const p = tree.nodes[at];
    chain.unshift({ coord: nodeCoord(p), version: p.version });
    at = p.parent;
  }
  return chain;
}

export function buildTreeIndex(tree: DependencyTree | null): TreeIndex {
  const artifacts = new Map<string, ArtifactUsage>();
  if (!tree) return { artifacts, conflicts: [] };

  for (const mod of tree.modules) {
    const perModuleVersions = new Map<string, Set<string>>();
    for (const node of mod.nodes) {
      const coord = nodeCoord(node);
      let usage = artifacts.get(coord);
      if (!usage) {
        usage = { coord, versions: [], occurrences: [], conflicted: false };
        artifacts.set(coord, usage);
      }
      usage.occurrences.push({ module: mod.module, node, chain: chainOf(mod, node) });
      // A kept node's version leads the list: it is the one that is actually on the classpath.
      if (!usage.versions.includes(node.version)) {
        if (node.omitted) usage.versions.push(node.version);
        else usage.versions.unshift(node.version);
      }
      const seen = perModuleVersions.get(coord) ?? new Set<string>();
      seen.add(node.version);
      perModuleVersions.set(coord, seen);
    }
    for (const [coord, versions] of perModuleVersions) {
      if (versions.size > 1) artifacts.get(coord)!.conflicted = true;
    }
  }

  const conflicts = [...artifacts.values()]
    .filter((a) => a.conflicted)
    .sort((a, b) => b.occurrences.length - a.occurrences.length || a.coord.localeCompare(b.coord));
  return { artifacts, conflicts };
}

/** The direct dependencies (of any module) that transitively bring `coord` in, nearest first and
 *  deduplicated — "this is here because of X, Y". Empty when the module declares it itself. */
export function introducers(usage: ArtifactUsage | undefined): string[] {
  if (!usage) return [];
  const out: string[] = [];
  for (const o of usage.occurrences) {
    if (o.node.omitted || o.chain.length === 0) continue;
    const top = o.chain[0].coord;
    if (!out.includes(top)) out.push(top);
  }
  return out;
}

/** Whether some module declares `coord` itself. */
export function isDeclaredDirectly(usage: ArtifactUsage | undefined): boolean {
  return !!usage?.occurrences.some((o) => o.chain.length === 0 && !o.node.omitted);
}

/** The artifacts a node's own dependency pulls in — one level, for "what does this drag along". */
export function childrenOf(tree: DependencyTree | null, module: string, coord: string): TreeNode[] {
  const mod = tree?.modules.find((m) => m.module === module);
  if (!mod) return [];
  return mod.nodes.filter(
    (n) => n.parent != null && !n.omitted && nodeCoord(mod.nodes[n.parent]) === coord,
  );
}
