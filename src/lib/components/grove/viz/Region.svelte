<script lang="ts">
  /**
   * One arrangement region (Logic-style block): a coloured, rounded block
   * spanning a stretch of the timeline, with a title bar and an inner
   * **waveform** whose character reflects the track kind + density. Pure
   * presentation — geometry comes from the parent lane (% of the full timeline),
   * the wave is deterministic (see waveform.ts) so it never flickers.
   */
  import type { Region } from '../mock/types';
  import { waveform, wavePath, type WaveKind } from './waveform';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    region,
    totalCycles,
    color,
    kind,
    seed,
    dimmed = false,
    info,
  }: {
    region: Region;
    totalCycles: number;
    color: string;
    kind: WaveKind;
    seed: number;
    dimmed?: boolean;
    /** Rich tooltip shown on hover (track summary: string or TooltipInput). */
    info?: any;
  } = $props();

  const leftPct = $derived((region.start / totalCycles) * 100);
  const widthPct = $derived((region.len / totalCycles) * 100);

  // Sample count scales with region length; the SVG stretches to fit (no
  // preserveAspectRatio), so the wave keeps its horizontal density.
  const W = 1000;
  const H = 100;
  const sampleCount = $derived(Math.max(24, Math.min(400, Math.round(region.len * 14))));
  const path = $derived(wavePath(waveform(sampleCount, kind, region.density, seed), W, H));
</script>

<div
  class="region"
  class:dimmed
  style="left: {leftPct}%; width: {widthPct}%; --c: {color};"
  use:tooltip={info ?? ''}
>
  <div class="region-bar"><span class="region-label">{region.label}</span></div>
  <div class="region-wave">
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
      <path d={path} />
    </svg>
  </div>
</div>

<style>
  .region {
    position: absolute;
    top: 5px;
    bottom: 5px;
    border-radius: 6px;
    background: linear-gradient(180deg,
      color-mix(in srgb, var(--c) 30%, var(--bg-base)),
      color-mix(in srgb, var(--c) 16%, var(--bg-base)));
    border: 1px solid color-mix(in srgb, var(--c) 65%, transparent);
    box-shadow: 0 1px 3px rgba(0,0,0,0.25), inset 0 1px 0 color-mix(in srgb, #fff 10%, transparent);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .region.dimmed { opacity: 0.4; }

  .region-bar {
    height: 15px;
    flex-shrink: 0;
    background: color-mix(in srgb, var(--c) 60%, transparent);
    display: flex;
    align-items: center;
    padding: 0 7px;
  }
  .region-label {
    font-size: 10px;
    font-weight: 600;
    color: #fff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--font-code);
    text-shadow: 0 1px 1px rgba(0,0,0,0.35);
  }

  .region-wave { flex: 1; min-height: 0; display: flex; padding: 2px 0; }
  .region-wave svg { width: 100%; height: 100%; display: block; }
  .region-wave path {
    fill: color-mix(in srgb, var(--c) 78%, #fff);
    opacity: 0.9;
  }
</style>
