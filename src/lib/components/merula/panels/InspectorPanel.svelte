<script lang="ts">
  /**
   * Inspector — rich read-only view of the selected track: identity, live mix
   * (gain bar + pan indicator reflecting the mixer's live overrides), a real
   * stereo meter, and the queried pattern character. The Mixer is the *edit*
   * surface (knobs); the Inspector is the *inspect* surface, so values here are
   * display-only and mirror what the mixer/arrangement hold.
   *
   * Selection is shared via the MerulaShell store (index-keyed), so clicking a
   * mixer strip (and, once Step 3a wires it, an arrangement header) drives this.
   *
   * Imports only shared/ui + merula-local.
   */
  import { Crosshair, Disc3, Volume2 } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import PeakMeter from './PeakMeter.svelte';
  import FxControls from './FxControls.svelte';
  import { mixerStore, PAN_CENTER, DELAY_DEFAULT } from '../stores/mixer.svelte';
  import { metersStore, diagnosticsStore } from '../stores/engine.svelte';
  import { arrangementStore, noteName } from '../viz/arrangement.svelte';
  import { controlsStore } from '../stores/controls.svelte';
  import { inspectStore } from '../stores/inspect.svelte';

  const index = $derived(mixerStore.selectedIndex);
  const track = $derived(index == null ? null : mixerStore.byIndex(index));

  // The event picked in the arrangement, shown only when it belongs to the track
  // currently in the Inspector (selection follows the track).
  const event = $derived.by(() => {
    const sel = inspectStore.selected;
    if (!sel || index == null || sel.track !== index) return null;
    return sel;
  });
  const fmt = (n: number) => n.toFixed(2).replace(/\.?0+$/, '');
  /** 1 cycle = 1 bar, 4 beats / bar → "bar:beat". */
  function barBeat(cyc: number): string {
    return `${Math.floor(cyc) + 1}:${fmt((cyc - Math.floor(cyc)) * 4 + 1)}`;
  }

  // Self-sufficient on the right rail: re-query the track model + re-parse the
  // source controls on every eval, so the delay/room readouts are correct even
  // when the mixer/arrangement panels aren't mounted.
  $effect(() => {
    void diagnosticsStore.errors;
    arrangementStore.schedule();
    controlsStore.schedule();
  });

  const DELAY_CALC = 'Delay is a calculated value here — edit it in the source.';

  const pct = (x: number) => `${Math.round(Math.max(0, Math.min(1, x)) * 100)}%`;
  function panLabel(p: number): string {
    if (Math.abs(p - PAN_CENTER) < 0.02) return 'C';
    return p < PAN_CENTER ? `L ${Math.round((PAN_CENTER - p) * 200)}` : `R ${Math.round((p - PAN_CENTER) * 200)}`;
  }
  function pitchRange(lo: number | null, hi: number | null): string {
    if (lo == null || hi == null) return '—';
    return lo === hi ? noteName(lo) : `${noteName(lo)}–${noteName(hi)}`;
  }
</script>

