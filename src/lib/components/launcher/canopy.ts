/**
 * Arbor Canopy launcher — model + circuit-tree geometry.
 *
 * The Canopy is a spatial launcher: the Nightprint suite is drawn as a
 * circuit-tree (one branch per product, fanning into a rounded canopy on a
 * hill), and every product is a status-lit node. This module is the pure,
 * framework-agnostic engine ported from the `Arbor Canopy` design prototype:
 * the product roster, colour math, starfield, branch geometry, the per-product
 * `decorate()` (status → labels/action), and `buildScene()` (geometry +
 * decorated tools → render data). `LauncherShell` / `CanopyTree` own the
 * Svelte rendering and interaction; this file has no Svelte/Tauri imports.
 */

// Runtime status — derived per render from real signals (window open? version
// behind?), not stored on the product. `installed` here means "installed and
// up-to-date".
export type ToolStatus = 'installed' | 'running' | 'update';

/** Static identity of a Canopy product (the dynamic bits — running, versions —
 *  come from the launcher at runtime; see `ToolRuntime` / `decorate`). */
export interface Product {
  id: string;
  name: string;
  bird: string;
  role: string;
  accent: string;
}

// ── Palette ──────────────────────────────────────────────────────────────────
export const GREEN = '#8fce6a';   // brand / installed
export const RUN = '#6ee7b7';     // running
export const TEAL = '#45c4b0';    // trunk trace base
export const CANOPY_W = 560;
export const CANOPY_H = 600;

// ── Product roster (bird-named Nightprint suite) ─────────────────────────────
// The three products that actually exist today, each mapping to a real Arbor
// window (`PRODUCT_WINDOW_OPENERS` in `ipc/app.ts`). Identity only — version and
// running state are resolved at runtime (see `versions.ts` + `decorate`).
export const BASE: Product[] = [
  { id: 'corvus', name: 'Corvus', bird: 'crow',        role: 'Git & CI client',     accent: '#7c9cf5' },
  { id: 'merula', name: 'Merula', bird: 'blackbird',   role: 'Music synthesizer',   accent: '#e8a857' },
  { id: 'sitta',  name: 'Sitta',  bird: 'treecreeper', role: 'File explorer',       accent: '#b58cf0' },
  { id: 'tyto',   name: 'Tyto',   bird: 'barn owl',    role: 'Screen recorder',     accent: '#f28b82' },
];

// ── Colour helpers ───────────────────────────────────────────────────────────
export function hexA(h: string, a: number): string {
  const n = parseInt(h.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}
/** Linear interpolation between two #rrggbb colours. */
export function lerpColor(a: string, b: string, t: number): string {
  const A = parseInt(a.slice(1), 16), B = parseInt(b.slice(1), 16);
  const r = Math.round(((A >> 16 & 255) * (1 - t) + (B >> 16 & 255) * t));
  const g = Math.round(((A >> 8 & 255) * (1 - t) + (B >> 8 & 255) * t));
  const bl = Math.round(((A & 255) * (1 - t) + (B & 255) * t));
  return `rgb(${r},${g},${bl})`;
}
function makeRng(seed: number): () => number {
  let s = seed >>> 0;
  return () => { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; };
}

// ── Starfield (the starry-night atmosphere) ──────────────────────────────────
export interface Star { x: number; y: number; c: string; big: boolean; }
export function genStars(count: number): Star[] {
  const arr: Star[] = []; let seed = 8123;
  const rnd = () => { seed = (seed * 1664525 + 1013904223) % 4294967296; return seed / 4294967296; };
  const tints = ['rgba(124,156,245,.8)', 'rgba(69,196,176,.75)', 'rgba(181,140,240,.8)', 'rgba(143,206,106,.7)', 'rgba(232,168,87,.75)'];
  for (let i = 0; i < count; i++) {
    arr.push({
      x: Math.floor(rnd() * 620),
      y: Math.floor(rnd() * 520),
      c: i % 9 < 6 ? 'rgba(206,215,231,' + (0.35 + rnd() * 0.5).toFixed(2) + ')' : tints[Math.floor(rnd() * 5)],
      big: rnd() > 0.85,
    });
  }
  return arr;
}
export function starShadow(stars: Star[]): string {
  return stars.map(s => `${s.x}px ${s.y}px 0 ${s.big ? '0.7px' : '0px'} ${s.c}`).join(',');
}

// ── Decorated tool (runtime status → labels / action / colours) ──────────────
export type ActionKind = 'primary' | 'run' | 'update';

/** Runtime state of a product, resolved by the launcher each render. */
export interface ToolRuntime {
  /** A product window is currently open. */
  running: boolean;
  /** Installed version (the running binary's version). */
  installed: string;
  /** Latest available version (== installed when up-to-date). */
  latest: string;
}

export interface DecoratedTool extends Product {
  status: ToolStatus;
  version: string;
  versionLabel: string;
  actionLabel: string;
  kind: ActionKind;
  statusLabel: string;
  statusColor: string;
  isRunning: boolean;
  isUpd: boolean;
  isRun: boolean;
  verMenu: { v: string; active: boolean }[];
  glyphId: string;
}

/** Compare two dotted version strings; true when `a` is strictly newer than `b`. */
export function isNewer(a: string, b: string): boolean {
  const pa = a.split('.').map(Number), pb = b.split('.').map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] || 0, y = pb[i] || 0;
    if (x !== y) return x > y;
  }
  return false;
}

