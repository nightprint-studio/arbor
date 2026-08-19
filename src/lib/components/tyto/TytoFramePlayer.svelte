<script lang="ts">
  /**
   * TytoFramePlayer — plays a recorded frame sequence.
   *
   * A sequence is not a video and can't be handed to a `<video>`: the frames are
   * separate images and, because identical ones were never written, they are spaced
   * by the manifest's per-frame timestamps rather than by a constant fps. So playback
   * is a clock, not a frame counter — a rAF loop advances a virtual time and shows
   * the frame that was on screen at that moment, which is what makes a ten-second
   * still stretch stay ten seconds instead of collapsing to one frame.
   *
   * Frames are drawn into a `<canvas>` and fetched in a window around the playhead:
   * a long recording is hundreds of images, and holding them all decoded would cost
   * more memory than the recording did on disk.
   */
  import { Play, Pause, SkipBack, SkipForward, Repeat, ChevronFirst, ChevronLast } from 'lucide-svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { FrameSequence } from '$lib/stores/tyto/recorder.svelte';

  let { sequence }: { sequence: FrameSequence } = $props();

  /** Frames fetched ahead of the playhead. Roughly five seconds at 12 fps. */
  const PREFETCH_AHEAD = 60;
  /** Decoded frames kept around the playhead before the far ones are dropped. */
  const KEEP_RADIUS = 90;

  const count = $derived(sequence.frames.length);
  const speedOptions = [
    { value: '0.5', label: '0.5×' },
    { value: '1',   label: '1×' },
    { value: '2',   label: '2×' },
  ];

  let index = $state(0);
  let playing = $state(true);
  let looping = $state(true);
  let speed = $state(1);

  let canvas = $state<HTMLCanvasElement | null>(null);
  let root = $state<HTMLDivElement | null>(null);

  // Decoded-frame cache, keyed by index. Plain (non-reactive) state: it is touched
  // per animation frame and nothing renders off it directly.
  const cache = new Map<number, HTMLImageElement>();
  /** Virtual playhead in ms — the authority; `index` follows it. */
  let clockMs = 0;
  let raf = 0;
  let lastTick = 0;

  /** The frame on screen at `ms`: the last one whose time has already passed. */
  function frameAt(ms: number): number {
    // A linear scan: a few hundred comparisons per animation frame costs nothing,
    // and starting from zero means a backward scrub needs no special case.
    let i = 0;
    while (i + 1 < count && sequence.times[i + 1] <= ms) i++;
    return i;
  }

  function ensure(i: number) {
    if (i < 0 || i >= count || cache.has(i)) return;
    const img = new Image();
    img.decoding = 'async';
    cache.set(i, img);
    img.onload = () => { if (i === index) render(); };
    img.src = sequence.frames[i];
  }

  function prefetchAround(i: number) {
    for (let k = i; k < Math.min(count, i + PREFETCH_AHEAD); k++) ensure(k);
    if (cache.size > KEEP_RADIUS * 2) {
      for (const k of [...cache.keys()]) {
        if (Math.abs(k - i) > KEEP_RADIUS) cache.delete(k);
      }
    }
  }

  /** Paint the current frame. A frame still decoding leaves the previous one up —
   *  a held frame reads as "nothing changed", a cleared canvas reads as broken. */
  function render() {
    const ctx = canvas?.getContext('2d');
    const img = cache.get(index);
    if (!ctx || !canvas || !img || !img.complete || img.naturalWidth === 0) return;
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  }

  function seek(to: number, resetClock = true) {
    index = Math.max(0, Math.min(count - 1, to));
    if (resetClock) clockMs = sequence.times[index] ?? 0;
    prefetchAround(index);
    render();
  }

  function step(delta: number) {
    playing = false;
    seek(index + delta);
  }

  function tick(now: number) {
    raf = requestAnimationFrame(tick);
    const dt = lastTick ? now - lastTick : 0;
    lastTick = now;
    if (!playing) return;

    clockMs += dt * speed;
    if (clockMs >= sequence.durationMs) {
      if (!looping) { playing = false; clockMs = sequence.durationMs; }
      else { clockMs = 0; }
    }
    const want = frameAt(clockMs);
    if (want !== index) {
      index = want;
      prefetchAround(index);
      render();
    }
  }

  $effect(() => {
    // A new sequence restarts from the top with a cold cache.
    void sequence;
    cache.clear();
    index = 0;
    clockMs = 0;
    playing = true;
    prefetchAround(0);
    render();
  });

  $effect(() => {
    lastTick = 0;
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  // Keyboard-first: the player takes focus so Space / arrows work the moment the
  // preview opens, without hunting for a control to click.
  $effect(() => { root?.focus(); });

  function onKeyDown(e: KeyboardEvent) {
    const jump = e.shiftKey ? 10 : 1;
    switch (e.key) {
      case ' ':          playing = !playing; break;
      case 'ArrowLeft':  step(-jump); break;
      case 'ArrowRight': step(jump); break;
      case 'Home':       playing = false; seek(0); break;
      case 'End':        playing = false; seek(count - 1); break;
      case 'l': case 'L': looping = !looping; break;
      default: return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  /** `12.4s` — a tutorial lives in seconds, not in minutes and colons. */
  function secs(ms: number): string {
    return `${(Math.max(0, ms) / 1000).toFixed(1)}s`;
  }
</script>

<div
  class="player"
  role="group"
  aria-label="Frame sequence player"
  tabindex="-1"
  bind:this={root}
  onkeydown={onKeyDown}
>
  <div class="stage">
    <canvas bind:this={canvas} width={sequence.width} height={sequence.height}></canvas>
    <span class="badge">{sequence.width} × {sequence.height} · {count} frames</span>
  </div>

  <input
    class="scrub"
    type="range"
    min="0"
    max={Math.max(0, count - 1)}
    value={index}
    aria-label="Timeline"
    oninput={(e) => seek(Number((e.currentTarget as HTMLInputElement).value))}
  />

  <div class="bar">
    <button type="button" class="ctl" aria-label="First frame" use:tooltip={{ content: 'First frame', shortcut: 'Home' }} onclick={() => { playing = false; seek(0); }}>
      <ChevronFirst size={15} />
    </button>
    <button type="button" class="ctl" aria-label="Previous frame" use:tooltip={{ content: 'Previous frame', shortcut: '←' }} onclick={() => step(-1)}>
      <SkipBack size={14} />
    </button>
    <button type="button" class="ctl primary" aria-label={playing ? 'Pause' : 'Play'} use:tooltip={{ content: playing ? 'Pause' : 'Play', shortcut: 'Space' }} onclick={() => (playing = !playing)}>
      {#if playing}<Pause size={15} fill="currentColor" />{:else}<Play size={15} fill="currentColor" />{/if}
    </button>
    <button type="button" class="ctl" aria-label="Next frame" use:tooltip={{ content: 'Next frame', shortcut: '→' }} onclick={() => step(1)}>
      <SkipForward size={14} />
    </button>
    <button type="button" class="ctl" aria-label="Last frame" use:tooltip={{ content: 'Last frame', shortcut: 'End' }} onclick={() => { playing = false; seek(count - 1); }}>
      <ChevronLast size={15} />
    </button>

    <span class="readout">
      <b>{secs(sequence.times[index] ?? 0)}</b> / {secs(sequence.durationMs)}
      <span class="dim">· frame {index + 1} of {count}</span>
    </span>

    <button
      type="button"
      class="ctl"
      class:on={looping}
      aria-label="Loop"
      aria-pressed={looping}
      use:tooltip={{ content: 'Loop', shortcut: 'L' }}
      onclick={() => (looping = !looping)}
    ><Repeat size={14} /></button>

    <RadioGroup
      appearance="segment"
      size="sm"
      nowrap
      value={String(speed)}
      options={speedOptions}
      onchange={(v) => (speed = Number(v))}
    />
  </div>
</div>

<style>
  .player { display: flex; flex-direction: column; gap: 8px; outline: none; }

  .stage {
    position: relative;
    aspect-ratio: 16 / 9;
    background: #05070b;
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  /* Letterboxed on the dark backing, so a non-16:9 capture reads as intentional. */
  .stage canvas { display: block; width: 100%; height: 100%; object-fit: contain; }
  .badge {
    position: absolute; left: 10px; bottom: 10px;
    font-size: var(--font-size-2xs); font-weight: 500;
    color: #fff; background: rgba(0, 0, 0, 0.45);
    padding: 3px 8px; border-radius: 6px;
    font-variant-numeric: tabular-nums;
  }

  /* Timeline — an accent track, tall enough to grab without aiming. */
  .scrub {
    width: 100%;
    height: 16px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .ctl {
    display: inline-flex; align-items: center; justify-content: center;
    width: 30px; height: 28px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .ctl:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ctl.primary {
    width: 34px;
    background: var(--accent);
    color: var(--text-on-accent, #fff);
    box-shadow: 0 2px 8px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .ctl.primary:hover { filter: brightness(1.08); background: var(--accent); color: var(--text-on-accent, #fff); }
  .ctl.on { color: var(--accent); background: var(--accent-subtle); border-color: color-mix(in srgb, var(--accent) 35%, transparent); }

  .readout {
    flex: 1;
    padding-left: 8px;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .readout b { color: var(--text-primary); font-weight: 650; }
  .readout .dim { color: var(--text-muted); }
</style>
