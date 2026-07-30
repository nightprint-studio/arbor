<script lang="ts">
  /**
   * Arrangement view toolbar — a thin IntelliJ/Logic-style strip of icon
   * toggles above the ruler that drive how the timeline is *drawn* (not its
   * contents). State lives in the viz-local `arrViewOptions` store; every toggle
   * is keyboard reachable (real <button>, aria-pressed) with a tooltip.
   *
   * Sticky-left so it stays in view while the (much wider) timeline scrolls.
   * Imports only merula-local + the shared tooltip action.
   */
  import { AudioLines, Crosshair, Grid3x3, Tag, Repeat, Timer, Hourglass, Map, ZoomIn, ZoomOut, Gauge } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { arrViewOptions as o, MIN_ZOOM, MAX_ZOOM } from './arr-view-options.svelte';
  import { transportUiStore } from '../stores/transport-ui.svelte';
  import TempoControl from './TempoControl.svelte';

  const hasLoop = $derived(transportUiStore.loop != null);
  const loopOn = $derived(transportUiStore.loopActive);
  const metroOn = $derived(transportUiStore.metronome);
  const countIn = $derived(transportUiStore.countIn);
  const zoomPct = $derived(Math.round(o.zoom * 100));

  const toggles = $derived([
    { key: 'waveform', icon: AudioLines, on: o.waveform, toggle: () => o.toggleWaveform(),
      tip: { content: 'Waveform', description: 'Draw audio regions as a waveform (sample / drum / signal lanes)' } },
    { key: 'follow', icon: Crosshair, on: o.follow, toggle: () => o.toggleFollow(),
      tip: { content: 'Follow playhead', description: 'Auto-scroll to keep the playhead in view while playing' } },
    { key: 'grid', icon: Grid3x3, on: o.grid, toggle: () => o.toggleGrid(),
      tip: { content: 'Grid', description: 'Show the bar grid lines' } },
    { key: 'labels', icon: Tag, on: o.labels, toggle: () => o.toggleLabels(),
      tip: { content: 'Labels', description: 'Show note / sound names on events when they fit' } },
    { key: 'minimap', icon: Map, on: o.minimap, toggle: () => o.toggleMinimap(),
      tip: { content: 'Minimap', description: 'Show the overview strip + viewport box below the timeline' } },
    { key: 'velocity', icon: Gauge, on: o.velocity, toggle: () => o.toggleVelocity(),
      tip: { content: 'Velocity heatmap', description: 'Colour events by gain — quieter notes fade, full-gain notes stay vivid' } },
  ] as const);
</script>

<div class="arr-tb" role="toolbar" tabindex="-1" aria-label="Arrangement view options">
  {#each toggles as t (t.key)}
    <button
      class="tb-btn"
      class:on={t.on}
      type="button"
      aria-pressed={t.on}
      aria-label={t.tip.content}
      use:tooltip={t.tip}
      onclick={t.toggle}
    >
      <t.icon size={14} />
    </button>
  {/each}
  <span class="tb-div"></span>
  <button
    class="tb-btn"
    class:on={loopOn}
    type="button"
    disabled={!hasLoop}
    aria-pressed={loopOn}
    aria-label="Loop region"
    use:tooltip={{ content: 'Loop region', description: hasLoop ? 'Cycle playback within the loop (Alt-drag the ruler to set it · Esc clears)' : 'Alt-drag the ruler to set a loop region' }}
    onclick={() => transportUiStore.toggleLoop()}
  >
    <Repeat size={14} />
  </button>
  <button
    class="tb-btn"
    class:on={metroOn}
    type="button"
    aria-pressed={metroOn}
    aria-label="Metronome"
    use:tooltip={{ content: 'Metronome · Ctrl+Shift+B', description: 'Audible click track on every beat (accented on the bar)' }}
    onclick={() => transportUiStore.toggleMetronome()}
  >
    <Timer size={14} />
  </button>
  <button
    class="tb-btn count"
    class:on={countIn > 0}
    type="button"
    aria-pressed={countIn > 0}
    aria-label="Count-in"
    use:tooltip={{ content: 'Count-in · Ctrl+Shift+U', description: countIn > 0 ? `${countIn} bar${countIn > 1 ? 's' : ''} of metronome pre-roll before playback starts (click to step)` : 'No count-in — click to add a metronome pre-roll before playback' }}
    onclick={() => transportUiStore.cycleCountIn()}
  >
    <Hourglass size={14} />
    {#if countIn > 0}<span class="cnt">{countIn}</span>{/if}
  </button>
  <span class="tb-div"></span>
  <TempoControl />
  <span class="tb-div"></span>
  <button
    class="tb-btn"
    type="button"
    disabled={o.zoom <= MIN_ZOOM}
    aria-label="Zoom out"
    use:tooltip={{ content: 'Zoom out', description: 'Ctrl+wheel over the timeline zooms too' }}
    onclick={() => o.zoomOut()}
  >
    <ZoomOut size={14} />
  </button>
  <button
    class="tb-zoom"
    type="button"
    aria-label="Reset zoom"
    use:tooltip={{ content: 'Reset zoom to 100%', description: 'Click to reset · Ctrl+wheel over the timeline to zoom' }}
    onclick={() => o.zoomReset()}
  >
    {zoomPct}%
  </button>
  <button
    class="tb-btn"
    type="button"
    disabled={o.zoom >= MAX_ZOOM}
    aria-label="Zoom in"
    use:tooltip={{ content: 'Zoom in', description: 'Ctrl+wheel over the timeline zooms too' }}
    onclick={() => o.zoomIn()}
  >
    <ZoomIn size={14} />
  </button>
</div>

<style>
  .arr-tb {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 100%;
    padding: 0 6px;
  }
  .tb-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 22px;
    flex-shrink: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tb-btn:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .tb-btn.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .tb-btn.on:hover { background: color-mix(in srgb, var(--accent) 26%, transparent); }
  .tb-btn:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent);
  }
  .tb-btn:disabled { opacity: 0.4; cursor: default; }
  .tb-btn:disabled:hover { background: transparent; color: var(--text-muted); }
  .tb-div { width: 1px; height: 14px; background: var(--border-subtle); margin: 0 4px; flex-shrink: 0; }
  /* Zoom readout doubles as a reset button. */
  .tb-zoom {
    min-width: 38px;
    height: 22px;
    padding: 0 4px;
    flex-shrink: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-size: var(--font-size-2xs);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tb-zoom:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .tb-zoom:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent);
  }
  /* Count-in: a small bar-count badge over the hourglass when a pre-roll is set. */
  .tb-btn.count { position: relative; }
  .cnt {
    position: absolute;
    right: 1px;
    bottom: 0;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
    background: var(--bg-base);
    border-radius: 2px;
    padding: 0 1px;
  }
</style>
