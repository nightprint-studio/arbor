<script lang="ts">
  /**
   * Grove footer — a grove-specific status strip wired to the live engine
   * telemetry: transport position · cps · active voices · DSP load · sample
   * rate · cursor row:col · render state. All figures come from the transport /
   * meters streams (no RAF, no mock); they idle naturally when stopped because
   * the engine stops emitting movement.
   */
  import { Activity, Cpu, AudioWaveform, MapPin, AlertTriangle } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { transportStore, metersStore, audioErrorStore } from '../stores/engine.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { projectStore } from '../stores/project.svelte';

  // cps: 2-3 significant digits, trimming trailing zeros (0.5, 0.35, 1.25…).
  const cpsLabel = $derived(Number(transportStore.cps.toPrecision(3)).toString());
  const dspPct   = $derived(Math.round(metersStore.dspLoad * 100));
  const srLabel  = $derived(`${transportStore.sampleRate / 1000} kHz`);
</script>

<div class="gf">
  <span class="gf-item gf-pos">
    <Activity size={12} />
    <span class:live={transportStore.playing}>{transportStore.position}</span>
  </span>
  <span class="gf-item">cps {cpsLabel}</span>
  <span class="gf-sep"></span>
  <span class="gf-item"><AudioWaveform size={12} /> {metersStore.voices} voices</span>
  <span class="gf-item"><Cpu size={12} /> {dspPct}% DSP</span>
  <span class="gf-sep"></span>
  {#if audioErrorStore.message}
    <span class="gf-item gf-error" use:tooltip={audioErrorStore.message}>
      <AlertTriangle size={12} /> audio error
    </span>
  {:else}
    <span class="gf-item">{srLabel}</span>
  {/if}

  <span class="gf-spacer"></span>

  {#if projectStore.activeFilePath}
    <span class="gf-item"><MapPin size={12} /> Ln {groveStore.caretLine}, Col {groveStore.caretCol}</span>
    <span class="gf-sep"></span>
  {/if}
  <span class="gf-item gf-render">{transportStore.playing ? 'playing' : 'idle'}</span>
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
  .gf-error { color: var(--error); }
  .gf-error :global(svg) { color: var(--error); }
  .gf-spacer { flex: 1; }
  .gf-sep { width: 1px; height: 12px; background: var(--border-subtle); }
  .gf-render { text-transform: uppercase; letter-spacing: 0.4px; font-size: 10px; }
</style>