export function decorate(t: Product, rt: ToolRuntime, green = GREEN): DecoratedTool {
  const A = t.accent;
  const hasUpdate = isNewer(rt.latest, rt.installed);
  const status: ToolStatus = rt.running ? 'running' : (hasUpdate ? 'update' : 'installed');

  let actionLabel: string, kind: ActionKind;
  if (status === 'running') { actionLabel = 'Open'; kind = 'run'; }
  else if (status === 'update') { actionLabel = 'Update'; kind = 'update'; }
  else { actionLabel = 'Launch'; kind = 'primary'; }

  const scMap: Record<ToolStatus, [string, string]> = {
    installed: ['Up to date', green],
    running: ['Running', RUN],
    update: ['Update ' + rt.latest, A],
  };
  const sc = scMap[status];

  // Versions on offer: today just the installed one (plus the newer one when an
  // update is available). The active entry is what's installed.
  const versions = hasUpdate ? [rt.latest, rt.installed] : [rt.installed];

  return {
    ...t,
    status,
    version: rt.installed,
    versionLabel: 'v' + rt.installed,
    actionLabel, kind,
    statusLabel: sc[0], statusColor: sc[1],
    isRunning: status === 'running',
    isUpd: status === 'update',
    isRun: status === 'running',
    verMenu: versions.map(vv => ({ v: vv, active: vv === rt.installed })),
    glyphId: t.id, // CanopyGlyph falls back to the generic mark for unknown ids
  };
}

// ── Circuit-tree geometry (one clean branch per product) ─────────────────────
export interface Seg { d: string; w: number; depth: number; ex: number; ey: number; }
export interface Tip { x: number; y: number; ang: number; seg: number; idx: number; }
export interface Twig { x: number; y: number; br: number; }
export interface Geometry {
  segments: Seg[];
  joints: { x: number; y: number }[];
  toolTips: Tip[];
  twigs: Twig[];
  baseY: number; cx: number; hy: number; forkY: number;
}

const geoCache: Record<string, Geometry> = {};

export function geometry(scale: 'few' | 'many', n: number): Geometry {
  const key = `${scale}:${n}`;
  if (geoCache[key]) return geoCache[key];
  const W = CANOPY_W, H = CANOPY_H, cx = W / 2;
  // hy = hill crest line, pushed far down (0.95) so the hill is a thin grassy
  // mound. forkY at 0.66 keeps the bare trunk short; the canopy reaches up via
  // long branches + foliage (the tree SVG is top-anchored — see CanopyTree).
  const hy = Math.round(H * 0.95), baseY = hy + 8, forkY = Math.round(H * 0.66);
  const many = scale === 'many';
  const rand = makeRng(many ? 7 : 3);
  const segments: Seg[] = [], joints = [{ x: cx, y: forkY }], toolTips: Tip[] = [], twigs: Twig[] = [];
  const seg = (x1: number, y1: number, cxp: number, cyp: number, x2: number, y2: number, w: number, depth: number): Seg =>
    ({ d: `M${x1.toFixed(1)} ${y1.toFixed(1)} Q ${cxp.toFixed(1)} ${cyp.toFixed(1)} ${x2.toFixed(1)} ${y2.toFixed(1)}`, w, depth, ex: x2, ey: y2 });
  // trunk (slight wave) from hill up to the fork
  segments.push(seg(cx, baseY, cx - 2, (baseY + forkY) / 2, cx, forkY, 6, 0));
  // one clean branch per tool, fanning into a rounded canopy
  const half = many ? 1.5 : 1.24;
  const Lmax = many ? H * 0.31 : H * 0.50;
  for (let i = 0; i < n; i++) {
    const frac = n === 1 ? 0.5 : i / (n - 1);
    const ang = -half + 2 * half * frac;
    const tier = many ? (i % 2 ? 0.82 : 1.0) : 1.0;
    const L = Lmax * (0.74 + 0.26 * Math.cos(ang)) * tier * (0.97 + rand() * 0.05);
    const ex = cx + Math.sin(ang) * L, ey = forkY - Math.cos(ang) * L;
    const ctrlx = cx + Math.sin(ang) * L * 0.30, ctrly = forkY - Math.cos(ang) * L * 0.64;
    segments.push(seg(cx, forkY, ctrlx, ctrly, ex, ey, 3.4, 1));
    toolTips.push({ x: ex, y: ey, ang, seg: segments.length - 1, idx: i });
    [0.52, 0.8].forEach(tt => {
      const mt = 1 - tt;
      twigs.push({ x: mt * mt * cx + 2 * mt * tt * ctrlx + tt * tt * ex, y: mt * mt * forkY + 2 * mt * tt * ctrly + tt * tt * ey, br: i });
    });
  }
  // roots fanning into the hill
  ([[-58, 16], [-30, 22], [30, 22], [58, 16]] as const).forEach(r => {
    const ex = cx + r[0], ey = baseY + r[1];
    segments.push(seg(cx, baseY, cx + r[0] * 0.5, baseY + r[1] * 0.6, ex, ey, 2.6, 0));
  });
  const geo: Geometry = { segments, joints, toolTips, twigs, baseY, cx, hy, forkY };
  geoCache[key] = geo;
  return geo;
}

