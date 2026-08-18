/**
 * Laying out the module graph — geometry only, and pure.
 *
 * The *analysis* (layers, cycles, transitive counts) is the backend's, in
 * `bennu_deps::module_graph`: it is about the project, it is worth unit tests, and it does not change
 * when the window is resized. What is left here is where each box goes and what curve joins two of
 * them, which depends on the label widths and the zoom and belongs beside the thing that draws it.
 *
 * ## Left to right, dependents first
 *
 * The layer a node is on comes from the backend; this turns it into a **column**, with the *highest*
 * layer on the left. So a chain reads the way the sentence does — `app → core → util`, dependents on
 * the left, the foundation on the right — and every arrow in a healthy graph points rightwards, in
 * the reading direction.
 *
 * That has a property worth more than the aesthetics: **any leftward arrow is a cycle**. A reader
 * spots one without consulting a legend, and it agrees with what the backend flagged rather than
 * being a second opinion.
 *
 * ## Long edges are routed, not drawn straight
 *
 * The thing that turns a layered graph into spaghetti is the edge that spans four columns: drawn as
 * one curve it passes *through* every box in between, and twenty of them make a picture nobody can
 * read. So an edge crossing intermediate columns gets a **bend point in each of them** — a slot of its
 * own, reserved in that column beside the boxes — and is drawn as a smooth line through those points.
 * Two things fall out: the edge no longer crosses a box, and the bends take part in the ordering
 * below, which pulls long edges into near-straight horizontal lines instead of letting them wander.
 *
 * This is the routing half of Sugiyama's method, and it is the difference between a diagram and a ball
 * of wool. It costs vertical space — the bends occupy rows — which is the trade, and the right one: a
 * taller picture you can follow beats a compact one you cannot.
 *
 * ## Ordering inside a column
 *
 * Barycentre sweeps: a slot wants to sit level with the average position of its neighbours, so
 * repeatedly ordering each column by that average pulls connected things into line and takes most of
 * the crossings out. Four passes, alternating direction — a fifth changes almost nothing on a real
 * workspace, and this runs on every layout.
 *
 * It is a heuristic, not a minimum: crossing minimisation is NP-hard, and a layout that is *stable*
 * and *fast* beats one that is optimal and neither. Ties break on the backend's node order, which is
 * manifest declaration order, so the same project always draws the same picture.
 */

import type { GraphEdge, GraphNode, ModuleGraph } from '$lib/ipc/bennu/deps';

/** Box height. Fixed: every node carries one line of content. */
export const NODE_H = 30;
/** The row a bend point occupies. Thin — it is a line passing through, not a thing. */
const BEND_H = 9;
/** Vertical gap between slots in a column. */
const GAP_Y = 11;
/** Horizontal gap between columns. Generous — it is the room the edge curves need to be readable. */
const GAP_X = 62;
/** Box width bounds. A long crate name ellipsizes rather than stretching the column. */
const MIN_W = 104;
const MAX_W = 230;
/** Rough advance per character at the label's size. */
const CHAR_W = 6.6;
/**
 * Room inside a box that is not label: the kind bar and the left padding, plus the right-hand column
 * the rebuild count sits in.
 *
 * One constant, read by both the width estimate and {@link clipLabel}, because the two disagreeing is
 * how a name ends up ellipsized inside a box that had room for it.
 */
const BOX_CHROME = 42;
/** How many barycentre sweeps to run. */
const PASSES = 4;
/**
 * Ceiling on routed bends.
 *
 * A dense graph can want thousands (edges × columns crossed), and past a point the bends cost more
 * vertical space than the routing buys in clarity. Above this every edge falls back to a direct
 * curve — a worse picture, but a picture.
 */
const MAX_BENDS = 900;

/** A node with a place. */
export interface PlacedNode {
  /** Index into `ModuleGraph.nodes` — the identity everything else keys on. */
  index: number;
  node: GraphNode;
  x: number;
  y: number;
  w: number;
  h: number;
  /** Which column it landed in (0 = leftmost = the most dependent). */
  column: number;
}