<PanelShell title="Inspector">
  {#snippet icon()}<Crosshair size={13} />{/snippet}

  {#if !track}
    <EmptyState message="Select a track (in the mixer or arrangement) to inspect it." />
  {:else}
    {@const gain = mixerStore.gain(track.index)}
    {@const pan = mixerStore.pan(track.index)}
    {@const muted = mixerStore.isMuted(track.index)}
    {@const soloed = mixerStore.isSoloed(track.index)}
    {@const dl = mixerStore.delay(track.index)}
    {@const dCalc = mixerStore.delayCalculated(track.index)}
    <div class="insp" style="--c: {track.color}">
      <!-- Identity -->
      <div class="insp-head">
        <span class="insp-swatch"></span>
        <div class="insp-id">
          <span class="insp-name">{track.name}</span>
          <span class="insp-voice"><Disc3 size={11} /> {track.voice}</span>
        </div>
        <div class="insp-flags">
          {#if soloed}<Badge variant="tone" tone="info" size="sm" label="solo" />{/if}
          {#if muted}<Badge variant="tone" tone="neutral" size="sm" label="muted" />{/if}
        </div>
      </div>

      <!-- Live stereo meter -->
      <div class="insp-meter-row">
        <Volume2 size={12} />
        <div class="insp-meter"><PeakMeter peak={metersStore.peak(track.index)} orientation="horizontal" dimmed={muted} /></div>
      </div>

      <div class="insp-section">Mix · live override</div>
      <div class="insp-ctl">
        <span class="insp-ctl-label">gain</span>
        <div class="bar"><span class="bar-fill" style="width: {pct(gain)}"></span></div>
        <code>{gain.toFixed(2)}</code>
      </div>
      <div class="insp-ctl">
        <span class="insp-ctl-label">pan</span>
        <div class="pan"><span class="pan-mid"></span><span class="pan-dot" style="left: {pct(pan)}"></span></div>
        <code>{panLabel(pan)}</code>
      </div>
      <div class="insp-row">
        <span>room</span>
        <code>{mixerStore.roomCalculated(track.index) ? 'calculated' : mixerStore.room(track.index).toFixed(2)}</code>
      </div>

      <!-- Delay (code-first): the three params edit the .delay(t, fb, mix) literal. -->
      <div class="insp-section">Delay · code-first</div>
      {#if dCalc}
        <div class="insp-row code-first"><span use:tooltip={DELAY_CALC}>delay</span><code>calculated</code></div>
      {:else}
        <div class="insp-delay">
          <div class="dknob">
            <Knob value={dl.t} min={0} max={1} default={DELAY_DEFAULT.t} size={30} color={track.color}
                  label="time" ariaLabel="{track.name} delay time" onchange={(v) => mixerStore.setDelayParam(track.index, 't', v)} />
            <span class="dval">{dl.t.toFixed(3)}</span>
          </div>
          <div class="dknob">
            <Knob value={dl.fb} min={0} max={1} default={DELAY_DEFAULT.fb} size={30} color={track.color}
                  label="fb" ariaLabel="{track.name} delay feedback" onchange={(v) => mixerStore.setDelayParam(track.index, 'fb', v)} />
            <span class="dval">{dl.fb.toFixed(2)}</span>
          </div>
          <div class="dknob">
            <Knob value={dl.mix} min={0} max={1} default={DELAY_DEFAULT.mix} size={30} color={track.color}
                  label="mix" ariaLabel="{track.name} delay mix" onchange={(v) => mixerStore.setDelayParam(track.index, 'mix', v)} />
            <span class="dval">{dl.mix.toFixed(2)}</span>
          </div>
        </div>
        {#if !mixerStore.delayActive(track.index)}
          <p class="insp-subhint">No delay in source — turning a knob adds <code>.delay(…)</code> to this track.</p>
        {/if}
      {/if}

      <!-- FX chain (code-first): parametric EQ + compressor strip inserts. -->
      <div class="insp-section">FX · code-first</div>
      <FxControls index={track.index} color={track.color} />

      <div class="insp-section">Pattern</div>
      <div class="insp-row"><span>haps / window</span><code>{track.hapCount}</code></div>
      <div class="insp-row" use:tooltip={'Peak simultaneous voices — this track’s voice cost'}>
        <span>max voices</span><code>{track.polyphony}</code>
      </div>
      <div class="insp-row"><span>sounds</span><code>{track.sounds.length ? track.sounds.slice(0, 4).join(' ') : '—'}</code></div>
      <div class="insp-row"><span>pitch range</span><code>{pitchRange(track.noteLo, track.noteHi)}</code></div>
      {#if track.hasContinuous}<div class="insp-row"><span>signal</span><code>continuous</code></div>{/if}

      <!-- Selected event — the single hap picked in the arrangement timeline. -->
      {#if event}
        {@const durBeats = (event.end - event.start) * 4}
        <div class="insp-section">Selected event</div>
        <div class="insp-row">
          <span>{event.note != null ? 'note' : event.has_onset ? 'sound' : 'signal'}</span>
          <code>{event.note != null ? noteName(event.note) : (event.sound ?? (event.has_onset ? 'event' : 'continuous'))}</code>
        </div>
        <div class="insp-row"><span>position</span><code>bar {barBeat(event.start)}</code></div>
        <div class="insp-row">
          <span>length</span>
          <code>{event.has_onset ? `${fmt(durBeats)} beat${durBeats === 1 ? '' : 's'}` : 'continuous'}</code>
        </div>
        {#if event.note != null}<div class="insp-row"><span>MIDI</span><code>{Math.round(event.note)}</code></div>{/if}
        {#if event.gain != null}<div class="insp-row"><span>gain</span><code>{event.gain.toFixed(2)}</code></div>{/if}
      {/if}

      <p class="insp-hint">
        Gain &amp; pan are <strong>live session overrides</strong> — commit them to
        source with the ↧ button on the mixer strip. Room &amp; delay are
        <strong>code-first</strong>: their knobs edit the <code>.merula</code> source
        literal directly.
      </p>
    </div>
  {/if}
</PanelShell>

<style>
  .insp { padding: 6px 0 14px; }

  .insp-head { display: flex; align-items: center; gap: 9px; padding: 6px 12px 12px; }
  .insp-swatch { width: 10px; height: 28px; border-radius: 3px; background: var(--c); flex-shrink: 0; }
  .insp-id { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .insp-name { font-size: var(--font-size-lg); font-weight: 600; color: var(--text-primary); }
  .insp-voice { display: flex; align-items: center; gap: 4px; font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .insp-voice :global(svg) { color: var(--c); flex-shrink: 0; }
  .insp-flags { display: flex; gap: 4px; flex-shrink: 0; }

  .insp-meter-row { display: flex; align-items: center; gap: 7px; padding: 0 12px 6px; color: var(--text-muted); }
  .insp-meter { flex: 1; }

  .insp-section {
    padding: 12px 12px 5px;
    font-size: var(--font-size-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px;
    color: var(--text-muted);
  }

  .insp-ctl { display: flex; align-items: center; gap: 8px; padding: 4px 12px; }
  .insp-ctl-label { width: 34px; flex-shrink: 0; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .insp-ctl code { width: 40px; flex-shrink: 0; text-align: right; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }

  .bar { flex: 1; height: 6px; border-radius: 3px; background: var(--bg-input); overflow: hidden; }
  .bar-fill { display: block; height: 100%; background: var(--c); border-radius: 3px; transition: width 90ms linear; }

  .pan { position: relative; flex: 1; height: 6px; border-radius: 3px; background: var(--bg-input); }
  .pan-mid { position: absolute; left: 50%; top: -2px; bottom: -2px; width: 1px; background: var(--border); }
  .pan-dot { position: absolute; top: 50%; width: 9px; height: 9px; border-radius: 50%; background: var(--c); transform: translate(-50%, -50%); box-shadow: 0 0 0 2px var(--bg-base); }

  .insp-row { display: flex; align-items: center; justify-content: space-between; padding: 3px 12px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .insp-row code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 55%; }
  .insp-row.code-first code { color: var(--text-muted); }

  .insp-delay { display: flex; align-items: flex-start; justify-content: center; gap: 14px; padding: 6px 12px 2px; }
  .dknob { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .dval { font-size: var(--font-size-3xs); color: var(--text-muted); font-family: var(--font-code); line-height: 1; }
  .insp-subhint { margin: 2px 12px 0; font-size: var(--font-size-2xs); color: var(--text-muted); line-height: 1.4; }
  .insp-subhint code { font-family: var(--font-code); color: var(--text-secondary); }

  .insp-hint {
    margin: 12px 12px 0; padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted);
  }
  .insp-hint code { font-family: var(--font-code); color: var(--text-secondary); }
  .insp-hint strong { color: var(--text-secondary); font-weight: 600; }
</style>
