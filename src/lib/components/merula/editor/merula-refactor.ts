/**
 * merula structural refactors over the Tree-sitter CST (pure, CodeMirror-free).
 *
 * The on-demand counterpart of `merula-edit.ts` (which rewrites control *literals*):
 * these reshape *structure* — rename a symbol everywhere, lift a selected pattern
 * into a named `let`, or inline a `let` back into its uses. Each planner returns
 * CodeMirror change specs ({@link EditChange}) the editor applies as one undoable
 * transaction (the re-eval that follows re-baselines the live state).
 *
 * The `.merula` language is **flat-scoped** — a top-level name has no shadowing, so
 * "every identifier with this text" is "every reference to this symbol" (the same
 * assumption `identifierUsages` documents). That makes rename/inline a safe,
 * file-local text rewrite. The deliberately-rejected cases (a selection inside
 * mini-notation, a `let` spliced into an island, a selection that uses a lambda
 * parameter) return a human message instead of mangling the source.
 *
 * Offsets are UTF-16 (tree-sitter `startIndex`/`endIndex` = CodeMirror coords).
 */

import { identifierUsages, extractSymbols, type Tree, type Node } from './merula-lang';
import type { EditChange } from './merula-edit';

/** A legal `.merula` identifier (the binding-name grammar). */
const IDENT_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** Grammar keywords — never a valid new name. */
const RESERVED = new Set(['let', 'fn', 'import', 'from']);

/** Host-expression CST node kinds (mirrors `walk_expr` in the lang crate's
 *  `parse.rs`) — the things a selection may be extracted as / a `let` may hold. */
const EXPR_TYPES = new Set([
  'number', 'string', 'note_literal', 'identifier', 'call_expression', 'method_call',
  'unary_expression', 'binary_expression', 'range_expression', 'lambda', 'island',
  'parenthesized',
]);

/** Low-precedence expression kinds: wrap in parens when inlined into a larger
 *  expression so binding doesn't change (same set as `merula-edit`'s LOW_PREC). */
const NEEDS_PARENS = new Set(['binary_expression', 'range_expression', 'lambda', 'unary_expression']);

/** Mini-notation node kinds (inside an `island` body) — a selection landing on one
 *  isn't a host expression and can't be lifted to a top-level `let`. */
const MINI_TYPES = new Set([
  'parallel', 'sequence', 'term', 'group', 'alternation', 'polymeter',
  'rest', 'extend', 'splice', 'sound_name', 'note_name',
]);

/** A planned refactor: the changes to dispatch, plus an error (nothing applied)
 *  or a success note (for a toast). */
export interface RefactorPlan {
  changes: EditChange[];
  error?: string;
  note?: string;
}

function validateNewName(name: string, tree: Tree): string | null {
  if (!IDENT_RE.test(name)) return 'Not a valid name (letters, digits, _)';
  if (RESERVED.has(name)) return `"${name}" is a reserved word`;
  const { defs, imports } = extractSymbols(tree);
  if (defs.has(name) || imports.has(name)) return `"${name}" already exists in this file`;
  return null;
}

// ── Rename ──────────────────────────────────────────────────────────────────────

/** Rename `oldName` to `newName` at every occurrence (declaration + references,
 *  incl. `$splice` uses — they're identifier nodes too). Flat scope makes this a
 *  whole-file text swap. */
export function renamePlan(tree: Tree, oldName: string, newName: string): RefactorPlan {
  const name = newName.trim();
  if (name === oldName) return { changes: [] }; // no-op
  const bad = validateNewName(name, tree);
  if (bad) return { changes: [], error: bad };
  const uses = identifierUsages(tree, oldName);
  if (!uses.length) return { changes: [], error: `No occurrences of "${oldName}"` };
  return {
    changes: uses.map((r) => ({ from: r.from, to: r.to, insert: name })),
    note: `Renamed ${uses.length} occurrence${uses.length === 1 ? '' : 's'}`,
  };
}

// ── Tree helpers ────────────────────────────────────────────────────────────────

/** The top-level item (child of the root) that contains `node`. */
function topLevelItemOf(tree: Tree, node: Node): Node | null {
  const rootId = tree.rootNode.id;
  let cur: Node = node;
  while (cur.parent && cur.parent.id !== rootId) cur = cur.parent;
  return cur.parent && cur.parent.id === rootId ? cur : null;
}

/** Parameter names of every lambda enclosing `node` (so an extraction doesn't lift
 *  a selection that depends on a parameter out of its scope). */
function enclosingLambdaParams(node: Node): Set<string> {
  const params = new Set<string>();
  for (let cur = node.parent; cur; cur = cur.parent) {
    if (cur.type !== 'lambda') continue;
    const body = cur.childForFieldName('body');
    for (const c of cur.namedChildren) {
      if (!c || (body && c.id === body.id)) continue;
      if (c.type === 'identifier') params.add(c.text);
      else if (c.type === 'parameters') {
        for (const id of c.namedChildren) if (id?.type === 'identifier') params.add(id.text);
      }
    }
  }
  return params;
}

/** Every identifier name used anywhere under `node`. */
function subtreeIdentifiers(node: Node): Set<string> {
  const out = new Set<string>();
  const walk = (n: Node) => {
    if (n.type === 'identifier') out.add(n.text);
    for (let i = 0; i < n.childCount; i++) { const c = n.child(i); if (c) walk(c); }
  };
  walk(node);
  return out;
}