/** An edge with a curve. */
export interface PlacedEdge {
  edge: GraphEdge;
  /** SVG path data — a smooth line through the routed bends, or one curve when it has none. */
  path: string;
  /** Whether it points leftwards: against the layering, which only a cycle edge does. */
  backward: boolean;
  /** A point on the line, for a hit target or a label. */
  mid: { x: number; y: number };
}

/** A column, for the ticks that say what left-to-right means. */
export interface PlacedColumn {
  x: number;
  width: number;
  /** The backend's layer number — kept even when columns are re-indexed, so the tick cannot lie. */
  layer: number;
  /** How many modules are in it (bends excluded). */
  count: number;
}

export interface GraphLayout {
  nodes: PlacedNode[];
  edges: PlacedEdge[];
  columns: PlacedColumn[];
  /** Full extent, for the SVG's viewBox. */
  width: number;
  height: number;
}

export interface LayoutOptions {
  /**
   * When given, only these node indices are laid out — **solo** mode.
   *
   * A filter and not a dimming: the point of isolating one crate's world is that everything else
   * stops taking up room, so the columns are recomputed from what is left and the empty ones collapse.
   * Dimming answers a different question (that is the search) and leaves the picture the same size.
   */
  only?: Set<number> | null;
}

/** Estimated box width for a label. Rounded **up**: rounding down costs the last character. */
function widthOf(label: string): number {
  return Math.max(MIN_W, Math.min(MAX_W, Math.ceil(label.length * CHAR_W + BOX_CHROME)));
}

/**
 * The label, cut to what fits in a box of `w`.
 *
 * Here rather than in the canvas so it shares [`BOX_CHROME`] and [`CHAR_W`] with the width estimate:
 * while the two were separate constants, a name that had been *given* a box wide enough for it was
 * still ellipsized by the renderer.
 *
 * SVG has no `text-overflow`, and `<foreignObject>` inside a scaled SVG is a WebView rendering-bug
 * generator, so cutting by character count is the option that works.
 */
export function clipLabel(label: string, w: number): string {
  const fits = Math.max(3, Math.floor((w - BOX_CHROME) / CHAR_W));
  return label.length <= fits ? label : `${label.slice(0, fits - 1)}…`;
}

/** A row in a column: either a module, or one edge passing through. */
type Slot = { kind: 'node'; index: number } | { kind: 'bend'; edge: number; seq: number };

const slotKey = (s: Slot): string => (s.kind === 'node' ? `n${s.index}` : `b${s.edge}.${s.seq}`);

/**
 * Place every node, route every edge.
 *
 * `edges` is passed separately from `graph.edges` so the caller can lay out a **subset** — hiding dev
 * dependencies — and get a layout that is genuinely tighter rather than the whole picture with lines
 * painted out.
 */
