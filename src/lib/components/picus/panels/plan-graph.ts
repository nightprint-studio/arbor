/**
 * A query plan as a drawable tree — geometry, and the two judgements both plan
 * views have to agree on.
 *
 * The backend hands over a **flat, depth-tagged list in execution-tree order**, which
 * is exactly right for the indented list and not enough for a diagram. This rebuilds
 * the tree from the depths, lays it out, and works out the two numbers a picture is
 * worth drawing: how much of the work happens *at* each node, and how many rows travel
 * along each edge.
 *
 * It is a plain module, not a component, for the usual reason: this is the part with
 * arithmetic in it, and arithmetic inside a `.svelte` file is arithmetic nobody tests.
 *
 * ## Why the thresholds live here
 *
 * "Did the planner get this badly wrong" is asked by the list and by the graph, and a
 * node the list badges amber while the graph draws it calm is two tools disagreeing in
 * the same panel. One definition, imported twice.
 */

import type { PlanNode, QueryPlan } from '$lib/ipc/picus/plan';

// ── The shared judgements ─────────────────────────────────────────────────────

/**
 * How far the estimate missed, as a signed factor — positive when the planner
 * expected too few rows. `null` when there is nothing to compare.
 *
 * Clamped to one on both sides: a zero is a division, and "the planner expected none
 * and got none" is not a discrepancy.
 */
export function deviation(plan: QueryPlan, node: PlanNode): number | null {
  if (!plan.analyzed || node.rows === null || node.actualRows === null) return null;
  const expected = Math.max(node.rows, 1);
  const got = Math.max(node.actualRows, 1);
  return got >= expected ? got / expected : -(expected / got);
}

/**
 * Only a real discrepancy is marked.
 *
 * Under a factor of two the planner was right; marking that would put a badge on every
 * node of every plan, which is the same as marking nothing.
 */
export const MARK_FROM = 2;
/** Where a discrepancy stops being interesting and starts being the answer. */
export const SERIOUS_FROM = 10;

/** How wrong this node was, as the two views both grade it. */
export function severityOf(plan: QueryPlan, node: PlanNode): 'none' | 'warn' | 'bad' {
  const off = deviation(plan, node);
  if (off === null) return 'none';
  const size = Math.abs(off);
  if (size >= SERIOUS_FROM) return 'bad';
  return size >= MARK_FROM ? 'warn' : 'none';
}

export function formatRows(n: number | null): string {
  return n === null ? '—' : Math.round(n).toLocaleString();
}

export function formatCost(n: number | null): string {
  return n === null ? '—' : n.toFixed(2);
}

// ── Geometry ──────────────────────────────────────────────────────────────────

/** Box size and spacing, in the SVG's own units. */
export const NODE_W = 172;
export const NODE_H = 58;
const H_GAP = 22;
const V_GAP = 46;

export interface PlanGraphNode {
  /** Index into `plan.nodes` — the identity every consumer addresses a node by. */
  index: number;
  node: PlanNode;
  parent: number | null;
  children: number[];
  /** Top-left corner of the box. */
  x: number;
  y: number;
  /**
   * Share of the plan's work done **at this node**, `0..1`.
   *
   * The engine reports costs and times **inclusively** — a node's number covers its
   * whole subtree — so the interesting quantity is the difference between a node and
   * its children. Without it every plan looks like one enormous root, which is true
   * and useless.
   *
   * Measured time when the plan was analysed, estimated cost otherwise; the view says
   * which, because the two are not the same claim.
   */
  share: number;
  /**
   * Rows leaving this node, per loop — what the edge to its parent carries. The real
   * count when it was measured, the estimate otherwise.
   */
  outRows: number | null;
}

export interface PlanGraph {
  nodes: PlanGraphNode[];
  width: number;
  height: number;
  /** True when `share` was computed from measured time rather than estimated cost. */
  measured: boolean;
  /** The largest `outRows` in the plan — the scale edge thickness is read against. */
  maxRows: number;
}

/**
 * Lay the plan out as a tidy top-down tree: the root on top, its inputs below it.
 *
 * **Top-down, not SSMS's right-to-left.** Rows do flow from the leaves to the root, and
 * SSMS spends that fact on a right-to-left diagram — which puts the answer on the left
 * of a reader who starts on the left, and disagrees with the indented list one tab
 * away, where the root is at the top. Same information, one reading order.
 *
 * The algorithm is the classic one: leaves take the next free column, a parent is
 * centred over the span of its children. Plans are tens of nodes deep at most, so the
 * recursion is safe and the quadratic-looking centring never bites.
 */
