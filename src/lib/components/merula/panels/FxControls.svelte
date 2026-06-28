<script lang="ts">
  /**
   * Per-track FX editor — parametric EQ + compressor strip inserts, shown in the
   * Inspector for the selected track. Like room/delay these are **code-first**:
   * the controls reflect the `.eq(...)` / `.comp(...)` literals in the source and
   * commit straight back to them (debounced for knob drags, immediate for adding /
   * removing a band). The EQ response curve is drawn from the bands so the user
   * reads the combined shape; band markers sit on it.
   *
   * Imports only shared/ui + merula-local. Knob-driven (keyboard-accessible) — the
   * curve is display-only.
   */
  import { Plus, X } from 'lucide-svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { mixerStore } from '../stores/mixer.svelte';
  import type { EqKind } from '../editor/merula-edit';
  import { eqResponseDb, logFreqAxis, freqToX, xToFreq, freqLabel } from './merula-eq-response';

  let { index, color }: { index: number; color: string } = $props();

  const bands = $derived(mixerStore.eq(index));
  const comp = $derived(mixerStore.comp(index));
  const compActive = $derived(mixerStore.compActive(index));
  const compCalc = $derived(mixerStore.compCalculated(index));

  // EQ response curve geometry.
  const W = 240, H = 84, DB = 18; // ± dB shown
  const AXIS = logFreqAxis(110);
  const yOf = (db: number) => H / 2 - (Math.max(-DB, Math.min(DB, db)) / DB) * (H / 2 - 4);
  const curve = $derived.by(() => {
    const ys = eqResponseDb(bands, AXIS);
    return AXIS.map((f, i) => `${(freqToX(f) * W).toFixed(1)},${yOf(ys[i]).toFixed(1)}`).join(' ');
  });
  // Gridlines at 100 / 1k / 10k for orientation.
  const GRID = [100, 1000, 10_000].map((f) => freqToX(f) * W);

  const KINDS: { k: EqKind; label: string }[] = [
    { k: 'peak', label: 'Pk' }, { k: 'low', label: 'Lo' }, { k: 'high', label: 'Hi' },
    { k: 'hpf', label: 'HP' }, { k: 'lpf', label: 'LP' },
  ];
  const usesGain = (k: EqKind) => k === 'peak' || k === 'low' || k === 'high';

  const CB = { threshold: -18, ratio: 4, attack: 0.005, release: 0.1, makeup: 0, knee: 6 };
  const ms = (s: number) => `${Math.round(s * 1000)}ms`;
</script>

