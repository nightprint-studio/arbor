/**
 * Call / type hierarchy IPC — one pair of calls over both engines.
 *
 * Its own module rather than a corner of `lsp.ts` because it is no longer a language-server
 * concept: a `.java` buffer is answered by Bennu's own engine over the whole-project reference
 * index, a `.rs` one by rust-analyzer, and the backend routes on the file. The panel asks "who calls
 * this" and does not have to know which engine replied.
 *
 * Same conventions as the rest of `ipc/bennu`: every call wraps its fields under `{ args: … }` and
 * every offset is a **UTF-8 byte offset**.
 */

import { bennu } from '../rpc';

/** One call site inside a hierarchy node. */
export interface HierarchyCallSite {
  file: string;
  start: number;
  end: number;
  line: number;
  preview: string;
}

/** One node of a call or type hierarchy.
 *
 *  The two share a shape because the question differs only in direction, and because the panel that
 *  draws them is one panel: a tree whose children are fetched a level at a time. */
export interface HierarchyNode {
  name: string;
  /** A lowercase kind name (`method`, `class`, `interface`, `struct`, `trait`). */
  kind: string;
  detail?: string | null;
  /** Where the declaration is — the name token, so go-to lands on it. **Empty** for an item with no
   *  source to open: a supertype that lives in a dependency jar is worth naming in the tree and
   *  cannot be jumped to. */
  file: string;
  start: number;
  end: number;
  line: number;
  col: number;
  /** The trimmed source line, for a preview. */
  preview: string;
  /** The call sites inside this node that reach the item asked about; empty for a type hierarchy. */
  call_sites: HierarchyCallSite[];
  /** The engine's own handle on this item, opaque. Sent back **verbatim** to fetch its children. */
  handle: unknown;
}

/** Which way a hierarchy is walked. `incoming`/`outgoing` are calls, `supertypes`/`subtypes` types. */
export type HierarchyDirection = 'incoming' | 'outgoing' | 'supertypes' | 'subtypes';

/** The item at `offset` a hierarchy can be built from — the root of the tree the panel draws.
 *
 *  `calls` picks which hierarchy: `true` for the call hierarchy, `false` for the type hierarchy. An
 *  empty list means the caret is not on something either can be built from.
 *  Wire: `bennu_hierarchy_prepare` — `{ file, source, offset, calls }`. */
export function prepareHierarchy(
  file: string,
  source: string,
  offset: number,
  calls: boolean,
): Promise<HierarchyNode[]> {
  return bennu('bennu_hierarchy_prepare', { args: { file, source, offset, calls } });
}

/** One level of a hierarchy, expanded from a node's handle.
 *
 *  `scope` is any path inside the project — which engine answers. Not the node's own file: a caller
 *  can live in a dependency's source, which is deliberately not a workspace of its own.
 *  `item` is the node's `handle`, passed back verbatim.
 *  Wire: `bennu_hierarchy_step` — `{ scope, item, direction }`. */
export function hierarchyStep(
  scope: string,
  item: unknown,
  direction: HierarchyDirection,
): Promise<HierarchyNode[]> {
  return bennu('bennu_hierarchy_step', { args: { scope, item, direction } });
}
