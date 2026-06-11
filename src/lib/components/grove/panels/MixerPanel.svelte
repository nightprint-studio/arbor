<script lang="ts">
  /**
   * Mixer — one strip per track, docked at the BOTTOM (Logic-style): a row of
   * compact channel strips with a real per-track stereo meter, gain + pan knobs
   * and mute/solo, plus a master strip at the end.
   *
   * Driven by the **real engine**: strips come from the shared arrangement query
   * (`mixerStore.tracks`, index-keyed), meters from `grove:meters`, and the
   * gain/pan knobs push **live ephemeral overrides** (`grove_set_track`, gate 2)
   * — the source stays authoritative, so every eval re-baselines the strips to
   * neutral. Mute/solo round-trip through the shared store (so the arrangement
   * headers + Inspector mirror them) and push the live audio override too.
   *
   * Room/send are per-event (code-first): there is no track-level audio command
   * for them yet (the future `grove_set_literal`), so those knobs are disabled.
   *
   * Imports only shared/ui (+ the tooltip action) + grove-local.
   */
  import { SlidersHorizontal, VolumeX, Headphones } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import PeakMeter from './PeakMeter.svelte';
  import { mixerStore, GAIN_UNITY, PAN_CENTER } from '../stores/mixer.svelte';
  import { metersStore, diagnosticsStore } from '../stores/engine.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';

  const tracks = $derived(mixerStore.tracks);

  // Re-query the arrangement + drop the live overrides on every eval (a fresh
  // diagnostics array is pushed each time). Keeps the mixer correct even when
  // the arrangement view is collapsed/unmounted; the query is debounced so the
  // arrangement view firing too only coalesces.
  $effect(() => {
    void diagnosticsStore.errors; // dep: reassigned on each eval
    arrangementStore.schedule();
    mixerStore.rebaseline();
  });

  const CODE_FIRST = 'Per-event (code-first) — set it in the source. A live track override is coming with grove_set_literal.';

  function panLabel(p: number): string {
    if (Math.abs(p - PAN_CENTER) < 0.02) return 'C';
    return p < PAN_CENTER ? `L${Math.round((PAN_CENTER - p) * 200)}` : `R${Math.round((p - PAN_CENTER) * 200)}`;
  }
</script>

