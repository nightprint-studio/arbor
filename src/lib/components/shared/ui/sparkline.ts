/** The geometry of a sparkline, kept apart from the widget so it can be tested without a DOM. */

export interface SparkGeometry {
  /** `x,y x,y …` for an SVG `<polyline points>`. Empty for fewer than two values. */
  points: string;
  /** The value the top edge stands for. */
  max: number;
  /** Y of the `reference` line, when one was asked for and it is inside the range. */
  referenceY: number | null;
}

/**
 * Lay `values` out in a `width × height` box, left to right, 0 at the bottom. The scale is
 * `0..max(values, reference)` so two graphs of the same thing are comparable at a glance; `padding`
 * keeps the line off the border.
 */
export function sparkGeometry(values: readonly number[], width: number, height: number, reference?: number, padding = 1): SparkGeometry {
  const finite = values.filter((v) => Number.isFinite(v));
  const max = Math.max(1e-9, ...finite, reference ?? 0);
  const y = (v: number) => padding + (height - 2 * padding) * (1 - v / max);
  const referenceY = reference !== undefined && reference <= max ? y(reference) : null;
  if (values.length < 2) return { points: '', max, referenceY };
  const step = (width - 2 * padding) / (values.length - 1);
  const points = values.map((v, i) => `${(padding + i * step).toFixed(1)},${y(Number.isFinite(v) ? v : 0).toFixed(1)}`).join(' ');
  return { points, max, referenceY };
}