export function layoutGraph(
  graph: ModuleGraph,
  edges: GraphEdge[],
  options: LayoutOptions = {},
): GraphLayout {
  const only = options.only ?? null;
  const shown = (i: number) => !only || only.has(i);

  const visible = graph.nodes.map((_, i) => i).filter(shown);
  if (!visible.length) return { nodes: [], edges: [], columns: [], width: 0, height: 0 };

  const live = edges.filter((e) => shown(e.from) && shown(e.to));

  // ── Columns ────────────────────────────────────────────────────────────────
  // Layer → column, mirrored so the deepest layer is leftmost, then **densified**: in solo mode whole
  // layers can be empty, and leaving their columns in place would put unexplained gaps through the
  // middle of the picture.
  const usedLayers = [...new Set(visible.map((i) => graph.nodes[i].layer))].sort((a, b) => b - a);
  const columnOfLayer = new Map(usedLayers.map((layer, at) => [layer, at]));
  const colOf = (i: number) => columnOfLayer.get(graph.nodes[i].layer) ?? 0;

  const columns: Slot[][] = Array.from({ length: usedLayers.length }, () => []);
  for (const i of visible) columns[colOf(i)].push({ kind: 'node', index: i });

  // ── Route the long edges ───────────────────────────────────────────────────
  // `chains[at]` is the slot sequence an edge travels: source, its bends, target. Only *forward* edges
  // are routed — a backward one is a cycle, drawn as an arc in the gutter, and giving it bends would
  // hide the very thing its shape exists to show.
  const wanted = live.reduce((sum, e) => sum + Math.max(0, colOf(e.to) - colOf(e.from) - 1), 0);
  const routing = wanted <= MAX_BENDS;

  const chains: Slot[][] = live.map((e, at) => {
    const ends: Slot[] = [
      { kind: 'node', index: e.from },
      { kind: 'node', index: e.to },
    ];
    const span = colOf(e.to) - colOf(e.from);
    if (!routing || span <= 1) return ends;
    const bends: Slot[] = [];
    for (let seq = 1; seq < span; seq += 1) {
      const bend: Slot = { kind: 'bend', edge: at, seq };
      columns[colOf(e.from) + seq].push(bend);
      bends.push(bend);
    }
    return [ends[0], ...bends, ends[1]];
  });

  // ── Order inside each column ───────────────────────────────────────────────
  const slotAt = new Map<string, number>();
  const noteSlots = () =>
    columns.forEach((col) => col.forEach((s, at) => slotAt.set(slotKey(s), at)));
  noteSlots();

  const rightNb = new Map<string, string[]>();
  const leftNb = new Map<string, string[]>();
  const link = (a: Slot, b: Slot) => {
    const ka = slotKey(a);
    const kb = slotKey(b);
    rightNb.set(ka, [...(rightNb.get(ka) ?? []), kb]);
    leftNb.set(kb, [...(leftNb.get(kb) ?? []), ka]);
  };
  chains.forEach((chain, at) => {
    const e = live[at];
    // A backward edge joins two columns in the wrong order; it must not vote on the ordering, or it
    // would drag its endpoints towards each other and undo the layering everything else follows.
    if (colOf(e.to) <= colOf(e.from)) return;
    for (let k = 0; k + 1 < chain.length; k += 1) link(chain[k], chain[k + 1]);
  });

  const barycentre = (key: string, side: Map<string, string[]>): number | null => {
    const nb = side.get(key);
    if (!nb?.length) return null;
    let sum = 0;
    let seen = 0;
    for (const other of nb) {
      const at = slotAt.get(other);
      if (at === undefined) continue;
      sum += at;
      seen += 1;
    }
    return seen ? sum / seen : null;
  };

  for (let pass = 0; pass < PASSES; pass += 1) {
    // Alternate the side each pass reads from, so information travels both ways along the graph.
    const side = pass % 2 === 0 ? rightNb : leftNb;
    for (const col of columns) {
      // Only slots that HAVE a neighbour on this side take part. One with none has no opinion about
      // where it should be, and sorting it against a number it does not have would sweep it to one end
      // of the column — which is how an isolated crate ends up pinned to a corner it has no reason to
      // be in. The others are reordered *among the slots they already occupy*.
      const opinionated = col
        .map((s, at) => ({ s, at, key: barycentre(slotKey(s), side) }))
        .filter((k): k is { s: Slot; at: number; key: number } => k.key !== null);
      if (opinionated.length < 2) continue;
      const sorted = [...opinionated].sort((a, b) => a.key - b.key || a.at - b.at);
      const next = [...col];
      opinionated.forEach((k, at) => { next[k.at] = sorted[at].s; });
      col.splice(0, col.length, ...next);
    }
    noteSlots();
  }

  // ── Geometry ───────────────────────────────────────────────────────────────
  const widths = new Map<number, number>(
    visible.map((i) => [i, widthOf(graph.nodes[i].name || graph.nodes[i].id)]),
  );
  const heightOf = (s: Slot) => (s.kind === 'node' ? NODE_H : BEND_H);

  const columnWidth = columns.map((col) =>
    col.reduce((m, s) => (s.kind === 'node' ? Math.max(m, widths.get(s.index) ?? MIN_W) : m), MIN_W),
  );
  const columnX: number[] = [];
  let x = 0;
  for (let c = 0; c < columns.length; c += 1) {
    columnX.push(x);
    x += columnWidth[c] + GAP_X;
  }
  const width = Math.max(0, x - GAP_X);

  const columnHeight = columns.map((col) =>
    col.reduce((sum, s, at) => sum + heightOf(s) + (at ? GAP_Y : 0), 0),
  );
  const height = Math.max(NODE_H, ...columnHeight);

  const placed: PlacedNode[] = [];
  /** Where each bend ended up, so the paths can be drawn through them. */
  const bendAt = new Map<string, { x: number; y: number }>();

  columns.forEach((col, c) => {
    // Columns are centred against the tallest: shorter edges on average, and a picture whose mass is
    // in the middle rather than hanging off the top.
    let y = (height - columnHeight[c]) / 2;
    for (const s of col) {
      const h = heightOf(s);
      if (s.kind === 'node') {
        const w = widths.get(s.index) ?? MIN_W;
        placed.push({
          index: s.index,
          node: graph.nodes[s.index],
          // Boxes are centred in their column, so an arrow between two average-width nodes stays
          // horizontal instead of stepping.
          x: columnX[c] + (columnWidth[c] - w) / 2,
          y,
          w,
          h,
          column: c,
        });
      } else {
        bendAt.set(slotKey(s), { x: columnX[c] + columnWidth[c] / 2, y: y + h / 2 });
      }
      y += h + GAP_Y;
    }
  });

  const byIndex = new Map(placed.map((p) => [p.index, p]));
  const routed: PlacedEdge[] = [];
  live.forEach((edge, at) => {
    const from = byIndex.get(edge.from);
    const to = byIndex.get(edge.to);
    if (!from || !to) return;
    const bends = chains[at]
      .filter((s) => s.kind === 'bend')
      .map((s) => bendAt.get(slotKey(s)))
      .filter((p): p is { x: number; y: number } => !!p);
    routed.push(route(edge, from, to, bends));
  });

  const placedColumns: PlacedColumn[] = columns.map((col, c) => ({
    x: columnX[c],
    width: columnWidth[c],
    layer: usedLayers[c],
    count: col.filter((s) => s.kind === 'node').length,
  }));

  return { nodes: placed, edges: routed, columns: placedColumns, width, height };
}

