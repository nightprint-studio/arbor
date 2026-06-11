<script lang="ts">
  /**
   * Stereo peak meter (L / R) driven by a linear `[l, r]` peak from the engine
   * (`grove:meters`). The level gradient is green → amber → red along the full
   * track, with an opaque "shutter" masking the inactive part, so only a hot
   * signal lights the red tip (clip-risk cue). Track colour is intentionally NOT
   * used here — the colour encodes *level*, not identity.
   *
   * Grove-local presentational widget shared by the mixer strips + master + the
   * Inspector (a candidate for `shared/ui/` if it gets reused outside grove).
   */
  import type { GroveStereoPeak } from '$lib/ipc/grove';

  let {
    peak,
    orientation = 'vertical',
    dimmed = false,
  }: {
    peak: GroveStereoPeak;
    orientation?: 'vertical' | 'horizontal';
    /** Force the meter empty (muted / solo-excluded strip). */
    dimmed?: boolean;
  } = $props();

  const clamp = (x: number) => Math.max(0, Math.min(1, x));
  const l = $derived(dimmed ? 0 : clamp(peak[0]));
  const r = $derived(dimmed ? 0 : clamp(peak[1]));
  // The shutter covers the *inactive* tail (1 - level).
  const off = (x: number) => `${((1 - x) * 100).toFixed(1)}%`;
</script>

<div class="meter {orientation}" aria-hidden="true">
  <span class="ch"><span class="off" style="--off: {off(l)}"></span></span>
  <span class="ch"><span class="off" style="--off: {off(r)}"></span></span>
</div>

<style>
  .meter { display: flex; gap: 2px; }
  .meter.vertical { flex-direction: row; height: 100%; }
  .meter.horizontal { flex-direction: column; width: 100%; }

  .ch {
    position: relative;
    border-radius: 2px;
    overflow: hidden;
    background: linear-gradient(
      var(--grad-dir),
      var(--success) 0%,
      var(--success) 55%,
      var(--warning) 80%,
      var(--error) 100%
    );
  }
  .meter.vertical .ch { --grad-dir: to top; width: 5px; flex: 1; }
  .meter.horizontal .ch { --grad-dir: to right; height: 5px; flex: 1; }

  /* Shutter — opaque mask over the inactive part of the track. */
  .off { position: absolute; background: var(--bg-input); transition: all 90ms linear; }
  .meter.vertical .off { top: 0; left: 0; right: 0; height: var(--off); }
  .meter.horizontal .off { top: 0; bottom: 0; right: 0; width: var(--off); }
</style>
