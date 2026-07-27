/**
 * Window tiling — the zones behind the mac-style green button's zoom menu.
 *
 * macOS pops a "Move & Resize" panel when you hover the zoom button; Arbor
 * paints its own (our windows are frameless everywhere, so the OS never gets a
 * chance to). This module owns both halves of it:
 *
 *  • {@link ZONE_FRACTIONS} / {@link zoneRect} — pure geometry, expressed as
 *    fractions of a work area. The menu draws each zone's preview glyph from
 *    the very same table the window is snapped to, so a new zone is one entry.
 *  • {@link applyZone} / {@link restorePrevious} — the Tauri side.
 *
 * Note we never call `setFullscreen`: on frameless (`decorations: false`)
 * windows WebView2 fails to resize back on exit and leaves a black band, so
 * "Fill" is the platform maximise instead — same as the button's own click.
 */
import {
  getCurrentWindow, currentMonitor, availableMonitors,
  PhysicalPosition, PhysicalSize,
  type Monitor, type Window,
} from '@tauri-apps/api/window';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

export type TileZone =
  | 'fill' | 'center'
  | 'left' | 'right' | 'top' | 'bottom'
  | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';

export interface Rect { x: number; y: number; width: number; height: number }

/**
 * Every zone as a fraction of the work area (0‥1). `center` is the odd one
 * out: at runtime it keeps the window's own size, so its fraction is only
 * what the menu glyph draws.
 */
export const ZONE_FRACTIONS: Record<TileZone, Rect> = {
  'fill':         { x: 0,    y: 0,   width: 1,   height: 1   },
  'center':       { x: 0.15, y: 0.15, width: 0.7, height: 0.7 },
  'left':         { x: 0,    y: 0,   width: 0.5, height: 1   },
  'right':        { x: 0.5,  y: 0,   width: 0.5, height: 1   },
  'top':          { x: 0,    y: 0,   width: 1,   height: 0.5 },
  'bottom':       { x: 0,    y: 0.5, width: 1,   height: 0.5 },
  'top-left':     { x: 0,    y: 0,   width: 0.5, height: 0.5 },
  'top-right':    { x: 0.5,  y: 0,   width: 0.5, height: 0.5 },
  'bottom-left':  { x: 0,    y: 0.5, width: 0.5, height: 0.5 },
  'bottom-right': { x: 0.5,  y: 0.5, width: 0.5, height: 0.5 },
};

/**
 * Canonical grouping — the categories macOS splits its panel into. Both
 * surfaces read this: the zoom panel renders one titled grid per group (`cols`
 * wide) and the Window ▸ Move & Resize menu uses the titles as its separators,
 * so neither can drift from the other.
 */
export interface ZoneGroup { title: string; cols: number; zones: TileZone[] }

export const ZONE_GROUPS: ZoneGroup[] = [
  { title: 'Fill & Center', cols: 2, zones: ['fill', 'center'] },
  { title: 'Halves',        cols: 4, zones: ['left', 'right', 'top', 'bottom'] },
  { title: 'Quarters',      cols: 4, zones: ['top-left', 'top-right', 'bottom-left', 'bottom-right'] },
];

/** Every zone, in presentation order. */
export const TILE_ZONES: TileZone[] = ZONE_GROUPS.flatMap(g => g.zones);

/** Human label for a zone — used by the menu caption and its aria-labels. */
export const ZONE_LABELS: Record<TileZone, string> = {
  'fill':         'Fill',
  'center':       'Center',
  'left':         'Left Half',
  'right':        'Right Half',
  'top':          'Top Half',
  'bottom':       'Bottom Half',
  'top-left':     'Top Left Quarter',
  'top-right':    'Top Right Quarter',
  'bottom-left':  'Bottom Left Quarter',
  'bottom-right': 'Bottom Right Quarter',
};

/** Project a zone onto a concrete rectangle (a monitor work area, or the
 *  menu's little preview box). Pure — no window APIs involved. */
export function zoneRect(zone: TileZone, area: Rect): Rect {
  const f = ZONE_FRACTIONS[zone];
  return {
    x:      area.x + f.x * area.width,
    y:      area.y + f.y * area.height,
    width:  f.width  * area.width,
    height: f.height * area.height,
  };
}

/**
 * Geometry captured before the FIRST zone was applied, so "Return to Previous
 * Size" can undo a whole chain of snaps in one go (mirrors macOS). Module-level
 * state is per-window: each webview loads its own module instance.
 */
type Previous = { kind: 'maximized' } | { kind: 'rect'; rect: Rect };
let previous: Previous | null = null;

/** Whether "Return to Previous Size" has anything to go back to. */
export function hasPreviousGeometry(): boolean {
  return previous !== null;
}

async function captureGeometry(w: Window): Promise<Previous> {
  if (await w.isMaximized()) return { kind: 'maximized' };
  const [pos, size] = await Promise.all([w.outerPosition(), w.outerSize()]);
  return { kind: 'rect', rect: { x: pos.x, y: pos.y, width: size.width, height: size.height } };
}

/** Remember where the window was, but only for the first snap of a chain. */
async function rememberCurrent(w: Window): Promise<void> {
  if (previous) return;
  previous = await captureGeometry(w);
}

