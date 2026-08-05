/**
 * The mark for each product of the suite — its initial, on a 24×24 grid, in `currentColor`.
 *
 * ## Why letters
 *
 * These used to be pictures: a lucide glyph per product in the tab strip (a branch, a coffee
 * cup, a musical note), and a hand-drawn bird per product in the launcher. Both failed at the
 * size they actually run. A tab gives its icon **16 pixels**, and at 16px a silhouette has
 * already lost every detail that told one bird from another — a beak is just a beak, and a
 * coffee cup says "Java" only to someone who already knows. A letter loses nothing at 16px,
 * and seven letters cannot be confused with each other by construction.
 *
 * ## How they are built
 *
 * Circular arcs of an exact radius plus straight stems, all at one stroke width per weight —
 * never freehand, and never a `<text>` element. Freehand matters because the eye compares a
 * letter's two bowls against each other and any inequality shows instantly; `<text>` matters
 * because a font that is missing wherever the mark is rasterised is a glyph nobody chose.
 *
 * The three weights are three weights of the *same* drawing, not three drawings. Note that
 * the heavier weights open their bowls slightly (larger radii, shorter crossbars): a stroke
 * that thickens without the counter growing to meet it fills the counter in, and a B whose
 * counters have closed is a blob.
 *
 * Pure data + one accessor. The rendering is `shared/internal/ProductMark.svelte`.
 */

// The roster and the ids come from `products.ts` — this module owns the geometry and nothing
// else. Re-exported so a consumer that only wants a mark needs one import.
import { isProductId, type ProductId } from './products';
export { isProductId, type ProductId };

/**
 * How heavily to draw it.
 * - `line` — sits among the lucide icons already in the chrome without shouting.
 * - `solid` — the same letter at nearly twice the stroke, for where a product mark should
 *   read as more than a toolbar action.
 * - `duotone` — the letter at an intermediate weight plus the product's own context as a
 *   ghost, placed **clear of the strokes** rather than behind them: a ghost drawn under a
 *   letter at 16px is a smudge on it.
 */
export type MarkWeight = 'line' | 'solid' | 'duotone';

export type MarkTag = 'path' | 'circle' | 'rect';

/** One SVG element of a mark, as tag + attributes (spread straight onto the element). */
export interface MarkPart {
  tag: MarkTag;
  attrs: Record<string, string | number>;
}

/** How faint the duotone ghost is. One value, so the second layer reads the same everywhere. */
const GHOST = 0.3;

const strokeAttrs = (w: number) => ({
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': w,
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
});

/** A stroked path at width `w`. */
const p = (d: string, w: number): MarkPart => ({ tag: 'path', attrs: { ...strokeAttrs(w), d } });
/** A filled dot. */
const dot = (cx: number, cy: number, r: number): MarkPart => ({
  tag: 'circle',
  attrs: { cx, cy, r, fill: 'currentColor' },
});
/** A filled rounded bar. */
const bar = (x: number, y: number, w: number, h: number, rx: number): MarkPart => ({
  tag: 'rect',
  attrs: { x, y, width: w, height: h, rx, fill: 'currentColor' },
});
/** A stroked circle. */
const ring = (cx: number, cy: number, r: number, w: number): MarkPart => ({
  tag: 'circle',
  attrs: { cx, cy, r, fill: 'none', stroke: 'currentColor', 'stroke-width': w },
});
/** Push a part into the second layer. */
const faint = (part: MarkPart): MarkPart => ({ ...part, attrs: { ...part.attrs, opacity: GHOST } });

const LINE = 2.2;
const SOLID = 4;
const DUO = 3.4;

/**
 * The marks. Each entry is the letter at each weight, and the duotone entry appends the
 * product's own context as a ghost:
 *
 * | | the ghost is |
 * |---|---|
 * | Corvus | the constellation its C is drawn from |
 * | Merula | the step cells its M is lit on |
 * | Sitta | tree elbows |
 * | Bennu | the cursor at the end of the line |
 * | Picus | the table strata its stem is bored through |
 * | Tyto | the record ring |
 * | Garrulus | the note's ruled lines |
 */
