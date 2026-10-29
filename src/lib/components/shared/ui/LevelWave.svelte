<script lang="ts">
  /**
   * LevelWave — a thin row of bars that looks like a waveform and is driven by one 0–1 level (a
   * microphone, a volume). The newest reading appears in the middle and spreads outwards, so the row
   * ripples with the voice instead of just rising and falling like {@link LevelMeter}.
   *
   * Cheap on purpose: the bars are fixed DOM that is never re-rendered; a requestAnimationFrame loop
   * (sampled at ~22 Hz) writes a `transform` on each, and goes to sleep once the row has settled flat.
   * App-agnostic: it only draws the number it is given.
   */
  import { onDestroy } from 'svelte';

  let {
    level,
    bars = 33,
    height = 36,
    active = true,
    ariaLabel = 'Level',
  }: {
    /** 0 (silence) to 1 (full scale). */
    level: number;
    bars?: number;
    /** Pixel height of the tallest bar. */
    height?: number;
    /** Flattens (and stops animating) when false. */
    active?: boolean;
    ariaLabel?: string;
  } = $props();

  const SAMPLE_MS = 45;
  const FLOOR = 0.07;

  let host = $state<HTMLElement | undefined>();
  let latest = 0;
  let history: number[] = [];
  let lastSample = 0;
  let frame = 0;

  const clamp01 = (n: number) => Math.max(0, Math.min(1, n));

  /** A fixed, uneven profile so a steady level still reads as a waveform rather than a flat block. */
  function profile(i: number, n: number): number {
    const window = 0.4 + 0.6 * Math.sin((Math.PI * (i + 0.5)) / n);
    const jitter = 0.78 + 0.22 * (((i * 7) % 5) / 4);
    return window * jitter;
  }

  function paint(kids: HTMLCollection) {
    const n = kids.length;
    const half = history.length;
    for (let i = 0; i < n; i++) {
      const d = Math.min(half - 1, Math.floor(Math.abs(i - (n - 1) / 2)));
      const v = Math.max(FLOOR, history[half - 1 - d] * profile(i, n));
      (kids[i] as HTMLElement).style.transform = `scaleY(${v.toFixed(3)})`;
    }
  }

  function step(now: number) {
    frame = 0;
    if (!host) return;
    const kids = host.children;
    const half = Math.ceil(kids.length / 2);
    if (history.length !== half) history = new Array(half).fill(0);
    if (now - lastSample >= SAMPLE_MS) {
      lastSample = now;
      history.shift();
      // A touch of release so a word's end fades out instead of dropping off a cliff.
      history.push(Math.max(latest, history[history.length - 1] * 0.82));
      paint(kids);
    }
    if (latest > 0 || history.some((v) => v > 0.01)) frame = requestAnimationFrame(step);
  }

  $effect(() => {
    latest = active ? clamp01(level) : 0;
    if (host && !frame) frame = requestAnimationFrame(step);
  });

  onDestroy(() => { if (frame) cancelAnimationFrame(frame); });
</script>

<span class="wave" class:idle={!active} role="meter" aria-label={ariaLabel} aria-valuemin={0} aria-valuemax={1} aria-valuenow={level}
      style="--h: {height}px" bind:this={host}>
  {#each Array(bars) as _, i (i)}<i></i>{/each}
</span>

<style>
  .wave { display: inline-flex; align-items: center; justify-content: center; gap: 3px; height: var(--h); }
  i {
    width: 3px; height: 100%; border-radius: 2px; transform: scaleY(0.07);
    background: linear-gradient(to top, var(--accent), var(--info));
    transition: transform 60ms linear;
  }
  .idle { opacity: 0.4; }

  @media (prefers-reduced-motion: reduce) {
    i { transition: none; }
  }
</style>
