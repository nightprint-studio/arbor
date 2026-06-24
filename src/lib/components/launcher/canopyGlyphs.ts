/**
 * Bird glyphs for the Arbor Canopy launcher — one tiny line-art mark per
 * product, drawn on a 24×24 grid in `currentColor` so the node accent tints
 * them. Ported verbatim from the `Arbor Canopy` design prototype; each mark is
 * behaviour-based (a raven over a git graph for Corvus, a woodpecker by DB
 * holes for Picus, …). Unknown ids fall back to `_default` (a generic perched
 * bird). Rendered by `CanopyGlyph.svelte`.
 */

export type GlyphTag = 'path' | 'circle' | 'ellipse' | 'line';

export interface GlyphPart {
  tag: GlyphTag;
  attrs: Record<string, string | number>;
}

// Stroke defaults shared by every part; filled parts override fill+stroke.
const STROKE = { fill: 'none', stroke: 'currentColor', 'stroke-width': 1.7, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' } as const;
const FILL = { fill: 'currentColor', stroke: 'none' } as const;

const s = (tag: GlyphTag, attrs: Record<string, string | number>): GlyphPart => ({ tag, attrs: { ...STROKE, ...attrs } });
const f = (tag: GlyphTag, attrs: Record<string, string | number>): GlyphPart => ({ tag, attrs: { ...STROKE, ...FILL, ...attrs } });

export const GLYPHS: Record<string, GlyphPart[]> = {
  // raven + git graph
  corvus: [
    s('line', { x1: 6, y1: 6, x2: 6, y2: 18 }),
    f('circle', { cx: 6, cy: 6.5, r: 1.4 }),
    s('path', { d: 'M6 12 C 9.5 12 10.5 8 14 8' }),
    f('circle', { cx: 14, cy: 8, r: 1.4 }),
    f('path', { d: 'M11.5 16.4 q3.4 -1 5.6 .6 l2.4 -1 -1.3 1.9 q-2.9 2 -6.3 .3 z' }),
  ],
  // blackbird singing
  merula: [
    f('ellipse', { cx: 8, cy: 14.5, rx: 4.3, ry: 3 }),
    f('circle', { cx: 5.4, cy: 11, r: 2.1 }),
    f('path', { d: 'M3.6 10 l-2.4 -.7 2.2 -1.1 z' }),
    s('path', { d: 'M11 16.5 l4 4' }),
    s('path', { d: 'M13.8 9.5 q2.4 2.6 0 5' }),
    s('path', { d: 'M16.8 8 q4 4.5 0 9' }),
  ],
  // woodpecker clinging + DB holes
  picus: [
    s('line', { x1: 6, y1: 4, x2: 6, y2: 20 }),
    s('path', { d: 'M6 8.5 q3.2 -1 4.6 1.2 q-1.4 3 -4.6 2.2' }),
    s('path', { d: 'M6 8.5 l-2.6 -1.4' }),
    s('path', { d: 'M9.8 12 l1.4 3' }),
    f('circle', { cx: 15, cy: 7.5, r: 1.15 }),
    f('circle', { cx: 15, cy: 12, r: 1.15 }),
    f('circle', { cx: 15, cy: 16.5, r: 1.15 }),
  ],
  // nuthatch head-down on trunk
  sitta: [
    s('line', { x1: 8, y1: 4, x2: 8, y2: 20 }),
    s('path', { d: 'M8 7.5 q4.4 1 4.8 5 q-1.4 2.4 -4.8 1.6' }),
    s('path', { d: 'M12.6 17.4 l1.6 2.8' }),
    s('path', { d: 'M12.6 12.4 l1.6 2.6' }),
    f('circle', { cx: 10.4, cy: 9.6, r: 0.85 }),
  ],
  // swift sickle wings
  apus: [
    s('path', { d: 'M3 7.5 C 8.5 11.5 11 11.5 12 15 C 13 11.5 15.5 11.5 21 7.5' }),
    s('path', { d: 'M12 15 l0 3' }),
  ],
  // magpie carrying a gem
  pica: [
    f('circle', { cx: 9, cy: 10, r: 2 }),
    f('ellipse', { cx: 8.6, cy: 14, rx: 3, ry: 2.4 }),
    s('path', { d: 'M6.6 15.4 l-4.2 4.4' }),
    s('path', { d: 'M10.8 9.2 l3 -1' }),
    f('path', { d: 'M16.4 5.6 l2.4 2.4 -2.4 2.4 -2.4 -2.4 z' }),
  ],
  // owl face watching (observability)
  strix: [
    s('path', { d: 'M5 11 q0 -5 7 -5 q7 0 7 5 q0 7 -7 7 q-7 0 -7 -7 z' }),
    s('circle', { cx: 9.3, cy: 11.5, r: 2.3 }),
    s('circle', { cx: 14.7, cy: 11.5, r: 2.3 }),
    f('circle', { cx: 9.3, cy: 11.5, r: 0.7 }),
    f('circle', { cx: 14.7, cy: 11.5, r: 0.7 }),
    s('path', { d: 'M11 15 l1 1.5 1 -1.5' }),
    s('path', { d: 'M6.5 7 l-1.2 -3' }),
    s('path', { d: 'M17.5 7 l1.2 -3' }),
  ],
  // generic perched bird
  _default: [
    s('path', { d: 'M4 17 h16' }),
    s('circle', { cx: 11, cy: 10, r: 2.6 }),
    s('path', { d: 'M13.4 9 l3.2 -1.1' }),
    s('path', { d: 'M10 12.4 l-.6 4.2' }),
    s('path', { d: 'M12 12.4 l.4 4.2' }),
  ],
};

export function glyphParts(id: string): GlyphPart[] {
  return GLYPHS[id] ?? GLYPHS._default;
}
