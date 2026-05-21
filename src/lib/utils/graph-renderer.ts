import type { CommitNode, GraphEdge } from '../types/git';

export const ROW_HEIGHT      = 28;
export const LANE_WIDTH      = 26;
export const NODE_RADIUS     = 10;
/** Minimum length (px) of the SVG path segment between two nodes.
 *  The path is centered in the inter-node gap; any extension beyond the
 *  natural gap sits behind the opaque node circle and is invisible. */
export const MIN_EDGE_LENGTH = 8;

export function nodeX(lane: number): number {
  return LANE_WIDTH * lane + LANE_WIDTH / 2;
}

export function nodeY(row: number): number {
  return ROW_HEIGHT * row + ROW_HEIGHT / 2;
}

export function edgePath(edge: GraphEdge): string {
  const x1 = nodeX(edge.from_lane);
  const y1 = nodeY(edge.from_row);
  const x2 = nodeX(edge.to_lane);
  const y2 = nodeY(edge.to_row);

  // Same lane → straight vertical line.
  // Clip endpoints to just outside the node circle so the line starts/ends
  // at the node boundary rather than its centre.  If the resulting gap is
  // shorter than MIN_EDGE_LENGTH we extend symmetrically around the midpoint
  // (the extension hides behind the opaque node circles and is invisible).
  if (x1 === x2) {
    const CLIP = NODE_RADIUS + 1;
    const dir  = y2 > y1 ? 1 : -1;
    const rawGap = Math.abs(y2 - y1) - 2 * CLIP;
    const len    = Math.max(rawGap, MIN_EDGE_LENGTH);
    const mid    = (y1 + y2) / 2;
    return `M ${x1} ${mid - dir * len / 2} L ${x1} ${mid + dir * len / 2}`;
  }

  // Shape rule (read bottom-to-top, how the user reads the graph):
  //
  //  FORK edges (pi=0, branch opening):  VERTICAL first, then HORIZONTAL.
  //    fork_left  (x2 < x1) — ⌟ shape: start at dest → go RIGHT → go UP
  //    fork_right (x2 > x1) — ⌞ shape: start at dest → go LEFT  → go UP
  //
  //  MERGE edges (pi>0, merge commit secondary parent): HORIZONTAL first, then VERTICAL.
  //    merge_left  (x2 < x1) — ┌ shape: start at dest → go UP → go RIGHT
  //    merge_right (x2 > x1) — ┐ shape: start at dest → go UP → go LEFT

  const isFork = edge.edge_type === 'fork_left' || edge.edge_type === 'fork_right';
  const sx     = x2 > x1 ? 1 : -1;
  const r      = Math.min(6, Math.abs(x2 - x1) / 2, (y2 - y1) / 2);

  if (isFork) {
    // VERTICAL first on source lane, then HORIZONTAL at destination row.
    if (r <= 0) return `M ${x1} ${y1} L ${x1} ${y2} L ${x2} ${y2}`;
    return [
      `M ${x1} ${y1}`,
      `L ${x1} ${y2 - r}`,
      `Q ${x1} ${y2} ${x1 + sx * r} ${y2}`,
      `L ${x2} ${y2}`,
    ].join(' ');
  } else {
    // HORIZONTAL first at source row, then VERTICAL at destination lane.
    if (r <= 0) return `M ${x1} ${y1} L ${x2} ${y1} L ${x2} ${y2}`;
    return [
      `M ${x1} ${y1}`,
      `L ${x2 - sx * r} ${y1}`,
      `Q ${x2} ${y1} ${x2} ${y1 + r}`,
      `L ${x2} ${y2}`,
    ].join(' ');
  }
}

export const LANE_COLORS = [
  'var(--graph-lane-0)',
  'var(--graph-lane-1)',
  'var(--graph-lane-2)',
  'var(--graph-lane-3)',
  'var(--graph-lane-4)',
  'var(--graph-lane-5)',
  'var(--graph-lane-6)',
  'var(--graph-lane-7)',
  'var(--graph-lane-8)',
  'var(--graph-lane-9)',
];

export function laneColor(colorIndex: number): string {
  return LANE_COLORS[colorIndex % LANE_COLORS.length];
}

export function svgWidth(laneCount: number): number {
  return Math.max(laneCount * LANE_WIDTH + LANE_WIDTH, 80);
}

export function svgHeight(rowCount: number): number {
  return rowCount * ROW_HEIGHT;
}

export function visibleRows(
  scrollTop: number,
  viewportHeight: number,
  buffer = 80,
): [number, number] {
  // Default 80 rows × 28px = 2240px on each side. Caller can pass a larger
  // buffer when the viewport is idle — pre-renders rows for the next jump.
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - buffer);
  const last  = Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) + buffer;
  return [first, last];
}