const MARKS: Record<ProductId, Record<MarkWeight, MarkPart[]>> = {
  // C — one arc of 260°, leaving the gap on the right.
  corvus: {
    line: [p('M16.5 6.6 A7 7 0 1 0 16.5 17.4', LINE)],
    solid: [p('M16.5 6.6 A7 7 0 1 0 16.5 17.4', SOLID)],
    duotone: [
      faint(dot(20, 4.5, 1.5)),
      faint(dot(21.5, 12, 1.1)),
      faint(dot(20, 19.5, 1.5)),
      p('M16.5 6.6 A7 7 0 1 0 16.5 17.4', DUO),
    ],
  },
  // M — four straight runs; the apexes stop below the cap line so the diagonals have room.
  merula: {
    line: [p('M5 20 V4.5 L12 14 L19 4.5 V20', LINE)],
    solid: [p('M5.2 19.8 V5 L12 13.6 L18.8 5 V19.8', SOLID)],
    duotone: [
      faint(bar(2.2, 2.6, 2.6, 2.6, 0.8)),
      faint(bar(19.2, 2.6, 2.6, 2.6, 0.8)),
      faint(bar(2.2, 18.8, 2.6, 2.6, 0.8)),
      faint(bar(19.2, 18.8, 2.6, 2.6, 0.8)),
      faint(bar(10.7, 20.4, 2.6, 2.6, 0.8)),
      p('M5.6 19.4 V5.4 L12 13.4 L18.4 5.4 V19.4', DUO),
    ],
  },
  // S — two arcs of one radius meeting exactly at the centre.
  sitta: {
    line: [p('M15.8 6.6 A4 4 0 1 0 12 12 A4 4 0 1 1 8.2 17.4', LINE)],
    solid: [p('M15.9 6.8 A4.2 4.2 0 1 0 12 12 A4.2 4.2 0 1 1 8.1 17.2', SOLID)],
    duotone: [
      faint(p('M2.4 3.4 V8.4 H5.4', 1.6)),
      faint(p('M21.6 15.6 V20.6 H18.6', 1.6)),
      p('M15.9 6.8 A4.2 4.2 0 1 0 12 12 A4.2 4.2 0 1 1 8.1 17.2', DUO),
    ],
  },
  // B — lower bowl larger than the upper (split at 11.5, not 12), or it looks top-heavy.
  bennu: {
    line: [
      p('M8 4 V20', LINE),
      p('M8 4 H11.5 A3.75 3.75 0 0 1 11.5 11.5 H8', LINE),
      p('M8 11.5 H12 A4.25 4.25 0 0 1 12 20 H8', LINE),
    ],
    solid: [
      p('M7.6 4.4 V19.6', SOLID),
      p('M7.6 4.4 H11.4 A3.6 3.6 0 0 1 11.4 11.6 H7.6', SOLID),
      p('M7.6 11.6 H12 A4 4 0 0 1 12 19.6 H7.6', SOLID),
    ],
    duotone: [
      faint(bar(15.4, 17.6, 6.4, 2.4, 0.7)),
      p('M7.4 4.6 V19.4', 3.2),
      p('M7.4 4.6 H11 A3.4 3.4 0 0 1 11 11.4 H7.4', 3.2),
      p('M7.4 11.4 H11.6 A4 4 0 0 1 11.6 19.4 H7.4', 3.2),
    ],
  },
  // P — the B's construction, minus the lower bowl.
  picus: {
    line: [p('M8 4 V20', LINE), p('M8 4 H12 A4.25 4.25 0 0 1 12 12.5 H8', LINE)],
    solid: [p('M7.6 4.4 V19.6', SOLID), p('M7.6 4.4 H12 A4.2 4.2 0 0 1 12 12.8 H7.6', SOLID)],
    duotone: [
      // Kept to the RIGHT of the stem, so the bands read as layers the letter stands in
      // rather than as a strikethrough.
      faint(bar(14.4, 14.4, 7.4, 2.2, 1.1)),
      faint(bar(14.4, 18.4, 7.4, 2.2, 1.1)),
      p('M7.4 4.6 V19.4', DUO),
      p('M7.4 4.6 H11.6 A4 4 0 0 1 11.6 12.6 H7.4', DUO),
    ],
  },
  // T
  tyto: {
    line: [p('M5 5.5 H19', LINE), p('M12 5.5 V20', LINE)],
    solid: [p('M5.2 6 H18.8', SOLID), p('M12 6 V19.6', SOLID)],
    duotone: [
      // A full ring rather than a fragment: recording is a state the whole tab is in.
      faint(ring(12, 12, 10, 1.8)),
      p('M6.4 7.4 H17.6', DUO),
      p('M12 7.4 V18.4', DUO),
    ],
  },
  // G — the C's arc carried 60° further, closing into a bar at mid-height. The bar shortens
  // as the stroke thickens, or the counter it exists to leave open fills in.
  garrulus: {
    line: [p('M16.5 6.6 A7 7 0 1 0 19 12 H14.5', LINE)],
    solid: [p('M16.5 6.6 A7 7 0 1 0 19 12 H15.2', SOLID)],
    duotone: [
      faint(bar(15.6, 2.4, 6.2, 2, 1)),
      faint(bar(15.6, 19.6, 6.2, 2, 1)),
      p('M16.5 6.6 A7 7 0 1 0 19 12 H15.2', DUO),
    ],
  },
};

/**
 * The parts of a product's mark. An unknown id yields an empty array rather than a
 * placeholder: a mark that stands for nothing is worse than no mark, and the caller that
 * needs a fallback (the tab strip's `home`, which is not a product) already has one.
 */
export function markParts(id: string, weight: MarkWeight = 'line'): MarkPart[] {
  return isProductId(id) ? MARKS[id][weight] : [];
}

/** The letter a product's mark draws — for a text-only fallback, or an aria label. */
export const PRODUCT_LETTER: Record<ProductId, string> = {
  corvus: 'C',
  merula: 'M',
  sitta: 'S',
  bennu: 'B',
  picus: 'P',
  tyto: 'T',
  garrulus: 'G',
};