/**
 * The line between two boxes, through any bends it was routed along.
 *
 * Anchored on the **sides** rather than the centres, so an arrowhead lands on the border of the box it
 * points at instead of underneath its label, and every control point is horizontal: the line leaves,
 * passes each bend and arrives level, which is what turns a bundle of edges into a readable fan rather
 * than a knot.
 */
function route(
  edge: GraphEdge,
  from: PlacedNode,
  to: PlacedNode,
  bends: { x: number; y: number }[],
): PlacedEdge {
  const fromMid = from.y + from.h / 2;
  const toMid = to.y + to.h / 2;
  const forward = to.x > from.x;
  // Two boxes a cycle put in the same column. There is no horizontal room between them to route
  // through, so both ends leave by the *left* and the curve bows out into the gutter — the readable
  // shape for "these two point at each other". Anchoring one end on the right instead would drag the
  // curve straight across both boxes.
  const stacked = Math.abs(to.x - from.x) < 8;

  const start = { x: forward ? from.x + from.w : from.x, y: fromMid };
  const end = { x: forward || stacked ? to.x : to.x + to.w, y: toMid };

  if (forward && bends.length) {
    return {
      edge,
      path: smooth([start, ...bends, end]),
      backward: false,
      mid: bends[Math.floor(bends.length / 2)],
    };
  }

  const span = Math.abs(end.x - start.x);
  // How far the curve leaves horizontally before it turns. Capped at **half the span** for a forward
  // edge: past that the two control points cross and the curve develops a wobble on exactly the short
  // hops a layered graph is mostly made of. A backward or stacked edge has no span to work with, so it
  // gets a fixed push instead — that is what makes it an arc rather than a straight line hidden behind
  // the boxes.
  const reach = forward ? Math.min(Math.max(span * 0.45, 12), 90) : Math.max(36, span * 0.45);
  const c1 = forward ? start.x + reach : start.x - reach;
  const c2 = forward ? end.x - reach : end.x - (stacked ? reach : -reach);

  return {
    edge,
    path: `M ${round(start.x)} ${round(start.y)} C ${round(c1)} ${round(start.y)}, ${round(c2)} ${round(end.y)}, ${round(end.x)} ${round(end.y)}`,
    backward: !forward,
    mid: { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2 },
  };
}