// ── Foliage (leaf clusters that give the canopy mass) ────────────────────────
export interface LeafEl { d: string; fill: string; opacity: number; transform: string; }
const LEAF_GREENS = ['#3f7a3a', '#4f9a45', '#5fb35a', '#356b34', '#2e5c2e', '#62b657'];

/** Deterministic hash-jitter in [0, b). */
function jit(i: number, seed: number, b: number): number {
  const r = Math.abs(Math.sin(i * 12.9898 + seed) * 43758.5453) % 1;
  return r * (b || 1);
}
function leaf(lx: number, ly: number, ang: number, size: number, fill: string, opacity: number): LeafEl {
  const d = `M0 0 C ${(size * 0.32).toFixed(1)} ${(-size * 0.46).toFixed(1)} ${(size * 0.82).toFixed(1)} ${(-size * 0.22).toFixed(1)} ${size.toFixed(1)} 0 C ${(size * 0.82).toFixed(1)} ${(size * 0.22).toFixed(1)} ${(size * 0.32).toFixed(1)} ${(size * 0.46).toFixed(1)} 0 0 Z`;
  return { d, fill, opacity, transform: `translate(${lx.toFixed(1)} ${ly.toFixed(1)}) rotate(${ang.toFixed(1)})` };
}
/** A fan of `n` leaves scattered around (x,y), oriented along `ang` (degrees). */
function leafCluster(x: number, y: number, ang: number, n: number, sizeBase: number, opacity: number, seed: number): LeafEl[] {
  const out: LeafEl[] = [];
  for (let i = 0; i < n; i++) {
    const a = ang - 70 + (140 * (i / (n - 1 || 1))) + jit(i, seed + i, 30) - 15;
    const dist = jit(i, i * 3 + 1 + seed, 6) + 2;
    const lx = x + Math.cos((a - 90) * Math.PI / 180) * dist;
    const ly = y + Math.sin((a - 90) * Math.PI / 180) * dist;
    const sz = sizeBase * (0.7 + jit(i, i + 7 + seed, 0.6));
    const fill = LEAF_GREENS[Math.floor(jit(i, i * 2 + 5 + seed, LEAF_GREENS.length - 0.01))];
    out.push(leaf(lx, ly, a, sz, fill, opacity));
  }
  return out;
}

// ── Scene assembly (pure render data for CanopyTree) ─────────────────────────
export interface TraceEl { d: string; stroke: string; width: number; opacity: number; dash: string; }
export interface DotEl { cx: number; cy: number; r: number; fill: string; stroke?: string; strokeWidth?: number; opacity: number; }
export interface NodeEl {
  id: string; x: number; y: number; accent: string; r: number;
  sel: boolean; op: number; isUpd: boolean; isRun: boolean;
  showLabel: boolean; name: string; glyphId: string;
}
export interface Scene {
  glow: TraceEl[]; trace: TraceEl[]; dots: DotEl[]; nodes: NodeEl[];
  foliage: LeafEl[]; trunk: string;
  hill: { hillD: string; crestD: string };
  plate: { x: number; y: number };
}

export type FilterKey = 'all' | 'running' | 'update';