/** Trim whitespace off a selection `[from, to)` over `src`. */
function trimRange(src: string, from: number, to: number): [number, number] {
  let a = from, b = to;
  while (a < b && /\s/.test(src[a])) a++;
  while (b > a && /\s/.test(src[b - 1])) b--;
  return [a, b];
}

/** The host-expression node a selection exactly spans, or null when it doesn't
 *  align to one (or sits inside mini-notation). Used to validate "Extract". */
export function extractTarget(tree: Tree, src: string, from: number, to: number): { from: number; to: number } | null {
  const [a, b] = trimRange(src, from, to);
  if (a >= b) return null;
  const node = tree.rootNode.descendantForIndex(a, b - 1);
  if (!node || node.startIndex !== a || node.endIndex !== b) return null;
  if (MINI_TYPES.has(node.type) || !EXPR_TYPES.has(node.type)) return null;
  return { from: a, to: b };
}

// ── Extract → let ────────────────────────────────────────────────────────────────

/** Lift the selection `[from, to)` into a new top-level `let name = <selection>`
 *  (inserted before the enclosing item) and replace the selection with `name`.
 *  Rejects a selection that isn't a clean host expression, lives inside mini-
 *  notation, or depends on a lambda parameter. */
export function extractLetPlan(
  tree: Tree, src: string, from: number, to: number, name: string,
): RefactorPlan {
  const newName = name.trim();
  const bad = validateNewName(newName, tree);
  if (bad) return { changes: [], error: bad };

  const node = tree.rootNode.descendantForIndex(from, to - 1);
  if (!node || node.startIndex !== from || node.endIndex !== to) {
    return { changes: [], error: 'Select a complete pattern' };
  }
  if (MINI_TYPES.has(node.type) || !EXPR_TYPES.has(node.type)) {
    return { changes: [], error: 'Select a complete pattern (not inside mini-notation)' };
  }
  const params = enclosingLambdaParams(node);
  if (params.size) {
    const used = subtreeIdentifiers(node);
    for (const p of params) {
      if (used.has(p)) return { changes: [], error: `Selection uses the local parameter "${p}"` };
    }
  }
  const item = topLevelItemOf(tree, node);
  if (!item) return { changes: [], error: "Couldn't find where to place the let" };

  const exprText = src.slice(from, to);
  return {
    changes: [
      { from: item.startIndex, to: item.startIndex, insert: `let ${newName} = ${exprText}\n` },
      { from, to, insert: newName },
    ],
    note: `Extracted ${newName}`,
  };
}

// ── Inline let ────────────────────────────────────────────────────────────────────

/** The `let_binding` node declaring `name`, or null. */
function findLetBinding(tree: Tree, name: string): Node | null {
  for (const item of tree.rootNode.namedChildren) {
    if (item?.type === 'let_binding' && item.childForFieldName('name')?.text === name) return item;
  }
  return null;
}

/** True when `offset` sits inside a `$splice` (mini-notation) — an inline target
 *  whose value is a host expression can't substitute there. */
function insideSplice(tree: Tree, offset: number): boolean {
  for (let cur: Node | null = tree.rootNode.descendantForIndex(offset); cur; cur = cur.parent) {
    if (cur.type === 'splice') return true;
  }
  return false;
}

/** Inline the `let name = value` into each of its uses and delete the declaration.
 *  Wraps a low-precedence value in parens. Rejects when `name` isn't a `let`, is
 *  unused, or is spliced into mini-notation (where a host value can't go). */
export function inlinePlan(tree: Tree, src: string, name: string): RefactorPlan {
  const letNode = findLetBinding(tree, name);
  if (!letNode) return { changes: [], error: `"${name}" is not a let you can inline` };
  const valueNode = letNode.childForFieldName('value');
  if (!valueNode) return { changes: [], error: `"${name}" has no value` };

  // Uses = every occurrence outside the declaration itself.
  const uses = identifierUsages(tree, name)
    .filter((r) => r.from < letNode.startIndex || r.from >= letNode.endIndex);
  if (!uses.length) return { changes: [], error: `"${name}" is never used` };
  for (const u of uses) {
    if (insideSplice(tree, u.from)) {
      return { changes: [], error: `"${name}" is spliced into mini-notation — can't inline` };
    }
  }

  let valueText = src.slice(valueNode.startIndex, valueNode.endIndex);
  if (NEEDS_PARENS.has(valueNode.type)) valueText = `(${valueText})`;

  const changes: EditChange[] = uses.map((u) => ({ from: u.from, to: u.to, insert: valueText }));
  // Remove the whole declaration line (its line start → start of the next line).
  const lineStart = src.lastIndexOf('\n', letNode.startIndex - 1) + 1;
  const nl = src.indexOf('\n', letNode.endIndex);
  const lineEnd = nl === -1 ? src.length : nl + 1;
  changes.push({ from: lineStart, to: lineEnd, insert: '' });

  changes.sort((a, b) => a.from - b.from || a.to - b.to);
  return { changes, note: `Inlined ${uses.length} use${uses.length === 1 ? '' : 's'}` };
}

// ── Fresh-name suggestion (Extract default) ──────────────────────────────────────

/** A name not already declared in the file: `base`, else `base2`, `base3`, … */
export function freshName(tree: Tree, base: string): string {
  const { defs, imports } = extractSymbols(tree);
  const taken = (n: string) => defs.has(n) || imports.has(n);
  if (!taken(base)) return base;
  let i = 2;
  while (taken(`${base}${i}`)) i++;
  return `${base}${i}`;
}
