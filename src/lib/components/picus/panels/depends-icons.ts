/**
 * One icon per object kind, for the dependency panel and its trees.
 *
 * The same four icons the object tree uses (`ConnectionSchemaTree`), on purpose: a
 * table has to look like a table in both places, or the graph reads as a different
 * database from the one in the sidebar. Routines get one of their own because the
 * object tree has no group for them — they are reachable here only as the far end
 * of an edge.
 */

import { Braces, Circle, Eye, ListOrdered, Table2, Zap } from 'lucide-svelte';

/** The lucide component for a `DependencyNode.kind`. */
export function iconForKind(kind: string) {
  switch (kind) {
    case 'view':      return Eye;
    case 'sequence':  return ListOrdered;
    case 'trigger':   return Zap;
    case 'function':
    case 'procedure': return Braces;
    case 'table':     return Table2;
    // An object the read schema never listed — the far side of an edge into a
    // schema nobody asked for. It has a name and nothing else, and says so.
    default:          return Circle;
  }
}

/** The four kinds that have a tab of their own. Anything else can be walked to but
 *  not opened, and the row's action is hidden rather than shown failing. */
export function isOpenable(kind: string): kind is 'table' | 'view' | 'sequence' | 'trigger' {
  return kind === 'table' || kind === 'view' || kind === 'sequence' || kind === 'trigger';
}
