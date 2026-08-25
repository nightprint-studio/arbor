<script lang="ts">
  /**
   * Minimap — a compressed overview of the whole arrangement with a draggable
   * viewport box, so you can pan a zoomed-in timeline at a glance. Read-only
   * mirror of the lanes (one thin row each, haps as blocks), overlaid with the
   * section bands, loop region, playhead / cursor, and the current view window.
   *
   * Coordinates are pure cycle → percent (`cycle / mapCycles`), so it's agnostic
   * to the pixels-per-cycle zoom the main timeline uses. Click / drag pans: it
   * reports the cycle under the pointer; the parent centres the view there.
   */
  import { laneColor, sectionColor } from '../palette';
  import type { VizLane } from './arrangement.svelte';
  import type { LoopRegion } from '../stores/transport-ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    lanes: VizLane[];
    /** Total cycle extent the minimap spans (song length, ≥ the view window). */
    mapCycles: number;
    /** Visible cycle window in the main timeline (drives the viewport box). */
    viewStart: number;
    viewEnd: number;
    /** Display playhead cycle, or < 0 to hide it (stopped / off-screen). */
    playCycle: number;
    cursorCycle: number;
    loop: LoopRegion | null;
    /** Pan request: centre the main view on `centerCycle`. */
    onPan: (centerCycle: number) => void;
  }

  let { lanes, mapCycles, viewStart, viewEnd, playCycle, cursorCycle, loop, onPan }: Props = $props();

  const span = $derived(Math.max(mapCycles, 0.0001));
  const pct = (c: number) => `${Math.max(0, Math.min(100, (c / span) * 100))}%`;
  const wpct = (a: number, b: number) => `${Math.max(0.4, Math.min(100, ((b - a) / span) * 100))}%`;

  let el = $state<HTMLElement | null>(null);

  /** Cycle under a client x, clamped to the map. */
  function cycleAt(clientX: number): number {
    if (!el) return 0;
    const r = el.getBoundingClientRect();
    return Math.max(0, Math.min(span, ((clientX - r.left) / r.width) * span));
  }

  // Drag (and plain click) pans the view, centring it on the pointer.
  function startDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const apply = (clientX: number) => onPan(cycleAt(clientX));
    apply(e.clientX);
    const onMove = (ev: MouseEvent) => apply(ev.clientX);
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
<div class="mm" bind:this={el} onmousedown={startDrag} use:tooltip={'Drag to pan the timeline'}>
  <!-- Section bands (full height, tinted) -->
  {#each lanes[0]?.sections ?? [] as s (s.name + '@' + s.start)}
    <div class="mm-band" style="left: {pct(s.start)}; width: {wpct(s.start, s.end)}; --sc: {sectionColor(s.name)}"></div>
  {/each}

  <!-- One row per lane, haps as compressed blocks -->
  <div class="mm-lanes" style="--n: {lanes.length}">
    {#each lanes as lane (lane.track)}
      {@const c = laneColor(lane.track)}
      <div class="mm-lane">
        {#each lane.haps as h, i (i)}
          <div class="mm-hap" style="left: {pct(h.start)}; width: {wpct(h.start, h.end)}; background: {c}"></div>
        {/each}
      </div>
    {/each}
  </div>

  {#if loop}
    <div class="mm-loop" style="left: {pct(loop.start)}; width: {wpct(loop.start, loop.end)}"></div>
  {/if}

  <!-- Current view window -->
  <div class="mm-view" style="left: {pct(viewStart)}; width: {wpct(viewStart, viewEnd)}"></div>

  {#if cursorCycle >= 0}
    <div class="mm-cursor" style="left: {pct(cursorCycle)}"></div>
  {/if}
  {#if playCycle >= 0}
    <div class="mm-play" style="left: {pct(playCycle)}"></div>
  {/if}
</div>

<style>
  .mm {
    position: relative;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-base);
    cursor: pointer;
  }
  .mm-band {
    position: absolute;
    top: 0; bottom: 0;
    background: color-mix(in srgb, var(--sc) 16%, transparent);
    border-left: 1px solid color-mix(in srgb, var(--sc) 35%, transparent);
    pointer-events: none;
  }
  .mm-lanes {
    position: absolute;
    inset: 2px 0;
    display: grid;
    grid-template-rows: repeat(var(--n), 1fr);
    gap: 1px;
    pointer-events: none;
  }
  .mm-lane { position: relative; min-height: 0; }
  .mm-hap {
    position: absolute;
    top: 0; bottom: 0;
    min-width: 1px;
    border-radius: 1px;
    opacity: 0.85;
  }
  .mm-loop {
    position: absolute;
    top: 0; bottom: 0;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-left: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-right: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    pointer-events: none;
  }
  .mm-view {
    position: absolute;
    top: 0; bottom: 0;
    background: color-mix(in srgb, var(--text-primary) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--text-primary) 45%, transparent);
    border-radius: 2px;
    pointer-events: none;
  }
  .mm-cursor { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--text-secondary); pointer-events: none; }
  .mm-play   { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--success); box-shadow: 0 0 4px var(--success); pointer-events: none; }
</style>