/**
 * A smooth line through `points`, with a horizontal tangent at every one of them.
 *
 * One cubic per segment, control points pushed halfway along the gap: the tangent is horizontal on
 * both sides of each bend, so consecutive segments meet without a visible corner and a routed edge
 * reads as one line rather than a chain of arcs.
 */
function smooth(points: { x: number; y: number }[]): string {
  let d = `M ${round(points[0].x)} ${round(points[0].y)}`;
  for (let i = 0; i + 1 < points.length; i += 1) {
    const a = points[i];
    const b = points[i + 1];
    const dx = Math.max(8, (b.x - a.x) / 2);
    d += ` C ${round(a.x + dx)} ${round(a.y)}, ${round(b.x - dx)} ${round(b.y)}, ${round(b.x)} ${round(b.y)}`;
  }
  return d;
}

/** Path data is markup: a rounded number is a shorter attribute and an identical picture. */
function round(v: number): number {
  return Math.round(v * 10) / 10;
}

/** Everything a module is built on, and everything built on it — what solo mode keeps. */
export interface Neighbourhood {
  /** Transitively reachable dependencies, excluding the module itself. */
  dependencies: Set<number>;
  /** Transitive dependents. */
  dependents: Set<number>;
  /** Direct dependencies, in the graph's order. */
  directDependencies: number[];
  /** Direct dependents. */
  directDependents: number[];
}

/**
 * Who is up- and downstream of one module.
 *
 * The transitive halves are what solo mode keeps on screen: "this crate and everything a change to it
 * touches" is a *reachability* question, and showing only the direct neighbours of a crate in the
 * middle of a workspace answers a different, less useful one. The direct lists are what the detail
 * panel lists, because those are the ones you can go and edit.
 */
export function neighbourhood(graph: ModuleGraph, index: number, edges: GraphEdge[]): Neighbourhood {
  const out = new Map<number, number[]>();
  const inc = new Map<number, number[]>();
  for (const e of edges) {
    out.set(e.from, [...(out.get(e.from) ?? []), e.to]);
    inc.set(e.to, [...(inc.get(e.to) ?? []), e.from]);
  }
  const walk = (adj: Map<number, number[]>): Set<number> => {
    const seen = new Set<number>();
    const stack = [index];
    while (stack.length) {
      for (const next of adj.get(stack.pop()!) ?? []) {
        if (next === index || seen.has(next)) continue;
        seen.add(next);
        stack.push(next);
      }
    }
    return seen;
  };
  const distinct = (list: number[] | undefined) => [...new Set(list ?? [])].sort((a, b) => a - b);
  return {
    dependencies: walk(out),
    dependents: walk(inc),
    directDependencies: distinct(out.get(index)),
    directDependents: distinct(inc.get(index)),
  };
}
