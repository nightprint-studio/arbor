<script module lang="ts">
  export type ImberOrbState = 'idle' | 'listening' | 'thinking' | 'speaking';
</script>

<script lang="ts">
  /**
   * ImberOrb — Imber's emblem: a glowing core inside a breathing halo, with two small moons in orbit.
   * One implementation for every place Imber shows up (the assistant's home, the voice overlay), so the
   * mark looks and moves the same everywhere.
   *
   *   idle       a soft breath.
   *   listening  ripples leave the core and the halo swells with the microphone `level`.
   *   thinking   a thin arc turns around the core and the moons orbit faster.
   *   speaking   the halo swells with `level` (or breathes when there is none) while an answer arrives.
   *
   * Animation is transform/opacity only. The live `level` is eased in a requestAnimationFrame loop that
   * writes one CSS variable on one element — no reactive churn, no layout — and the loop sleeps as soon as
   * the value has settled. No `backdrop-filter` anywhere (it keeps the WebView2 compositor busy). Motion
   * stops with `prefers-reduced-motion` and with the app's animations switched off; the state is then
   * still readable from the surrounding text.
   */
  import { onDestroy, type Snippet } from 'svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  let {
    size = 76,
    hue = 'var(--accent)',
    level = 0,
    // Renamed locally: a variable called `state` would hide the `$state` rune below.
    state: orbState = 'idle',
    children,
  }: {
    /** Diameter in px of the whole mark, orbits included. */
    size?: number;
    /** Any CSS colour; the second accent is always `--info`. */
    hue?: string;
    /** 0..1 loudness; only listening and speaking react to it. */
    level?: number;
    state?: ImberOrbState;
    /** What sits on the core (an icon). */
    children?: Snippet;
  } = $props();

  let lv = $state<HTMLElement | undefined>();
  let target = 0;
  let shown = 0;
  let frame = 0;

  const reactive = $derived(orbState === 'listening' || orbState === 'speaking');

  function tick() {
    frame = 0;
    // Quick attack, slow release: speech peaks read as a beat and the fall-off as a breath.
    shown += (target - shown) * (target > shown ? 0.5 : 0.12);
    if (Math.abs(target - shown) < 0.003) shown = target;
    lv?.style.setProperty('--lvl', shown.toFixed(3));
    if (shown !== target) frame = requestAnimationFrame(tick);
  }

  $effect(() => {
    target = reactive && animStore.enabled ? Math.max(0, Math.min(1, level)) : 0;
    if (!frame) frame = requestAnimationFrame(tick);
  });

  onDestroy(() => { if (frame) cancelAnimationFrame(frame); });
</script>

<span class="orb" class:still={!animStore.enabled} data-state={orbState} aria-hidden="true"
      style="--s: {size}px; --orb-hue: {hue}">
  <span class="ring r1"></span>
  <span class="ring r2"></span>
  <span class="lv" bind:this={lv}>
    <span class="halo"></span>
    <span class="core">{@render children?.()}</span>
  </span>
  <span class="arc"></span>
  <span class="orbit o1"><i></i></span>
  <span class="orbit o2"><i></i></span>
</span>