<div class="fx" style="--c: {color}">
  <!-- ── Parametric EQ ──────────────────────────────────────────────────────── -->
  <div class="fx-head">
    <span class="fx-title">EQ</span>
    <button class="fx-add" use:tooltip={'Add EQ band'} aria-label="Add EQ band" onclick={() => mixerStore.addEqBand(index)}>
      <Plus size={12} /> band
    </button>
  </div>

  {#if bands.length}
    <svg class="eq-curve" viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
      {#each GRID as gx}<line x1={gx} y1="0" x2={gx} y2={H} class="grid" />{/each}
      <line x1="0" y1={H / 2} x2={W} y2={H / 2} class="grid zero" />
      <polyline points={curve} class="resp" />
      {#each bands as b}
        {#if !b.calculated}
          <circle cx={freqToX(b.freq) * W} cy={yOf(usesGain(b.kind) ? b.gainDb : 0)} r="3.5" class="dot" />
        {/if}
      {/each}
    </svg>

    {#each bands as b, k (k)}
      <div class="band" class:calc={b.calculated}>
        <div class="seg" role="group" aria-label="band {k + 1} type">
          {#each KINDS as opt}
            <button class="seg-btn" class:on={b.kind === opt.k} disabled={b.calculated}
                    aria-pressed={b.kind === opt.k}
                    onclick={() => mixerStore.setEqBand(index, k, { kind: opt.k })}>{opt.label}</button>
          {/each}
        </div>
        <div class="bknob">
          <Knob value={freqToX(b.freq)} min={0} max={1} size={26} color={color} disabled={b.calculated}
                label="freq" ariaLabel="band {k + 1} frequency"
                onchange={(v) => mixerStore.setEqBand(index, k, { freq: xToFreq(v) })} />
          <span class="bval">{freqLabel(b.freq)}</span>
        </div>
        <div class="bknob">
          <Knob value={b.gainDb} min={-DB} max={DB} bipolar default={0} size={26} color={color}
                disabled={b.calculated || !usesGain(b.kind)}
                label="gain" ariaLabel="band {k + 1} gain"
                onchange={(v) => mixerStore.setEqBand(index, k, { gainDb: v })} />
          <span class="bval">{usesGain(b.kind) ? `${b.gainDb.toFixed(1)}` : '—'}</span>
        </div>
        <div class="bknob">
          <Knob value={b.q} min={0.1} max={10} default={0.7} size={26} color={color} disabled={b.calculated}
                label="Q" ariaLabel="band {k + 1} Q"
                onchange={(v) => mixerStore.setEqBand(index, k, { q: v })} />
          <span class="bval">{b.q.toFixed(2)}</span>
        </div>
        <button class="fx-rm" use:tooltip={b.calculated ? 'Calculated band — edit in source' : 'Remove band'}
                aria-label="Remove band {k + 1}" disabled={b.calculated}
                onclick={() => mixerStore.removeEqBand(index, k)}><X size={11} /></button>
      </div>
    {/each}
  {:else}
    <p class="fx-empty">No EQ — <button class="fx-link" onclick={() => mixerStore.addEqBand(index)}>add a band</button> to shape this track.</p>
  {/if}

  <!-- ── Compressor ─────────────────────────────────────────────────────────── -->
  <div class="fx-head">
    <span class="fx-title">Compressor</span>
    {#if compActive && !compCalc}
      <button class="fx-rm" use:tooltip={'Remove compressor'} aria-label="Remove compressor" onclick={() => mixerStore.removeComp(index)}><X size={11} /></button>
    {/if}
  </div>

  {#if compCalc}
    <p class="fx-empty">Compressor is a calculated value — edit it in the source.</p>
  {:else if comp && compActive}
    <div class="comp-grid">
      <div class="bknob">
        <Knob value={comp.threshold} min={-60} max={0} default={CB.threshold} size={26} color={color}
              label="thr" ariaLabel="threshold" onchange={(v) => mixerStore.setCompParam(index, 'threshold', v)} />
        <span class="bval">{comp.threshold.toFixed(0)}dB</span>
      </div>
      <div class="bknob">
        <Knob value={comp.ratio} min={1} max={20} default={CB.ratio} size={26} color={color}
              label="ratio" ariaLabel="ratio" onchange={(v) => mixerStore.setCompParam(index, 'ratio', v)} />
        <span class="bval">{comp.ratio.toFixed(1)}:1</span>
      </div>
      <div class="bknob">
        <Knob value={comp.attack} min={0} max={0.1} default={CB.attack} size={26} color={color}
              label="atk" ariaLabel="attack" onchange={(v) => mixerStore.setCompParam(index, 'attack', v)} />
        <span class="bval">{ms(comp.attack)}</span>
      </div>
      <div class="bknob">
        <Knob value={comp.release} min={0} max={1} default={CB.release} size={26} color={color}
              label="rel" ariaLabel="release" onchange={(v) => mixerStore.setCompParam(index, 'release', v)} />
        <span class="bval">{ms(comp.release)}</span>
      </div>
      <div class="bknob">
        <Knob value={comp.makeup} min={0} max={24} default={CB.makeup} size={26} color={color}
              label="gain" ariaLabel="make-up gain" onchange={(v) => mixerStore.setCompParam(index, 'makeup', v)} />
        <span class="bval">{comp.makeup.toFixed(0)}dB</span>
      </div>
      <div class="bknob">
        <Knob value={comp.knee} min={0} max={18} default={CB.knee} size={26} color={color}
              label="knee" ariaLabel="knee" onchange={(v) => mixerStore.setCompParam(index, 'knee', v)} />
        <span class="bval">{comp.knee.toFixed(0)}dB</span>
      </div>
    </div>
  {:else}
    <p class="fx-empty">No compressor — <button class="fx-link" onclick={() => mixerStore.addComp(index)}>add one</button> to glue this track.</p>
  {/if}
</div>

<style>
  .fx { padding: 2px 12px 4px; }

  .fx-head { display: flex; align-items: center; justify-content: space-between; padding: 10px 0 6px; }
  .fx-title { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }

  .fx-add {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 2px 7px 2px 5px; border: 1px solid var(--border-subtle);
    background: var(--bg-input); border-radius: var(--radius-sm);
    color: var(--text-secondary); font-size: 10.5px; cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .fx-add:hover { background: var(--bg-hover); color: var(--text-primary); }
  .fx-add:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }

  .eq-curve {
    width: 100%; height: 84px; display: block;
    background: var(--bg-input); border-radius: var(--radius-sm);
    margin-bottom: 6px;
  }
  .grid { stroke: var(--border-subtle); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .grid.zero { stroke: var(--border); }
  .resp { fill: none; stroke: var(--c); stroke-width: 2; vector-effect: non-scaling-stroke; }
  .dot { fill: var(--bg-base); stroke: var(--c); stroke-width: 2; }

  .band { display: flex; align-items: flex-start; gap: 7px; padding: 4px 0; }
  .band.calc { opacity: 0.6; }

  .seg { display: flex; flex-direction: column; gap: 2px; flex-shrink: 0; }
  .seg-btn {
    width: 26px; padding: 2px 0; font-size: 9px; font-weight: 600;
    border: 1px solid var(--border-subtle); background: var(--bg-input);
    color: var(--text-muted); border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .seg-btn:hover:not(:disabled) { color: var(--text-primary); }
  .seg-btn.on { background: var(--c); color: #1a1b1e; border-color: transparent; }
  .seg-btn:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .seg-btn:disabled { cursor: default; }

  .bknob { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .bval { font-size: 9px; color: var(--text-muted); font-family: var(--font-code); line-height: 1; }

  .comp-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px 4px; padding: 2px 0 4px; justify-items: center; }

  .fx-rm {
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 18px; flex-shrink: 0; margin-top: 2px;
    border: 1px solid var(--border-subtle); background: var(--bg-input);
    color: var(--text-muted); border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .fx-rm:hover:not(:disabled) { background: var(--error); color: #fff; border-color: transparent; }
  .fx-rm:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .fx-rm:disabled { cursor: default; opacity: 0.5; }

  .fx-empty { margin: 2px 0 4px; font-size: 11px; color: var(--text-muted); line-height: 1.4; }
  .fx-link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; font: inherit; text-decoration: underline; }
  .fx-link:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); border-radius: 2px; }
</style>
