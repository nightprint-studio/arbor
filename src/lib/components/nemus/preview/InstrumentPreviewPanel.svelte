<script lang="ts">
  /**
   * Instrument preview — a docked BOTTOM panel (audition keyboard + controls).
   *
   * Shows the instrument picked from the Sound bank (preview button) or the editor
   * (Ctrl/Cmd+click on an `inst("…")` / `s("…")` name), driven by `previewStore`.
   * The controls (knobs + scale/root + a free-form chain) plus the pressed key are
   * compiled into a tiny `.nemus` snippet (see `snippet.ts`) and sent to
   * `nemus_audition_expr`, which evaluates it with the real language and plays one
   * cycle on a dedicated bus that bypasses the song mixer. So the whole language —
   * notes, chords, scales, any effect — drives the preview, with no per-param IPC.
   *
   * Imports only shared/ui (+ the tooltip action) + nemus-local.
   */
  import { Piano, ChevronLeft, ChevronRight, Play, Package } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import PianoKeyboard from './PianoKeyboard.svelte';
  import { buildSnippet, type PreviewControls } from './snippet';
  import { nemusStore } from '../nemus-store.svelte';
  import { previewStore } from '../stores/preview.svelte';
  import { nemusAuditionExpr } from '$lib/ipc/nemus';

  const inst = $derived(previewStore.inst);
  const pitched = $derived(!!inst && inst.kind !== 'sample');
  const kindLabel = $derived(
    !inst ? ''
    : inst.kind === 'synth' ? 'Synth preset'
    : inst.kind === 'sfz' ? 'Multisample · SFZ'
    : 'Sample one-shot',
  );

  // Per-note knobs — persist across hits / instrument changes for this session.
  let gain  = $state(1.0);
  let vel   = $state(0.8);
  let room  = $state(0.0);
  let speed = $state(1.0);
  let pan   = $state(0.5);

  // Articulation (multisample); '' = default voice.
  let art = $state('');

  // Scale: '' = chromatic piano (note names); else a degree keyboard + `.scale()`.
  let scaleMode = $state('');
  let root = $state('c');

  const SCALES = [
    { value: '', label: 'Chromatic' },
    { value: 'major', label: 'Major' }, { value: 'minor', label: 'Minor' },
    { value: 'dorian', label: 'Dorian' }, { value: 'phrygian', label: 'Phrygian' },
    { value: 'lydian', label: 'Lydian' }, { value: 'mixolydian', label: 'Mixolydian' },
    { value: 'locrian', label: 'Locrian' }, { value: 'harmonicminor', label: 'Harmonic minor' },
    { value: 'melodicminor', label: 'Melodic minor' },
    { value: 'majpent', label: 'Major pent' }, { value: 'minpent', label: 'Minor pent' },
  ];
  const ROOTS = ['c', 'cs', 'd', 'ds', 'e', 'f', 'fs', 'g', 'gs', 'a', 'as', 'b'].map((r) => ({
    value: r,
    label: r.length === 2 ? `${r[0].toUpperCase()}♯` : r.toUpperCase(),
  }));

  // Free-form DSL tail (e.g. `.lpf(800).crush(4)`), appended verbatim.
  let chain = $state('');

  // Keyboard window: 4 octaves; chromatic counts in MIDI, scale in degrees.
  const OCTAVES = 4;
  let chromFrom = $state(48); // C3
  let degBase = $state(0);    // scale degree of the leftmost key
  const kbFrom = $derived(scaleMode ? degBase : chromFrom);
  const rangeLabel = $derived(
    scaleMode
      ? `deg ${degBase}…${degBase + OCTAVES * 7 - 1}`
      : `C${Math.floor(chromFrom / 12) - 1}–C${Math.floor(chromFrom / 12) - 1 + OCTAVES}`,
  );
  function shift(delta: number) {
    if (scaleMode) degBase = Math.max(0, Math.min(degBase + delta * 7, 21));
    else chromFrom = Math.max(0, Math.min(chromFrom + delta * 12, 108 - OCTAVES * 12));
  }

  function controls(): PreviewControls {
    return { gain, vel, room, speed, pan, art, scale: scaleMode, root, chain };
  }
  function fire(trigger: { note?: number | null; degree?: number | null }) {
    if (!inst) return;
    nemusAuditionExpr(buildSnippet(inst, controls(), trigger)).catch(() => { /* engine not ready */ });
  }
  function play(value: number) { fire(scaleMode ? { degree: value } : { note: value }); }
</script>