<style>
  .orb { position: relative; display: block; flex-shrink: 0; width: var(--s); height: var(--s); }

  /* The part that follows the microphone. `--lvl` is written by the script, 0..1. */
  .lv { --lvl: 0; position: absolute; inset: 0; display: grid; place-items: center; }

  .halo {
    position: absolute; inset: calc(var(--s) * 0.079); border-radius: 50%;
    transform: scale(calc(1 + var(--lvl) * 0.55));
    opacity: calc(0.75 + var(--lvl) * 0.25);
    transition: transform 70ms linear, opacity 70ms linear;
  }
  .halo::before {
    content: ''; position: absolute; inset: 0; border-radius: 50%;
    background: radial-gradient(circle, color-mix(in srgb, var(--orb-hue) 45%, transparent), transparent 70%);
    animation: breathe calc(var(--anim-dur-slow, 240ms) * 16) ease-in-out infinite;
  }

  .core {
    position: relative; z-index: 1;
    width: calc(var(--s) * 0.605); height: calc(var(--s) * 0.605); border-radius: calc(var(--s) * 0.197);
    display: grid; place-items: center; color: var(--text-on-accent);
    background: linear-gradient(140deg, var(--orb-hue), color-mix(in srgb, var(--orb-hue) 45%, var(--info)));
    box-shadow: 0 calc(var(--s) * 0.105) calc(var(--s) * 0.29) color-mix(in srgb, var(--orb-hue) 38%, transparent);
    transform: scale(calc(1 + var(--lvl) * 0.1));
    transition: transform 70ms linear;
  }

  .orbit { position: absolute; inset: 0; animation: spin calc(var(--anim-dur-slow, 240ms) * 40) linear infinite; }
  .orbit i {
    position: absolute; top: calc(var(--s) * 0.026); left: 50%;
    width: clamp(4px, calc(var(--s) * 0.066), 9px); height: clamp(4px, calc(var(--s) * 0.066), 9px);
    margin-left: calc(clamp(4px, calc(var(--s) * 0.066), 9px) / -2);
    border-radius: 50%; background: var(--orb-hue); box-shadow: 0 0 8px var(--orb-hue);
  }
  .o2 { inset: calc(var(--s) * 0.118); animation-duration: calc(var(--anim-dur-slow, 240ms) * 62); animation-direction: reverse; }
  .o2 i { background: var(--info); box-shadow: 0 0 8px var(--info); width: clamp(3px, calc(var(--s) * 0.053), 7px); height: clamp(3px, calc(var(--s) * 0.053), 7px); margin-left: calc(clamp(3px, calc(var(--s) * 0.053), 7px) / -2); }

  /* Listening: rings leave the core and fade out. */
  .ring {
    position: absolute; inset: calc(var(--s) * 0.197); border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--orb-hue) 70%, transparent);
    opacity: 0; display: none; pointer-events: none;
  }
  [data-state='listening'] .ring { display: block; animation: ripple calc(var(--anim-dur-slow, 240ms) * 9) ease-out infinite; }
  [data-state='listening'] .r2 { animation-delay: calc(var(--anim-dur-slow, 240ms) * 4.5); }

  /* Thinking: a thin arc turns around the core. */
  .arc {
    position: absolute; inset: calc(var(--s) * 0.095); border-radius: 50%; opacity: 0; pointer-events: none;
    background: conic-gradient(from 0deg, transparent 0 55%, var(--orb-hue) 85%, var(--info) 100%);
    -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1px));
    mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1px));
    transition: opacity calc(var(--anim-dur-slow, 240ms) * 1.5) ease;
  }
  [data-state='thinking'] .arc { opacity: 0.9; animation: spin calc(var(--anim-dur-slow, 240ms) * 8) linear infinite; }
  [data-state='thinking'] .o1 { animation-duration: calc(var(--anim-dur-slow, 240ms) * 22); }
  [data-state='thinking'] .o2 { animation-duration: calc(var(--anim-dur-slow, 240ms) * 34); }
  [data-state='thinking'] .halo::before,
  [data-state='speaking'] .halo::before { animation-duration: calc(var(--anim-dur-slow, 240ms) * 9); }

  @keyframes breathe {
    0%, 100% { transform: scale(0.92); opacity: 0.7; }
    50%      { transform: scale(1.12); opacity: 1; }
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes ripple {
    0%   { transform: scale(0.9); opacity: 0.6; }
    100% { transform: scale(2.1); opacity: 0; }
  }

  .still .halo, .still .core { transition: none; transform: none; opacity: 1; }
  .still .halo::before, .still .orbit, .still .ring, .still .arc { animation: none; }

  @media (prefers-reduced-motion: reduce) {
    .halo, .core { transition: none; transform: none; opacity: 1; }
    .halo::before, .orbit, .ring, .arc { animation: none !important; }
  }
</style>
