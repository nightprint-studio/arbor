/*
 * Granular node-tree patches — applies `plugin:form-update { op: "patch" }`
 * ops against the live `nodes` $state tree, mutating it in place (no re-mount).
 *
 * Sibling to the whole-tree `replace` path: where `replace` swaps the entire
 * tree, `patch` surgically merges props / appends children / removes nodes
 * addressed by their stable `id`. Used by studio-grade, high-frequency UIs
 * (log streams, lazy trees) that can't afford a full rebuild per update.
 *
 * Each op is a `FormPatchOp` (see `$lib/types/plugin`). Ops are applied in
 * order; an op whose target id isn't found is silently skipped.
 */
import type { FormNode, FormPatchOp } from '$lib/types/plugin';
import { normalizeNode } from './normalize';
import { toArr } from './helpers';

type Located = { arr: FormNode[]; idx: number; node: any };

/** Every array that may hold child nodes for a given node, across all
 *  branching node types (mirrors `flattenAll`'s traversal). */
function childArraysOf(n: any): FormNode[][] {
  const out: FormNode[][] = [];
  if (n.type === 'switch') {
    for (const a of Object.values(n.cases ?? {})) if (Array.isArray(a)) out.push(a as FormNode[]);
    if (Array.isArray(n.default)) out.push(n.default);
  } else if (n.type === 'tabs') {
    for (const t of toArr<any>(n.tabs)) if (Array.isArray(t?.children)) out.push(t.children);
  } else if (n.type === 'wizard') {
    for (const s of toArr<any>(n.steps)) if (Array.isArray(s?.children)) out.push(s.children);
  } else if (n.type === 'tree_layout') {
    for (const key of ['nav_children', 'nav_footer_children', 'content_children']) {
      if (Array.isArray(n[key])) out.push(n[key]);
    }
  } else if (n.type === 'pipeline_editor') {
    if (Array.isArray(n.step_detail_form)) out.push(n.step_detail_form);
  } else if (n.type === 'tree') {
    // A `tree` field holds its rows in `nodes` (FormTreeNode[]); each row nests
    // further rows in `children`, caught by the generic branch below on the
    // recursive descent. Exposing `nodes` makes a tree row addressable by its
    // own `id` — that's how lazy children land (a `merge`/`set` onto the
    // expanded row replaces its `children` and clears `loading`).
    if (Array.isArray(n.nodes)) out.push(n.nodes);
  } else if (Array.isArray(n.children)) {
    out.push(n.children);
  }
  return out;
}

/** Depth-first locate a node by id, returning its containing array + index so
 *  callers can mutate props or splice it out. */
function locate(roots: FormNode[], id: string): Located | null {
  const rec = (arr: FormNode[]): Located | null => {
    for (let i = 0; i < arr.length; i++) {
      const n = arr[i] as any;
      if (n?.id === id) return { arr, idx: i, node: n };
      for (const kids of childArraysOf(n)) {
        const hit = rec(kids);
        if (hit) return hit;
      }
    }
    return null;
  };
  return rec(roots);
}

/** Keys whose value, when assigned via merge/set, holds a child-node subtree
 *  that must be passed through `normalizeNode` so freshly-emitted nodes pick
 *  up auto-IDs (FormNodeLayout iterates with `{#each ... (child.id)}` — an
 *  array of un-ID'd siblings would all key as `undefined` and the renderer
 *  would keep the stale markup). Mirrors the arrays `childArraysOf` walks. */
const CHILD_ARRAY_KEYS = new Set<string>([
  'children',
  'nav_children', 'nav_footer_children', 'content_children',
  'nodes',
  'step_detail_form',
]);

function normalizeChildSlot(key: string, value: unknown): unknown {
  if (!CHILD_ARRAY_KEYS.has(key) || !Array.isArray(value)) return value;
  return (value as FormNode[]).map(normalizeNode);
}

/** Assign `value` at a path of segments inside `obj`, creating intermediate
 *  containers (object or array, inferred from the next segment) as needed. */
function setDeep(obj: any, path: (string | number)[], value: unknown): void {
  if (path.length === 0) return;
  let cur = obj;
  for (let i = 0; i < path.length - 1; i++) {
    const k = path[i];
    if (cur[k] == null || typeof cur[k] !== 'object') {
      cur[k] = typeof path[i + 1] === 'number' ? [] : {};
    }
    cur = cur[k];
  }
  cur[path[path.length - 1]] = value;
}

/**
 * Apply patch ops to the node tree in place. `roots` is the component's
 * `nodes` $state array — mutations on its proxied elements/arrays are reactive.
 */
export function applyPatchOps(roots: FormNode[], ops: FormPatchOp[]): void {
  if (!Array.isArray(ops)) return;
  for (const op of ops) {
    if (!op || typeof (op as any).id !== 'string') continue;
    const found = locate(roots, (op as any).id);
    if (!found) continue;
    const { node, arr, idx } = found;

    if ('remove' in op && op.remove) {
      arr.splice(idx, 1);
      continue;
    }
    if ('merge' in op && op.merge && typeof op.merge === 'object') {
      for (const [k, v] of Object.entries(op.merge)) node[k] = normalizeChildSlot(k, v);
      continue;
    }
    if ('set' in op && Array.isArray(op.set)) {
      const last = op.set[op.set.length - 1];
      const value = typeof last === 'string' ? normalizeChildSlot(last, op.value) : op.value;
      setDeep(node, op.set, value);
      continue;
    }
    if ('append' in op && op.append) {
      const to = op.to ?? 'children';
      if (!Array.isArray(node[to])) node[to] = [];
      node[to].push(normalizeNode(op.append));
      continue;
    }
  }
}