<div class="ipp">
  <BottomPanelHeader title="Preview" onClose={() => nemusStore.toggleBottom('preview')}>
    {#snippet icon()}<Piano size={13} />{/snippet}
  </BottomPanelHeader>

  {#if !inst}
    <EmptyState message="Preview a voice from the Sound bank, or Ctrl-click an inst(…) / s(…) name in the editor." />
  {:else}
    <div class="ipp-body">
      <div class="ipp-top">
        <div class="ipp-id">
          <div class="ipp-name">{inst.name}</div>
          <div class="ipp-meta">
            <span class="ipp-kind">{kindLabel}</span>
            {#if inst.pack_name}<span class="ipp-pack"><Package size={10} />{inst.pack_name}</span>{/if}
          </div>
          {#if inst.articulations.length}
            <div class="ipp-arts">
              <button type="button" class="ipp-chip" class:on={art === ''} onclick={() => (art = '')}>default</button>
              {#each inst.articulations as a (a)}
                <button type="button" class="ipp-chip" class:on={art === a} onclick={() => (art = a)}>{a}</button>
              {/each}
            </div>
          {/if}
        </div>

        <div class="ipp-knobs">
          <Knob bind:value={gain}  min={0} max={1.5} default={1}   size={32} label="Gain"   ariaLabel="Gain" />
          <Knob bind:value={vel}   min={0} max={1}   default={0.8} size={32} label="Vel"    ariaLabel="Velocity" />
          <Knob bind:value={room}  min={0} max={1}   default={0}   size={32} label="Reverb" ariaLabel="Reverb" />
          <Knob bind:value={speed} min={0.25} max={2} default={1}  size={32} label="Speed"  ariaLabel="Speed" />
          <Knob bind:value={pan}   min={0} max={1}   default={0.5} size={32} bipolar label="Pan" ariaLabel="Pan" />
        </div>
      </div>

      {#if pitched}
        <div class="ipp-row">
          <label class="ipp-field">
            <span class="ipp-flabel">Scale</span>
            <Select value={scaleMode} options={SCALES} onchange={(v) => (scaleMode = v)} />
          </label>
          {#if scaleMode}
            <label class="ipp-field ipp-narrow">
              <span class="ipp-flabel">Root</span>
              <Select value={root} options={ROOTS} onchange={(v) => (root = v)} />
            </label>
          {/if}
          <label class="ipp-field ipp-grow">
            <span class="ipp-flabel">Chain</span>
            <Input value={chain} oninput={(v) => (chain = v)} placeholder=".lpf(800).crush(4)" size="sm" block />
          </label>
        </div>
      {/if}

      <div class="ipp-play">
        {#if pitched}
          <div class="ipp-octave">
            <button type="button" class="ipp-oct-btn" use:tooltip={'Down an octave'}
                    aria-label="Down an octave" onclick={() => shift(-1)}><ChevronLeft size={14} /></button>
            <span class="ipp-range">{rangeLabel}</span>
            <button type="button" class="ipp-oct-btn" use:tooltip={'Up an octave'}
                    aria-label="Up an octave" onclick={() => shift(1)}><ChevronRight size={14} /></button>
          </div>
          <PianoKeyboard from={kbFrom} octaves={OCTAVES} mode={scaleMode ? 'scale' : 'chromatic'} onnote={(v) => play(v)} />
          <p class="ipp-hint">Click keys or drag to glide · focus the keyboard, then the Z… and Q… rows play two octaves (◀ ▶ shifts them).</p>
        {:else}
          <button type="button" class="ipp-trigger" onclick={() => fire({})}>
            <Play size={16} /><span>Play</span>
          </button>
          <p class="ipp-hint">A one-shot sample — plays at its native pitch.</p>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .ipp { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .ipp-body {
    flex: 1; min-height: 0; overflow: auto;
    display: flex; flex-direction: column; gap: 10px;
    padding: 8px 12px 12px;
  }

  .ipp-top { display: flex; align-items: flex-start; gap: 16px; flex-wrap: wrap; }

  .ipp-id { display: flex; flex-direction: column; gap: 5px; min-width: 0; flex: 1; }
  .ipp-name { font-family: var(--font-code); font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .ipp-meta { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .ipp-kind {
    font-size: 9px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--text-muted); background: var(--bg-overlay);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); padding: 1px 6px;
  }
  .ipp-pack { display: inline-flex; align-items: center; gap: 4px; font-size: 10px; color: var(--text-muted); }

  .ipp-arts { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 2px; }
  .ipp-chip {
    font-family: var(--font-code); font-size: 10.5px;
    padding: 2px 8px; border-radius: var(--radius-sm); cursor: pointer;
    color: var(--text-secondary); background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ipp-chip:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ipp-chip.on { background: var(--accent); color: var(--text-on-accent); border-color: transparent; }

  .ipp-knobs { display: flex; gap: 14px; flex-shrink: 0; }

  .ipp-row { display: flex; align-items: flex-end; gap: 10px; flex-wrap: wrap; }
  .ipp-field { display: flex; flex-direction: column; gap: 3px; min-width: 140px; }
  .ipp-field.ipp-narrow { min-width: 90px; }
  .ipp-field.ipp-grow { flex: 1; min-width: 180px; }
  .ipp-flabel {
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-disabled);
  }

  .ipp-play { display: flex; flex-direction: column; gap: 6px; }
  .ipp-octave { display: flex; align-items: center; gap: 8px; }
  .ipp-oct-btn {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 22px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    color: var(--text-secondary); cursor: pointer;
  }
  .ipp-oct-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .ipp-range {
    font-family: var(--font-code); font-size: 11px; color: var(--text-muted);
    min-width: 78px; text-align: center;
  }

  .ipp-hint { margin: 0; font-size: 11px; color: var(--text-muted); }

  .ipp-trigger {
    align-self: flex-start;
    display: inline-flex; align-items: center; gap: 8px;
    padding: 9px 22px; border-radius: var(--radius-md);
    background: var(--accent); color: var(--text-on-accent);
    border: none; cursor: pointer; font-size: 13px; font-weight: 600;
  }
  .ipp-trigger:hover { filter: brightness(1.08); }
</style>
