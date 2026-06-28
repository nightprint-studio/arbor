<script lang="ts">
  /**
   * Gain-reduction meter: a thin bar that fills DOWNWARD from the top as a
   * dynamics processor ducks the signal — the console convention (the louder you
   * hit the limiter/compressor, the more bar drops from 0 dB). Driven by a linear
   * reduction amount `0..1` (`0` = none) from the engine (`merula:meters` for the
   * master limiter; per-track compressors later). Colour encodes *amount*, not
   * identity, like {@link PeakMeter}.
   *
   * Merula-local presentational widget (master limiter + future per-strip comp).
   */
  let {
    reduction,
    /** dB at which the bar reads full — typical limiter ducking lives within ~12 dB. */
    range = 12,
  }: {
    reduction: number;
    range?: number;
  } = $props();

  // reduction → dB of attenuation (≥ 0), then a 0..1 fill over `range`.
  const gain = $derived(Math.max(0, Math.min(1, 1 - reduction)));
  const db = $derived(gain <= 0.0001 ? range : -20 * Math.log10(gain));
  const fill = $derived(Math.max(0, Math.min(1, db / range)));
</script>

<div class="gr" aria-hidden="true">
  <span class="bar" style="--fill: {(fill * 100).toFixed(1)}%"></span>
</div>

<style>
  .gr {
    position: relative;
    width: 5px;
    height: 100%;
    border-radius: 2px;
    overflow: hidden;
    background: var(--bg-input);
  }
  /* Fills from the top down — the bar IS the reduction (amber → red as it deepens). */
  .bar {
    position: absolute;
    top: 0; left: 0; right: 0;
    height: var(--fill);
    background: linear-gradient(to bottom, var(--warning) 0%, var(--error) 100%);
    transition: height 90ms linear;
  }
</style>
