<script lang="ts">
  /**
   * LevelMeter — a small row of bars that rise and fall with a 0–1 level (a microphone, a
   * volume). App-agnostic: it only draws the number it is given, so how often the level updates
   * is the caller's business. The bars are lit left to right; the colour never changes, because a
   * meter that flashes red reads as an error rather than as "loud".
   */
  let {
    level,
    bars = 12,
    height = 18,
    active = true,
    ariaLabel = 'Level',
  }: {
    /** 0 (silence) to 1 (full scale). */
    level: number;
    bars?: number;
    /** Pixel height of the tallest bar. */
    height?: number;
    /** Dimmed when false (not listening). */
    active?: boolean;
    ariaLabel?: string;
  } = $props();

  const lit = $derived(Math.round(Math.max(0, Math.min(1, level)) * bars));
</script>

<span class="meter" class:idle={!active} role="meter" aria-label={ariaLabel} aria-valuemin={0} aria-valuemax={1} aria-valuenow={level} style="--h: {height}px">
  {#each Array(bars) as _, i (i)}
    <i class:on={active && i < lit} style="height: {Math.round(height * (0.35 + 0.65 * ((i + 1) / bars)))}px"></i>
  {/each}
</span>

<style>
  .meter { display: inline-flex; align-items: flex-end; gap: 2px; height: var(--h); }
  i { width: 3px; border-radius: 1px; background: var(--border); transition: background-color 60ms linear; }
  i.on { background: var(--accent); }
  .idle { opacity: 0.5; }
</style>
