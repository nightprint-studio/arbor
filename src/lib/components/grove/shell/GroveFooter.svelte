<script lang="ts">
  /**
   * Grove footer — a grove-specific status strip: transport position · cps ·
   * active voices · DSP load · sample rate/buffer · cursor row:col · render
   * state. All mocked; the voices/DSP figures idle when stopped.
   */
  import { Activity, Cpu, AudioWaveform, MapPin } from 'lucide-svelte';
  import { groveStore } from '../grove-store.svelte';

  // Fake but plausible live figures while running.
  const voices = $derived(groveStore.running ? 14 : 0);
  const dsp = $derived(groveStore.running ? 23 : 0);
  // A slowly-advancing cycle/position label while running.
  let posCycle = $state(0);
  let posFrac = $state(0);
  $effect(() => {
    if (!groveStore.running) return;
    let raf = 0; let last = performance.now();
    const tick = (now: number) => {
      const dt = now - last; last = now;
      posFrac += dt / 2000;            // cps 0.5 → 2s/cycle
      while (posFrac >= 1) { posFrac -= 1; posCycle += 1; }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
  const posLabel = $derived(`${posCycle}.${Math.floor(posFrac * 4) + 1}`);
</script>

<div class="gf">
  <span class="gf-item gf-pos">
    <Activity size={12} />
    <span class:live={groveStore.running}>{posLabel}</span>
  </span>
  <span class="gf-item">cps 0.5</span>
  <span class="gf-sep"></span>
  <span class="gf-item"><AudioWaveform size={12} /> {voices} voices</span>
  <span class="gf-item"><Cpu size={12} /> {dsp}% DSP</span>
  <span class="gf-sep"></span>
  <span class="gf-item">48 kHz · 512</span>

  <span class="gf-spacer"></span>

  <span class="gf-item"><MapPin size={12} /> Ln 15, Col 3</span>
  <span class="gf-sep"></span>
  <span class="gf-item gf-render">{groveStore.running ? 'playing' : 'idle'}</span>
</div>

<style>
  .gf {
    display: flex; align-items: center; gap: 12px;
    height: 24px; flex-shrink: 0;
    padding: 0 12px;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-subtle);
    font-size: 11px; color: var(--text-muted);
    user-select: none;
  }
  .gf-item { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .gf-item :global(svg) { color: var(--text-disabled); }
  .gf-pos { font-variant-numeric: tabular-nums; }
  .gf-pos .live { color: var(--success); font-weight: 600; }
  .gf-spacer { flex: 1; }
  .gf-sep { width: 1px; height: 12px; background: var(--border-subtle); }
  .gf-render { text-transform: uppercase; letter-spacing: 0.4px; font-size: 10px; }
</style>