<PanelShell title="Mixer" count={tracks.length}>
  {#snippet icon()}<SlidersHorizontal size={13} />{/snippet}

  {#if !tracks.length}
    <EmptyState message="No arrangement yet — Run a .grove file to see its mixer." />
  {:else}
    <div class="mix">
      {#each tracks as t (t.index)}
        {@const dimmed = mixerStore.isDimmed(t.index)}
        {@const muted = mixerStore.isMuted(t.index)}
        {@const soloed = mixerStore.isSoloed(t.index)}
        <div class="strip" class:selected={mixerStore.selectedIndex === t.index} style="--c: {t.color}">
          <button class="strip-name" use:tooltip={t.voice} onclick={() => mixerStore.select(t.index)}>
            <span class="dot"></span><span class="nm">{t.name}</span>
          </button>

          <div class="strip-body">
            <div class="meter"><PeakMeter peak={metersStore.peak(t.index)} {dimmed} /></div>
            <div class="kcol">
              <Knob value={mixerStore.gain(t.index)} default={GAIN_UNITY} size={32} color={t.color}
                    label="gain" ariaLabel="{t.name} gain" onchange={(v) => mixerStore.setGain(t.index, v)} />
              <span class="kval">{mixerStore.gain(t.index).toFixed(2)}</span>
            </div>
          </div>

          <div class="knobs-row">
            <div class="kcol">
              <Knob value={mixerStore.pan(t.index)} bipolar default={PAN_CENTER} size={24} color={t.color}
                    label="pan" ariaLabel="{t.name} pan" onchange={(v) => mixerStore.setPan(t.index, v)} />
              <span class="kval">{panLabel(mixerStore.pan(t.index))}</span>
            </div>
            <span use:tooltip={CODE_FIRST}><Knob value={0} disabled size={24} label="room" ariaLabel="{t.name} room (code-first)" /></span>
            <span use:tooltip={CODE_FIRST}><Knob value={0} disabled size={24} label="send" ariaLabel="{t.name} send (code-first)" /></span>
          </div>

          <div class="ms-row">
            <button class="ms" class:on={muted} use:tooltip={'Mute'} aria-label="{t.name} mute" aria-pressed={muted}
                    onclick={() => mixerStore.toggleMute(t.index)}><VolumeX size={11} /></button>
            <button class="ms solo" class:on={soloed} use:tooltip={'Solo'} aria-label="{t.name} solo" aria-pressed={soloed}
                    onclick={() => mixerStore.toggleSolo(t.index)}><Headphones size={11} /></button>
          </div>
        </div>
      {/each}

      <!-- Master strip -->
      <div class="strip master">
        <div class="strip-name master-name"><span class="nm">MASTER</span></div>
        <div class="strip-body">
          <div class="meter"><PeakMeter peak={metersStore.master} /></div>
          <div class="kcol">
            <Knob value={mixerStore.masterGain} default={GAIN_UNITY} size={32} color="var(--accent)"
                  label="gain" ariaLabel="Master gain" onchange={(v) => mixerStore.setMasterGain(v)} />
            <span class="kval">{mixerStore.masterGain.toFixed(2)}</span>
          </div>
        </div>
        <div class="ms-row"><span class="dsp" use:tooltip={'DSP load'}>{Math.round(metersStore.dspLoad * 100)}%</span></div>
      </div>
    </div>
  {/if}
</PanelShell>

<style>
  .mix { display: flex; gap: 4px; padding: 6px 8px; height: 100%; overflow-x: auto; align-items: stretch; }

  .strip {
    display: flex; flex-direction: column; align-items: center; gap: 4px;
    width: 96px; flex-shrink: 0;
    padding: 5px 4px 6px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    box-shadow: inset 0 0 0 1px transparent;
    transition: box-shadow var(--transition-fast);
  }
  .strip.selected { box-shadow: inset 0 0 0 1px var(--c); }
  .strip.master { background: color-mix(in srgb, var(--accent) 8%, var(--bg-elevated)); margin-left: 4px; }

  .strip-name {
    display: flex; align-items: center; gap: 4px; max-width: 100%;
    background: transparent; border: none; padding: 1px 2px; border-radius: var(--radius-sm);
    cursor: pointer; color: var(--text-primary);
  }
  .strip-name:hover { background: var(--bg-hover); }
  .strip-name:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .strip-name.master-name { cursor: default; }
  .strip-name.master-name:hover { background: transparent; }
  .dot { width: 7px; height: 7px; border-radius: 2px; background: var(--c); flex-shrink: 0; }
  .nm {
    font-size: 11px; font-weight: 600;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .master-name .nm { font-size: 9.5px; letter-spacing: 0.6px; color: var(--text-muted); }

  .strip-body { display: flex; align-items: center; justify-content: center; gap: 8px; min-height: 52px; }
  .meter { height: 46px; flex-shrink: 0; }

  .knobs-row { display: flex; align-items: flex-start; justify-content: center; gap: 5px; }
  .kcol { display: flex; flex-direction: column; align-items: center; gap: 1px; }
  .kval { font-size: 9px; color: var(--text-muted); font-family: var(--font-code); line-height: 1; }

  .ms-row { display: flex; gap: 3px; align-items: center; min-height: 18px; }
  .ms {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 18px; border: 1px solid var(--border-subtle);
    background: var(--bg-input); border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ms:hover { color: var(--text-primary); }
  .ms:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .ms.on { background: var(--warning); color: #1a1b1e; border-color: transparent; }
  .ms.solo.on { background: var(--info); color: #fff; }

  .dsp { font-size: 9.5px; font-family: var(--font-code); color: var(--text-muted); }
</style>
