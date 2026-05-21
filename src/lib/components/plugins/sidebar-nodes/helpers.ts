/*
 * Pure helpers shared by PluginSidebarPanel + its sub-renderers. No
 * Svelte / DOM / store dependencies — purely transforms.
 */

/** Build a unique key for `{#each}` regardless of duplicate types/ids.
 *  Appending the index guarantees uniqueness even when a plugin has
 *  e.g. multiple `divider` nodes in a row. */
export function nodeType(n: unknown): string {
  return (n as any)?.type ?? 'unknown';
}

export function nodeKey(n: unknown, i: number): string {
  return `${nodeType(n)}:${(n as any)?.id ?? (n as any)?.name ?? ''}:${i}`;
}

export function fieldKey(n: any, index: number): string {
  return (n.id ?? n.name ?? `field-${index}`);
}

/** Parse `#rrggbb` (or `#rgb`) into normalised {r,g,b} floats in [0,1].
 *  Returns null on malformed input. */
export function parseHexColor(hex: string): { r: number; g: number; b: number } | null {
  if (typeof hex !== 'string') return null;
  let h = hex.trim();
  if (h.startsWith('#')) h = h.slice(1);
  if (h.length === 3) h = h.split('').map(c => c + c).join('');
  if (!/^[0-9a-fA-F]{6}$/.test(h)) return null;
  const r = parseInt(h.slice(0, 2), 16) / 255;
  const g = parseInt(h.slice(2, 4), 16) / 255;
  const b = parseInt(h.slice(4, 6), 16) / 255;
  return { r, g, b };
}

/** Pack normalised {r,g,b} floats back to `#rrggbb`. Clamps to [0,1]. */
export function packHexColor(r: number, g: number, b: number): string {
  const clamp = (x: number) => Math.max(0, Math.min(1, Number.isFinite(x) ? x : 0));
  const c = (x: number) => Math.round(clamp(x) * 255).toString(16).padStart(2, '0');
  return '#' + c(r) + c(g) + c(b);
}

/** Convert a plugin-supplied `points: [{ts,value}|[ts,value]]` series into
 *  the {x, y} shape LineChart consumes. */
export function pointsToXY(raw: unknown): Array<{ x: number; y: number }> {
  if (!Array.isArray(raw)) return [];
  const out: Array<{ x: number; y: number }> = [];
  for (const p of raw) {
    let x: number | null = null;
    let y: number | null = null;
    if (Array.isArray(p) && p.length >= 2) {
      x = Number(p[0]); y = Number(p[1]);
    } else if (p && typeof p === 'object') {
      const o = p as Record<string, unknown>;
      x = Number(o.ts ?? o.x ?? o.t);
      y = Number(o.value ?? o.y ?? o.v);
    }
    if (x != null && y != null && Number.isFinite(x) && Number.isFinite(y)) {
      out.push({ x, y });
    }
  }
  return out;
}

export function sparklinePath(points: Array<{ x: number; y: number }>, w: number, h: number): string {
  if (points.length < 1) return '';
  let xMin = Infinity, xMax = -Infinity, yMin = Infinity, yMax = -Infinity;
  for (const p of points) {
    if (p.x < xMin) xMin = p.x; if (p.x > xMax) xMax = p.x;
    if (p.y < yMin) yMin = p.y; if (p.y > yMax) yMax = p.y;
  }
  if (xMin === xMax) { xMin -= 1; xMax += 1; }
  if (yMin === yMax) { yMin -= 1; yMax += 1; }
  const px = 2, py = 2;
  const sx = (v: number) => px + ((v - xMin) / (xMax - xMin)) * (w - px * 2);
  const sy = (v: number) => h - py - ((v - yMin) / (yMax - yMin)) * (h - py * 2);
  let d = '';
  for (let i = 0; i < points.length; i++) {
    const p = points[i];
    d += (i === 0 ? 'M' : 'L') + sx(p.x).toFixed(1) + ' ' + sy(p.y).toFixed(1) + ' ';
  }
  return d.trimEnd();
}
