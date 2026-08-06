/**
 * A parsed syntax tree, as `arbor-syntax` produces it.
 *
 * Shared rather than per-product because the crate that builds these knows no language: Picus
 * hands it a SQL grammar and Bennu a Java one, and what comes back is the same shape. One type
 * here is what lets one panel draw both.
 *
 * Byte ranges throughout, never character offsets — this side counts UTF-16 code units and the
 * backend counts UTF-8 bytes, so a range that crossed the seam as "characters" would be a bug
 * waiting for the first accented identifier.
 */

/** One node. */
export interface SyntaxNode {
  /** Tree-sitter's kind. For an anonymous node this **is** its text (`","`). */
  kind: string;
  /** The field it fills in its parent (`name`, `body`) — the column that turns "an identifier"
   *  into "the table being written to". */
  field?: string;
  named: boolean;
  error?: boolean;
  missing?: boolean;
  range: { start: number; end: number };
  /** 1-based. */
  line: number;
  text?: string;
  children?: SyntaxNode[];
  /** Has children that were not walked — the depth or node budget ran out. */
  elided?: boolean;
  /** Its children come from a second parse of its own text — a `$$ … $$` routine body, which
   *  the SQL grammar hands back as one token. */
  injected?: boolean;
  /** **Nothing in the file says this.** Only ever set on a *derived* tree, where part of the
   *  model is written by the language rather than by the author — a Java record's accessors, a
   *  Lombok getter. Its `range` points at whatever declares it, so selecting it still lands
   *  somewhere true; the flag is what stops the panel claiming those bytes are the member. */
  synthesized?: boolean;
}

export interface SyntaxTree {
  root: SyntaxNode;
  nodeCount: number;
  /** The walk stopped early. A panel says so rather than implying the file ends. */
  truncated: boolean;
  hasErrors: boolean;
}

/** How much of the tree to walk, and how much of the source to carry back. */
export interface TreeRequest {
  maxDepth?: number;
  maxNodes?: number;
  /** Hide the commas and the keywords. */
  namedOnly?: boolean;
}

/** A node's identity in a panel: its range plus its kind, unique within one tree and stable
 *  across a re-parse that did not touch it — so expansion survives typing. */
export function nodeKey(node: SyntaxNode): string {
  return `${node.range.start}:${node.range.end}:${node.kind}`;
}
