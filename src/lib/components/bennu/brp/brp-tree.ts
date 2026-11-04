/**
 * The pieces of the entity tree that need no store: nesting a filtered result under its parents, and
 * picking what the keyboard should do with a typed character.
 */

import type { BrpRow } from '$lib/ipc/bennu/brp';

export interface FilterNode {
  row: BrpRow;
  /** Matched the search; `false` for an ancestor shown only to give a hit its place. */
  hit: boolean;
  children: FilterNode[];
}

/**
 * Nest `rows` by their `parent` field. A row whose parent is not among the rows is a root, so a filter
 * that matches only deep entities still shows a tree. `context` lists the ids that are there as
 * ancestors only. Siblings come out in id order, as the unfiltered tree lists them.
 */
export function buildFilterTree(rows: readonly BrpRow[], context: readonly number[] = []): FilterNode[] {
  const ctx = new Set(context);
  const nodes = new Map<number, FilterNode>(rows.map((row) => [row.id, { row, hit: !ctx.has(row.id), children: [] }]));
  const roots: FilterNode[] = [];
  for (const node of nodes.values()) {
    const parent = node.row.parent !== null ? nodes.get(node.row.parent) : undefined;
    (parent ?? { children: roots }).children.push(node);
  }
  const order = (list: FilterNode[]) => {
    list.sort((a, b) => a.row.id - b.row.id);
    list.forEach((n) => order(n.children));
  };
  order(roots);
  return roots;
}

/** Every id in the filtered tree that has children — what "expanded" means when a filter opens it. */
export function branchIds(nodes: readonly FilterNode[]): number[] {
  return nodes.flatMap((n) => (n.children.length > 0 ? [n.row.id, ...branchIds(n.children)] : []));
}

/** A key that should start (or continue) a filter when the tree has the focus: one printable character. */
export function isFilterKey(e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey'>): boolean {
  return e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && e.key !== ' ';
}
