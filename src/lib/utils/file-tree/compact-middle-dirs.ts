// ── Compact middle directories (IntelliJ-style) ─────────────────────────────
//
// Collapses single-child directory chains so that
//
//     a/
//       b/
//         c/
//           foo.md
//           bar.toml
//
// renders as
//
//     a/b/c/
//       foo.md
//       bar.toml
//
// The top-level entry in the user's tree shows the joined path `a/b/c`, and
// expanding it reveals the inner-most directory's children directly. Files
// are never collapsed; chains stop at any directory that has more than one
// child or that contains a file.
//
// The helper is shape-agnostic: each consumer plugs in accessors for its own
// node type. Mutation is in-place; the caller's existing build pipeline keeps
// driving the data, this just rewrites names + children references.

export interface CompactAccessors<N> {
  /** Whether the node is a directory (i.e. is a candidate for collapsing
   *  *into* and gets recursed into). Leaves (files) short-circuit. */
  isDir:        (n: N) => boolean;
  getName:      (n: N) => string;
  setName:      (n: N, name: string) => void;
  /** Returns the ordered children list. The returned array reference may be
   *  mutated by the caller before being passed back to setChildren — but the
   *  helper itself only reads it and replaces it wholesale. */
  getChildren:  (n: N) => N[];
  setChildren:  (n: N, kids: N[]) => void;
}

/** Collapse single-child directory chains under each of `roots`. Returns the
 *  new top-level list (a chain head is replaced by its deepest non-collapsible
 *  descendant, renamed with the joined path).
 *
 *  Order is preserved — if you want children re-sorted by their joined names,
 *  sort the returned arrays yourself afterwards. */
export function compactMiddleDirs<N>(roots: N[], acc: CompactAccessors<N>): N[] {
  return roots.map((n) => collapse(n, acc));
}

function collapse<N>(node: N, acc: CompactAccessors<N>): N {
  if (!acc.isDir(node)) return node;

  // Walk the single-child directory chain. `cur` ends pointing at the deepest
  // dir whose direct children are *not* "exactly one sub-directory".
  let cur  = node;
  let kids = acc.getChildren(cur);
  while (kids.length === 1 && acc.isDir(kids[0])) {
    const next = kids[0];
    acc.setName(next, acc.getName(cur) + '/' + acc.getName(next));
    cur  = next;
    kids = acc.getChildren(cur);
  }

  // Recurse into the (possibly replaced) node's children and write them back.
  const newKids = kids.map((c) => collapse(c, acc));
  acc.setChildren(cur, newKids);
  return cur;
}
