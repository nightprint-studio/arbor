// Pure helpers shared by `<GaugeChart>`, `<LineChart>` and any future
// SVG-based chart widget under `shared/charts/`. These are intentionally
// dependency-free and domain-agnostic — the security dashboard is the
// first consumer, but anything else (CI metrics, repo activity, plugin
// telemetry) should be able to lift these straight off the shelf.

/** A 2-D point in chart space. `x` may be a `number` (linear) or `Date` (time). */
export interface XYPoint { x: number | Date; y: number }

/** Convert any X value to a numeric coordinate (Date → ms since epoch). */
export function xToNumber(x: number | Date): number {
  return x instanceof Date ? x.getTime() : x;
}

/**
 * Build a "nice" set of tick values inside `[min, max]` whose step is a
 * 1/2/5 multiple of a power of ten. Returns at most `target+1` ticks.
 */
export function niceTicks(min: number, max: number, target = 5): number[] {
  if (!isFinite(min) || !isFinite(max)) return [];
  if (max - min === 0) return [min];
  const span = max - min;
  const rawStep = span / Math.max(1, target);
  const mag = Math.pow(10, Math.floor(Math.log10(rawStep)));
  const norm = rawStep / mag;
  let step: number;
  if (norm < 1.5)      step = 1   * mag;
  else if (norm < 3)   step = 2   * mag;
  else if (norm < 7)   step = 5   * mag;
  else                 step = 10  * mag;

  const start = Math.ceil(min / step) * step;
  const ticks: number[] = [];
  for (let v = start; v <= max + 1e-9; v += step) {
    // Clamp tiny float drift to multiples of `step`.
    ticks.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toFixed(10)));
  }
  return ticks;
}

/** Linear scale: maps a value from `[domainMin, domainMax]` to `[rangeMin, rangeMax]`. */
export function scaleLinear(
  domainMin: number, domainMax: number,
  rangeMin: number,  rangeMax: number,
): (v: number) => number {
  const dSpan = domainMax - domainMin;
  if (dSpan === 0) return () => (rangeMin + rangeMax) / 2;
  const rSpan = rangeMax - rangeMin;
  return (v: number) => rangeMin + ((v - domainMin) / dSpan) * rSpan;
}

/**
 * Format a numeric ms-since-epoch tick as a short axis label. Picks a
 * sensible granularity given the total range of the data:
 *   - ≤ 2 days  →  "HH:mm"
 *   - ≤ 90 days →  "MMM dd"
 *   - else      →  "MMM yy"
 */
export function formatTimeAxis(rangeMs: number): (v: number) => string {
  const TWO_DAYS = 2 * 24 * 3600 * 1000;
  const NINETY   = 90 * 24 * 3600 * 1000;
  if (rangeMs <= TWO_DAYS) {
    return (v) => {
      const d = new Date(v);
      return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
    };
  }
  if (rangeMs <= NINETY) {
    return (v) => {
      const d = new Date(v);
      return `${MONTHS[d.getMonth()]} ${pad2(d.getDate())}`;
    };
  }
  return (v) => {
    const d = new Date(v);
    return `${MONTHS[d.getMonth()]} ${String(d.getFullYear()).slice(-2)}`;
  };
}

/**
 * Build an SVG path `d` attribute from a series of pixel-space points.
 * Skips non-finite values (yields a discontinuous line, like d3).
 */
export function pathFromPoints(points: { x: number, y: number }[]): string {
  let d = '';
  let pen = false;
  for (const p of points) {
    if (!isFinite(p.x) || !isFinite(p.y)) { pen = false; continue; }
    if (!pen) { d += `M${round(p.x)},${round(p.y)}`; pen = true; }
    else      { d += `L${round(p.x)},${round(p.y)}`; }
  }
  return d;
}

/** Build an SVG arc path between two angles on a centred circle. */
export function arcPath(
  cx: number, cy: number, radius: number,
  startRad: number, endRad: number,
): string {
  const x0 = cx + Math.cos(startRad) * radius;
  const y0 = cy + Math.sin(startRad) * radius;
  const x1 = cx + Math.cos(endRad)   * radius;
  const y1 = cy + Math.sin(endRad)   * radius;
  const largeArc = Math.abs(endRad - startRad) > Math.PI ? 1 : 0;
  const sweep    = endRad > startRad ? 1 : 0;
  return `M${round(x0)},${round(y0)} A${radius},${radius} 0 ${largeArc} ${sweep} ${round(x1)},${round(y1)}`;
}

/* ---------- helpers ---------- */
const MONTHS = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
function pad2(n: number): string { return n < 10 ? `0${n}` : String(n); }
function round(n: number): number { return Math.round(n * 100) / 100; }