export function layoutPlan(plan: QueryPlan): PlanGraph {
  const source = plan.nodes;
  const n = source.length;
  const parent = new Array<number | null>(n).fill(null);
  const children: number[][] = Array.from({ length: n }, () => []);

  // The list is pre-order with a depth on each entry, so the ancestor chain is a stack:
  // at depth d the parent is whatever sits at depth d-1.
  const stack: number[] = [];
  for (let i = 0; i < n; i += 1) {
    const depth = Math.max(0, source[i].depth);
    while (stack.length > depth) stack.pop();
    const owner = stack.length ? stack[stack.length - 1] : null;
    parent[i] = owner;
    if (owner !== null) children[owner].push(i);
    stack.push(i);
  }

  // ── Where the work happens ──
  // Inclusive numbers minus the children's, floored at zero: a plan whose numbers do
  // not add up (a missing cost, a node the engine reports oddly) must not produce a
  // negative share that then renders as a bar pointing the wrong way.
  const measured = plan.analyzed && source.some((s) => s.actualMs !== null);
  const own = (i: number): number => {
    const value = measured ? source[i].actualMs : source[i].cost;
    if (value === null) return 0;
    const kids = children[i].reduce((sum, c) => {
      const child = measured ? source[c].actualMs : source[c].cost;
      return sum + (child ?? 0);
    }, 0);
    return Math.max(0, value - kids);
  };
  const selves = source.map((_, i) => own(i));
  const totalSelf = selves.reduce((a, b) => a + b, 0);

  // ── Columns ──
  let cursor = 0;
  const x = new Array<number>(n).fill(0);
  const place = (i: number) => {
    const kids = children[i];
    if (kids.length === 0) {
      x[i] = cursor;
      cursor += NODE_W + H_GAP;
      return;
    }
    kids.forEach(place);
    x[i] = (x[kids[0]] + x[kids[kids.length - 1]]) / 2;
  };
  for (let i = 0; i < n; i += 1) if (parent[i] === null) place(i);

  const nodes: PlanGraphNode[] = source.map((node, i) => ({
    index: i,
    node,
    parent: parent[i],
    children: children[i],
    x: x[i],
    y: Math.max(0, node.depth) * (NODE_H + V_GAP),
    share: totalSelf > 0 ? selves[i] / totalSelf : 0,
    outRows: plan.analyzed && node.actualRows !== null ? node.actualRows : node.rows,
  }));

  const depth = source.reduce((max, s) => Math.max(max, Math.max(0, s.depth)), 0);
  return {
    nodes,
    width: Math.max(NODE_W, cursor - H_GAP),
    height: (depth + 1) * (NODE_H + V_GAP) - V_GAP,
    measured,
    maxRows: nodes.reduce((max, g) => Math.max(max, g.outRows ?? 0), 0),
  };
}

/** Thinnest and thickest an edge may be drawn, in SVG units. */
const EDGE_MIN = 1.25;
const EDGE_MAX = 9;

/**
 * How thick the edge carrying `rows` should be, against a plan whose busiest edge
 * carries `maxRows`.
 *
 * **Logarithmic**, because row counts in one plan routinely span six orders of
 * magnitude: linear thickness would draw every edge but one as a hairline, and the one
 * fact this borrows from SSMS — you *see* where the rows explode — would be lost in the
 * plans that need it most.
 */
export function edgeWidth(rows: number | null, maxRows: number): number {
  if (rows === null || rows <= 0 || maxRows <= 0) return EDGE_MIN;
  const ratio = Math.log10(1 + rows) / Math.log10(1 + maxRows);
  return EDGE_MIN + (EDGE_MAX - EDGE_MIN) * Math.min(1, Math.max(0, ratio));
}

/**
 * The elbow from a child's top edge up into its parent's bottom edge.
 *
 * Orthogonal with rounded corners rather than a curve: a tree of boxes read as a
 * circuit, and two beziers crossing at a shallow angle are much harder to follow than
 * two right angles.
 */
export function edgePath(child: PlanGraphNode, owner: PlanGraphNode): string {
  const x1 = child.x + NODE_W / 2;
  const y1 = child.y;
  const x2 = owner.x + NODE_W / 2;
  const y2 = owner.y + NODE_H;
  if (Math.abs(x1 - x2) < 0.5) return `M ${x1} ${y1} L ${x2} ${y2}`;
  const mid = (y1 + y2) / 2;
  const r = Math.min(10, Math.abs(x1 - x2) / 2, Math.abs(y1 - y2) / 2);
  const dir = x2 > x1 ? 1 : -1;
  return [
    `M ${x1} ${y1}`,
    `L ${x1} ${mid + r}`,
    `Q ${x1} ${mid} ${x1 + dir * r} ${mid}`,
    `L ${x2 - dir * r} ${mid}`,
    `Q ${x2} ${mid} ${x2} ${mid - r}`,
    `L ${x2} ${y2}`,
  ].join(' ');
}
