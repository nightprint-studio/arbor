<script lang="ts">
  /**
   * Tempo control — a compact BPM readout with nudge (−/+) and a tap target,
   * for matching / feeling a tempo live. Drives `tempoStore`, which pushes a live
   * cps override to the engine (released on the next eval). Fully keyboard
   * reachable: every control is a real <button> (Tab to focus, Space/Enter to
   * act — so Tap works by pressing Space in rhythm).
   */
  import { Minus, Plus } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { tempoStore, bpmToCps } from '../stores/tempo.svelte';

  // ±1 BPM per nudge (Shift = coarse ±5).
  const NUDGE = 1;
  const NUDGE_COARSE = 5;

  const bpm = $derived(tempoStore.bpm);
  const overridden = $derived(tempoStore.overridden);
  const bpmLabel = $derived(fmtBpm(bpm));
  const cpsLabel = $derived(Number(bpmToCps(bpm).toPrecision(3)).toString());

  function fmtBpm(v: number): string {
    const r = Math.round(v * 10) / 10;
    return Number.isInteger(r) ? String(r) : r.toFixed(1);
  }

  function nudge(sign: number, e: MouseEvent | KeyboardEvent) {
    const step = (e.shiftKey ? NUDGE_COARSE : NUDGE) * sign;
    tempoStore.nudge(step);
  }

  // The readout doubles as a reset affordance once a live override is in effect.
  const readoutTip = $derived(
    overridden
      ? `Tempo ${bpmLabel} BPM (${cpsLabel} cps) — live override · click to follow the score again`
      : `Tempo ${bpmLabel} BPM (${cpsLabel} cps) — from the score`,
  );
</script>

<div class="tempo" role="group" aria-label="Tempo">
  <button
    class="t-btn"
    type="button"
    aria-label="Nudge tempo down"
    use:tooltip={{ content: 'Nudge tempo −1 BPM', description: 'Shift for ±5 BPM. A live override, released on the next run.' }}
    onclick={(e) => nudge(-1, e)}
  >
    <Minus size={13} />
  </button>

  <button
    class="t-read"
    class:on={overridden}
    type="button"
    aria-label={overridden ? 'Reset tempo to the score' : 'Tempo'}
    use:tooltip={readoutTip}
    onclick={() => tempoStore.reset()}
  >
    <span class="bpm">{bpmLabel}</span><span class="unit">BPM</span>
  </button>

  <button
    class="t-btn"
    type="button"
    aria-label="Nudge tempo up"
    use:tooltip={{ content: 'Nudge tempo +1 BPM', description: 'Shift for ±5 BPM. A live override, released on the next run.' }}
    onclick={(e) => nudge(1, e)}
  >
    <Plus size={13} />
  </button>

  <button
    class="t-tap"
    type="button"
    aria-label="Tap tempo"
    use:tooltip={{ content: 'Tap tempo', description: 'Tap in time (focus + Space works too) — sets the tempo from your taps.' }}
    onclick={() => tempoStore.tap()}
  >
    TAP
  </button>
</div>

<style>
  .tempo {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    height: 100%;
    padding: 0 2px;
  }
  .t-btn,
  .t-read,
  .t-tap {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 22px;
    flex-shrink: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .t-btn { width: 20px; }
  .t-btn:hover { background: var(--bg-hover); color: var(--text-secondary); }

  .t-read {
    gap: 3px;
    padding: 0 5px;
    font-variant-numeric: tabular-nums;
  }
  .t-read .bpm { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-secondary); }
  .t-read .unit { font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.04em; color: var(--text-disabled); }
  /* Following the score (no override): plain readout, no hover affordance. */
  .t-read:not(.on) { cursor: default; }
  /* Overridden: the readout becomes an accented reset button. */
  .t-read.on .bpm { color: var(--accent); }
  .t-read.on:hover { background: color-mix(in srgb, var(--accent) 16%, transparent); }

  .t-tap {
    width: 30px;
    margin-left: 2px;
    font-size: var(--font-size-3xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--text-secondary);
    background: var(--bg-hover);
  }
  .t-tap:hover { background: color-mix(in srgb, var(--accent) 20%, transparent); color: var(--accent); }
  .t-tap:active { background: color-mix(in srgb, var(--accent) 32%, transparent); }

  .t-btn:focus-visible,
  .t-read:focus-visible,
  .t-tap:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent);
  }
</style>
