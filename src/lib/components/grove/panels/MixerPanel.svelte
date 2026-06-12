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
   * `room` is a code-first knob (seeded from the source literal, commits back to
   * it); `delay`'s three params live in the Inspector. gain/pan keep their live
   * override AND write through to the source on a debounce — there's no explicit
   * commit button, the value lands in the `.grove` on its own once the drag rests.
   *
   * Mute writes `.gain(0)` into the source (unmute restores the pre-mute gain);
   * when a track's gain is a calculated argument it can't be rewritten, so mute is
   * live-only there and the strip flags it.
   *
   * Imports only shared/ui (+ the tooltip action) + grove-local.
   */
  import { SlidersHorizontal, VolumeX, Headphones } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import PeakMeter from './PeakMeter.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { mixerStore, GAIN_UNITY, PAN_CENTER } from '../stores/mixer.svelte';
  import { metersStore, diagnosticsStore } from '../stores/engine.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';
  import { controlsStore } from '../stores/controls.svelte';

  const tracks = $derived(mixerStore.tracks);

  // Re-query the arrangement, re-parse the source controls + drop the live
  // overrides on every eval (a fresh diagnostics array is pushed each time).
  // Keeps the mixer correct even when the arrangement view is collapsed; the
  // query/parse are debounced so the arrangement view firing too only coalesces.
  $effect(() => {
    void diagnosticsStore.errors; // dep: reassigned on each eval
    arrangementStore.schedule();
    controlsStore.schedule();
    mixerStore.rebaseline();
  });

  const ROOM_CALC = 'Room is a calculated value here — edit it in the source.';
  const MUTE_CALC = 'Muted live only — gain is a calculated value, so .gain(0) can’t be written to the source.';

  function panLabel(p: number): string {
    if (Math.abs(p - PAN_CENTER) < 0.02) return 'C';
    return p < PAN_CENTER ? `L${Math.round((PAN_CENTER - p) * 200)}` : `R${Math.round((p - PAN_CENTER) * 200)}`;
  }
</script>

<div class="mixer-root">
  <BottomPanelHeader title="Mixer" count={tracks.length} onClose={() => groveStore.toggleBottom('mixer')}>
    {#snippet icon()}<SlidersHorizontal size={13} />{/snippet}
  </BottomPanelHeader>

  <div class="mixer-body">
  {#if !tracks.length}
    <EmptyState message="No arrangement yet — Run a .grove file to see its mixer." />
  {:else}
    <div class="mix">
      {#each tracks as t (t.index)}
        {@const dimmed = mixerStore.isDimmed(t.index)}
        {@const muted = mixerStore.isMuted(t.index)}
        {@const soloed = mixerStore.isSoloed(t.index)}
        {@const muteCalc = muted && mixerStore.gainCalculated(t.index)}
        <div class="strip" class:selected={mixerStore.selectedIndex === t.index} style="--c: {t.color}">
          <button class="strip-name" use:tooltip={t.voice} onclick={() => mixerStore.select(t.index)}>
            <span class="dot"></span><span class="nm">{t.name}</span>
          </button>

          <div class="fader">
            <div class="meter"><PeakMeter peak={metersStore.peak(t.index)} {dimmed} /></div>
            <div class="kcol gain-col">
              <Knob value={mixerStore.gain(t.index)} default={GAIN_UNITY} size={36} color={t.color}
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
            {#if mixerStore.roomCalculated(t.index)}
              <span use:tooltip={ROOM_CALC}><Knob value={0} disabled size={24} label="room" ariaLabel="{t.name} room (calculated)" /></span>
            {:else}
              <div class="kcol">
                <Knob value={mixerStore.room(t.index)} default={0} size={24} color={t.color}
                      label="room" ariaLabel="{t.name} room" onchange={(v) => mixerStore.setRoom(t.index, v)} />
                <span class="kval">{mixerStore.room(t.index).toFixed(2)}</span>
              </div>
            {/if}
          </div>

          <div class="ms-row">
            <button class="ms" class:on={muted} class:calc={muteCalc}
                    use:tooltip={muteCalc ? MUTE_CALC : 'Mute'} aria-label="{t.name} mute" aria-pressed={muted}
                    onclick={() => mixerStore.toggleMute(t.index)}><VolumeX size={11} /></button>
            <button class="ms solo" class:on={soloed} use:tooltip={'Solo'} aria-label="{t.name} solo" aria-pressed={soloed}
                    onclick={() => mixerStore.toggleSolo(t.index)}><Headphones size={11} /></button>
          </div>
        </div>
      {/each}

      <!-- Master strip -->
      <div class="strip master">
        <div class="strip-name master-name"><span class="nm">MASTER</span></div>
        <div class="fader">
          <div class="meter"><PeakMeter peak={metersStore.master} /></div>
          <div class="kcol gain-col">
            <Knob value={mixerStore.masterGain} default={GAIN_UNITY} size={36} color="var(--accent)"
                  label="gain" ariaLabel="Master gain" onchange={(v) => mixerStore.setMasterGain(v)} />
            <span class="kval">{mixerStore.masterGain.toFixed(2)}</span>
          </div>
        </div>
        <div class="ms-row"><span class="dsp" use:tooltip={'DSP load'}>{Math.round(metersStore.dspLoad * 100)}%</span></div>
      </div>
    </div>
  {/if}
  </div>
</div>

<style>
  .mixer-root { display: flex; flex-direction: column; height: 100%; background: var(--bg-base); }
  .mixer-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .mix { display: flex; gap: 4px; padding: 6px 8px; height: 100%; min-height: 0; overflow-x: auto; align-items: stretch; }

  .strip {
    display: flex; flex-direction: column; align-items: center; gap: 5px;
    width: 84px; flex-shrink: 0; height: 100%; min-height: 0;
    padding: 6px 5px 7px;
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

  /* Fader zone — the only flexible region: it soaks up the panel height so the
     meter grows tall on a big panel and shrinks (never the knobs) on a small one. */
  .fader {
    flex: 1; min-height: 56px;
    display: flex; align-items: stretch; justify-content: center; gap: 9px;
    width: 100%;
  }
  .meter { flex-shrink: 0; min-height: 0; }
  .gain-col { justify-content: flex-end; }

  .knobs-row { display: flex; align-items: flex-start; justify-content: center; gap: 5px; flex-shrink: 0; }
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
  /* Muted but not persistible (calculated gain) — dashed outline = "live only". */
  .ms.on.calc { background: color-mix(in srgb, var(--warning) 55%, var(--bg-input)); border: 1px dashed var(--warning); color: var(--text-primary); }

  .dsp { font-size: 9.5px; font-family: var(--font-code); color: var(--text-muted); }
</style>