/** A monitor's usable rectangle (taskbar/dock excluded). */
function areaOf(m: Monitor): Rect {
  return {
    x:      m.workArea.position.x,
    y:      m.workArea.position.y,
    width:  m.workArea.size.width,
    height: m.workArea.size.height,
  };
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

async function moveTo(w: Window, rect: Rect): Promise<void> {
  // A maximised window ignores position/size changes — leave that state first
  // or the snap silently no-ops.
  if (await w.isMaximized()) await w.unmaximize();
  await w.setPosition(new PhysicalPosition(Math.round(rect.x), Math.round(rect.y)));
  await w.setSize(new PhysicalSize(Math.round(rect.width), Math.round(rect.height)));
}

/**
 * Every public action goes through here: moving a window can fail for reasons
 * the user can act on (a capability that isn't granted, a monitor that went
 * away mid-click), and a rejected promise in a click handler is invisible.
 */
async function guard(what: string, run: () => Promise<void>): Promise<void> {
  try {
    await run();
  } catch (err) {
    toastStore.show(`${what} failed: ${err}`, 'error');
  }
}

/** Snap this window to `zone` on the monitor it currently sits on. */
export function applyZone(zone: TileZone): Promise<void> {
  return guard(`Snap to ${ZONE_LABELS[zone]}`, () => snapTo(zone));
}

async function snapTo(zone: TileZone): Promise<void> {
  const w = getCurrentWindow();
  await rememberCurrent(w);

  // "Fill" is the platform maximise so the OS state — and therefore the
  // button's restore glyph — stays truthful.
  if (zone === 'fill') {
    await w.maximize();
    return;
  }

  const mon = await currentMonitor();
  if (!mon) return;
  const area = areaOf(mon);

  if (zone === 'center') {
    const size = await w.outerSize();
    const width  = Math.min(size.width,  area.width);
    const height = Math.min(size.height, area.height);
    await moveTo(w, {
      x: area.x + (area.width  - width)  / 2,
      y: area.y + (area.height - height) / 2,
      width, height,
    });
    return;
  }

  await moveTo(w, zoneRect(zone, area));
}

/** One entry of the zoom panel's display switcher. */
export interface DisplayInfo {
  /** Position in the platform's monitor list — what {@link moveToDisplay} takes. */
  index: number;
  /** "Display 1", "Display 2", … — stable and readable, unlike the OS name. */
  label: string;
  /** The platform's own name (`\\.\DISPLAY1`, "Built-in Retina Display", …). */
  name: string | null;
  /** Resolution, for the panel's caption line. */
  width: number;
  height: number;
  /** Whether this window currently sits on it. */
  current: boolean;
}

/** The monitors this window could move to, in platform order. */
export async function listDisplays(): Promise<DisplayInfo[]> {
  const [all, cur] = await Promise.all([availableMonitors(), currentMonitor()]);
  return all.map((m, i) => ({
    index:  i,
    label:  `Display ${i + 1}`,
    name:   m.name,
    width:  m.size.width,
    height: m.size.height,
    // Monitors can't overlap, so their top-left corner identifies them — more
    // reliable than the name, which is empty or duplicated on some setups.
    current: !!cur && m.position.x === cur.position.x && m.position.y === cur.position.y,
  }));
}

/**
 * Send the window to another monitor, keeping where it "feels" — its offset
 * inside the work area is carried across proportionally and its size clamped to
 * fit. A maximised window arrives maximised, like macOS's *Move to Display*.
 */
export function moveToDisplay(index: number): Promise<void> {
  return guard(`Move to Display ${index + 1}`, () => sendToDisplay(index));
}

async function sendToDisplay(index: number): Promise<void> {
  const w = getCurrentWindow();
  const all = await availableMonitors();
  const target = all[index];
  if (!target) return;

  await rememberCurrent(w);
  const wasMaximized = await w.isMaximized();
  const cur  = await currentMonitor();
  const from = cur ? areaOf(cur) : areaOf(target);
  const to   = areaOf(target);
  const [pos, size] = await Promise.all([w.outerPosition(), w.outerSize()]);

  const width  = Math.min(size.width,  to.width);
  const height = Math.min(size.height, to.height);
  const relX   = from.width  ? (pos.x - from.x) / from.width  : 0;
  const relY   = from.height ? (pos.y - from.y) / from.height : 0;

  await moveTo(w, {
    x: clamp(to.x + relX * to.width,  to.x, to.x + to.width  - width),
    y: clamp(to.y + relY * to.height, to.y, to.y + to.height - height),
    width, height,
  });
  if (wasMaximized) await w.maximize();
}

/** Undo the whole snap chain, back to where the window was before the first
 *  zone was applied. No-op (beyond un-maximising) when nothing was captured. */
export function restorePrevious(): Promise<void> {
  return guard('Return to Previous Size', () => undoSnaps());
}

async function undoSnaps(): Promise<void> {
  const w = getCurrentWindow();
  const prev = previous;
  previous = null;
  if (!prev) {
    if (await w.isMaximized()) await w.unmaximize();
    return;
  }
  if (prev.kind === 'maximized') {
    await w.maximize();
    return;
  }
  await moveTo(w, prev.rect);
}