export function buildScene(geo: Geometry, tools: DecoratedTool[], selId: string, filter: FilterKey): Scene {
  const W = CANOPY_W, H = CANOPY_H;
  // tool → tip, and the status of each terminal branch segment
  const tipFor: Record<string, { tip: Tip; tool: DecoratedTool }> = {};
  geo.toolTips.forEach((tp, i) => { if (tools[i]) tipFor[tools[i].id] = { tip: tp, tool: tools[i] }; });
  const segStatus: Record<number, 'run' | 'upd' | 'ok'> = {};
  Object.keys(tipFor).forEach(id => {
    const o = tipFor[id];
    segStatus[o.tip.seg] = o.tool.isRun ? 'run' : (o.tool.isUpd ? 'upd' : 'ok');
  });

  const glow: TraceEl[] = [], trace: TraceEl[] = [];
  geo.segments.forEach((sg, i) => {
    if (i === 0) return; // segment 0 is the trunk — drawn as a filled tapered shape
    let bright = lerpColor(TEAL, '#c2f291', Math.min(1, sg.depth)), gcol = GREEN, glowOp = 0.2;
    const st = segStatus[i];
    if (st === 'run') { bright = '#6ee7b7'; gcol = '#6ee7b7'; glowOp = 0.32; }
    else if (st === 'upd') { bright = '#f3cc86'; gcol = '#e8a857'; glowOp = 0.26; }
    glow.push({ d: sg.d, stroke: gcol, width: sg.w + 5, opacity: glowOp, dash: 'none' });
    trace.push({ d: sg.d, stroke: bright, width: sg.w, opacity: 1, dash: 'none' });
  });

  const dots: DotEl[] = [];
  geo.joints.forEach(j => dots.push({ cx: j.x, cy: j.y, r: 2.4, fill: '#0b1410', stroke: GREEN, strokeWidth: 1.2, opacity: 1 }));
  geo.twigs.forEach(tw => dots.push({ cx: tw.x, cy: tw.y, r: 2.1, fill: GREEN, opacity: 0.7 }));

  const nodes: NodeEl[] = [];
  tools.forEach(tool => {
    const ref = tipFor[tool.id]; if (!ref) return;
    const tp = ref.tip, sel = tool.id === selId;
    const match = filter === 'all'
      || (filter === 'running' && tool.isRun)
      || (filter === 'update' && tool.isUpd);
    nodes.push({
      id: tool.id, x: tp.x, y: tp.y, accent: tool.accent,
      r: sel ? 32 : 27,
      sel, op: (filter !== 'all' && !match) ? 0.16 : 1,
      isUpd: tool.isUpd, isRun: tool.isRun,
      showLabel: true, name: tool.name, glyphId: tool.glyphId,
    });
  });

  // Tapered, filled trunk (organic) instead of a uniform stroke: wide at the
  // base, narrowing to the fork, with a slight inward curve on each side.
  const cxv = geo.cx, baseW = 15, topW = 5, midY = (geo.baseY + geo.forkY) / 2;
  const trunk = `M${(cxv - baseW / 2).toFixed(1)} ${geo.baseY} `
    + `C ${(cxv - baseW / 2 - 2).toFixed(1)} ${midY.toFixed(1)} ${(cxv - topW / 2 - 3).toFixed(1)} ${midY.toFixed(1)} ${(cxv - topW / 2).toFixed(1)} ${geo.forkY} `
    + `L ${(cxv + topW / 2).toFixed(1)} ${geo.forkY} `
    + `C ${(cxv + topW / 2 + 3).toFixed(1)} ${midY.toFixed(1)} ${(cxv + baseW / 2 + 2).toFixed(1)} ${midY.toFixed(1)} ${(cxv + baseW / 2).toFixed(1)} ${geo.baseY} Z`;

  // Foliage — leaf clusters at each tip (mass behind the nodes) + along the
  // branch twigs, sparser for catalog branches. Behind the nodes.
  const foliage: LeafEl[] = [];
  geo.toolTips.forEach((tp, i) => {
    if (!tools[i]) return;
    foliage.push(...leafCluster(tp.x, tp.y, tp.ang * 180 / Math.PI, 9, 14, 0.46, i + 1));
  });
  geo.twigs.forEach((tw, i) => {
    foliage.push(...leafCluster(tw.x, tw.y, 0, 3, 9.5, 0.34, i + 41));
  });

  const hy = geo.baseY;
  // Overscan past the viewBox so the hill always reaches the panel edges: the
  // tree SVG is `meet`-scaled+centred, leaving a letterbox on one axis; flat
  // skirts beyond x∈[0,W] fill side gaps, and extending below H fills any
  // bottom gap. The visible dome (0..W) keeps its original shape.
  const OX = 140, OY = 260;
  const hill = {
    hillD: `M${-OX} ${H + OY} L${-OX} ${hy + 28} L0 ${hy + 28} Q ${W * 0.5} ${hy - 6} ${W} ${hy + 28} L${W + OX} ${hy + 28} L${W + OX} ${H + OY} Z`,
    crestD: `M${-OX} ${hy + 28} L0 ${hy + 28} Q ${W * 0.5} ${hy - 6} ${W} ${hy + 28} L${W + OX} ${hy + 28}`,
  };
  return { glow, trace, dots, nodes, foliage, trunk, hill, plate: { x: geo.cx, y: hy + 20 } };
}
