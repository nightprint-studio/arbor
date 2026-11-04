/**
 * The Bevy Remote icon set: what the inspector's tree, tabs and headers draw instead of generic glyphs.
 *
 * One 16×16 grid, a 1.5 stroke with round caps and joins, `currentColor` and no fill — so every icon takes the
 * colour of the text around it and reads on a dark theme without a palette of its own. An icon is a list of
 * primitives; `BrpIcon.svelte` draws them.
 */

export type Prim =
  | { p: string }
  | { c: [cx: number, cy: number, r: number] }
  | { r: [x: number, y: number, w: number, h: number, radius: number] };

const path = (d: string): Prim => ({ p: d });
const circle = (cx: number, cy: number, r: number): Prim => ({ c: [cx, cy, r] });
const rect = (x: number, y: number, w: number, h: number, radius = 1.5): Prim => ({ r: [x, y, w, h, radius] });
/** A dot: a zero-length round-capped stroke. */
const dot = (x: number, y: number): Prim => path(`M${x} ${y}h.01`);

export const ICONS = {
  // ── What a node in the tree is ──
  /** An entity: a cube. */
  entity: [path('M8 1.8 13.5 4.9v6.2L8 14.2 2.5 11.1V4.9z'), path('M2.5 4.9 8 8l5.5-3.1M8 8v6.2')],
  /** An entity with children: a cube with a branch to a smaller one. */
  group: [path('M6 1.8 10.2 4.2v4.6L6 11.2 1.8 8.8V4.2z'), path('M1.8 4.2 6 6.6l4.2-2.4M6 6.6v4.6'), path('M10.2 11.4h1.3'), rect(11.5, 9.6, 3.4, 3.6, 0.7)],
  camera: [rect(1.8, 4.6, 12.4, 8.4, 1.6), circle(8, 8.9, 2.4), path('M5.4 4.6 6.4 3h3.2l1 1.6')],
  light: [path('M8 1.8a4.2 4.2 0 0 0-2.4 7.6c.5.4.8.9.8 1.6h3.2c0-.7.3-1.2.8-1.6A4.2 4.2 0 0 0 8 1.8Z'), path('M6.6 12.6h2.8M7.2 14.2h1.6')],
  mesh: [path('M8 2 14 12.6H2z'), path('M8 2 5.4 12.6M8 2l2.6 10.6M3.7 8.2h8.6')],
  ui: [rect(1.8, 2.8, 12.4, 10.4, 1.6), path('M1.8 6.2h12.4'), dot(4.2, 4.5), dot(6.4, 4.5)],
  sprite: [rect(1.8, 2.6, 12.4, 10.8, 1.6), circle(5.4, 6, 1.1), path('M2.2 11.8l3.9-3.7 2.5 2.4 1.9-1.7 3.4 3')],
  text: [path('M3 4.6V3.2h10v1.4M8 3.2v9.6M6.2 12.8h3.6')],
  window: [rect(1.8, 2.8, 12.4, 8.2, 1.4), path('M5.8 13.4h4.4M8 11v2.4')],
  /** A resource: a data store. */
  resource: [path('M2.5 4.2C2.5 3 5 2 8 2s5.5 1 5.5 2.2S11 6.4 8 6.4 2.5 5.4 2.5 4.2Z'), path('M2.5 4.2v7.6C2.5 13 5 14 8 14s5.5-1 5.5-2.2V4.2'), path('M2.5 8c0 1.2 2.5 2.2 5.5 2.2S13.5 9.2 13.5 8')],
  /** A state machine: two states and the move between them. */
  state: [circle(3.8, 8, 2.1), circle(12.2, 8, 2.1), path('M6.3 8h3.4M8.6 6.6 10 8l-1.4 1.4')],

  // ── What a component or a value is ──
  component: [rect(2.2, 2.2, 7.2, 7.2, 1.2), rect(6.6, 6.6, 7.2, 7.2, 1.2)],
  transform: [path('M3 13V3M3 13h10'), path('M3 13 9.6 6.4M7.2 6.2h2.6v2.6')],
  timer: [circle(8, 9, 5), path('M8 9V6.4M6.4 1.8h3.2')],
  handle: [path('M6.6 9.4a2.5 2.5 0 0 1 0-3.5l1.7-1.7a2.5 2.5 0 0 1 3.5 3.5l-.8.8'), path('M9.4 6.6a2.5 2.5 0 0 1 0 3.5l-1.7 1.7a2.5 2.5 0 0 1-3.5-3.5l.8-.8')],
  system: [circle(8, 8, 2.2), path('M8 1.8v2M8 12.2v2M1.8 8h2M12.2 8h2M3.6 3.6 5 5M11 11l1.4 1.4M12.4 3.6 11 5M5 11l-1.4 1.4')],
  schedule: [circle(8, 8, 5.8), path('M8 4.8V8l2.2 1.4')],

  // ── The panel's views ──
  detail: [rect(2, 2.5, 12, 11, 1.5), path('M2 9.4h12')],
  stats: [path('M2 13.5V2.8M2 13.5h12'), path('M4.4 10.4 7 7l2.2 2L13 4.4')],
  input: [rect(1.4, 4.4, 13.2, 8.2, 1.6), dot(4.4, 7.4), dot(7, 7.4), dot(9.6, 7.4), dot(12, 7.4), path('M5 10.4h6')],
  console: [path('M3 4.6 7 8l-4 3.4M8.6 12.2h4.4')],
  types: [path('M6 2.5C4.5 2.5 4.5 3.6 4.5 5v1.2c0 .8-.6 1.3-1.5 1.8.9.5 1.5 1 1.5 1.8V11c0 1.4 0 2.5 1.5 2.5'), path('M10 2.5c1.5 0 1.5 1.1 1.5 2.5v1.2c0 .8.6 1.3 1.5 1.8-.9.5-1.5 1-1.5 1.8V11c0 1.4 0 2.5-1.5 2.5')],
} as const satisfies Record<string, readonly Prim[]>;

export type BrpIconName = keyof typeof ICONS;
