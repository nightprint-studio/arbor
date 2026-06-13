/**
 * Transport UI store — the front-end-only transport state the BE clock doesn't
 * own: the **edit cursor** (the seek anchor the ruler + arrows move), the **loop
 * region**, and **markers** (named jump points).
 *
 * These ride the existing seek (`nemusEngine.seek`) rather than adding real-time
 * transport state: the loop is enforced by seeking back to its start when the
 * playhead crosses the end (the watch lives in {@link NemusShell}, always
 * mounted), and play-from-cursor is seek-then-play. ~30 fps transport-event
 * granularity is fine for a practice/section loop.
 *
 * Cursor + loop + markers are in **arrangement cycles** (the displayed, looped
 * timeline — `raw % loopCycles`), matching the ruler.
 */

import { nemusEngine } from './engine.svelte';

/** An inclusive-start, exclusive-end loop span, in arrangement cycles. */
export interface LoopRegion {
  start: number;
  end: number;
}

/** A named jump point on the ruler, in arrangement cycles. */
export interface Marker {
  id: number;
  cycle: number;
  label: string;
}

function createTransportUiStore() {
  // Edit / seek cursor (the playhead anchor): last ruler click / arrow seek.
  let cursor = $state(0);
  // Loop region. Kept as two bounds + an enabled flag so toggling off preserves
  // the region (re-enable without redrawing it).
  let loopStart = $state<number | null>(null);
  let loopEnd = $state<number | null>(null);
  let loopOn = $state(false);
  // Markers, kept sorted by cycle; ids are monotonic (stable keys / nav).
  let markers = $state<Marker[]>([]);
  let nextMarkerId = 1;

  return {
    // ── Edit cursor ──────────────────────────────────────────────────────────
    get cursor() { return cursor; },
    setCursor(c: number) { cursor = Math.max(0, c); },

    // ── Loop region ──────────────────────────────────────────────────────────
    get loopStart() { return loopStart; },
    get loopEnd() { return loopEnd; },
    /** The loop span when one is defined (end strictly after start), else null. */
    get loop(): LoopRegion | null {
      return loopStart != null && loopEnd != null && loopEnd > loopStart
        ? { start: loopStart, end: loopEnd }
        : null;
    },
    /** Whether the loop is both defined AND switched on (drives the seek-back). */
    get loopActive(): boolean { return loopOn && this.loop != null; },
    /** Define (and switch on) a loop from two cycle bounds (order-independent). */
    setLoop(a: number, b: number) {
      const s = Math.max(0, Math.min(a, b));
      const e = Math.max(a, b);
      if (e - s < 0.01) return; // ignore a zero-width drag
      loopStart = s; loopEnd = e; loopOn = true;
    },
    /** Toggle the loop on/off (no-op when no region is defined). */
    toggleLoop() { if (this.loop) loopOn = !loopOn; },
    /** Remove the loop entirely. */
    clearLoop() { loopStart = null; loopEnd = null; loopOn = false; },

    // ── Play-from-cursor / punch-in ──────────────────────────────────────────
    /** Seek to the cursor and start playback there (not from cycle 0). */
    playFromCursor() {
      void nemusEngine.seek(cursor);
      void nemusEngine.play();
    },

    // ── Markers ──────────────────────────────────────────────────────────────
    get markers() { return markers; },
    /** Add a marker at `cycle` (defaults to a numbered label), returning its id. */
    addMarker(cycle: number, label?: string): number {
      const id = nextMarkerId++;
      const m: Marker = { id, cycle: Math.max(0, cycle), label: label ?? `M${markers.length + 1}` };
      markers = [...markers, m].sort((x, y) => x.cycle - y.cycle);
      return id;
    },
    removeMarker(id: number) { markers = markers.filter((m) => m.id !== id); },
    renameMarker(id: number, label: string) {
      markers = markers.map((m) => (m.id === id ? { ...m, label } : m));
    },
    clearMarkers() { markers = []; },
    /** The nearest marker strictly after / before `cycle` (for keyboard nav). */
    nextMarker(cycle: number): Marker | null {
      return markers.find((m) => m.cycle > cycle + 0.001) ?? null;
    },
    prevMarker(cycle: number): Marker | null {
      return [...markers].reverse().find((m) => m.cycle < cycle - 0.001) ?? null;
    },
    /** Move the cursor to the next/prev marker and seek there. Returns it (or null
     *  when there's none in that direction). Shared by the arrangement keyboard nav
     *  and the command palette. */
    seekNextMarker(): Marker | null {
      const m = markers.find((x) => x.cycle > cursor + 0.001) ?? null;
      if (m) { cursor = m.cycle; void nemusEngine.seek(m.cycle); }
      return m;
    },
    seekPrevMarker(): Marker | null {
      const m = [...markers].reverse().find((x) => x.cycle < cursor - 0.001) ?? null;
      if (m) { cursor = m.cycle; void nemusEngine.seek(m.cycle); }
      return m;
    },
    /** Seed the markers (e.g. from persisted per-project state). */
    setMarkers(ms: Marker[]) {
      markers = [...ms].sort((a, b) => a.cycle - b.cycle);
      nextMarkerId = ms.reduce((mx, m) => Math.max(mx, m.id), 0) + 1;
    },
  };
}

export const transportUiStore = createTransportUiStore();
