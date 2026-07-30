<script lang="ts">
  /**
   * Reverb return strip — the shared reverb bus, shown at the end of the mixer
   * after the master. Each track sends to it via its `room` knob; this strip
   * visualises those sends converging into the return and exposes the bus's
   * **decay** (procedural IR length). Decay is a global, session-only control
   * like the master gain (no `.merula` representation) — it persists across evals.
   *
   * Imports only shared/ui + merula-local.
   */
  import { Waves } from 'lucide-svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { mixerStore, REVERB_DECAY_DEFAULT } from '../stores/mixer.svelte';

  const tracks = $derived(mixerStore.tracks);
  // Tracks actually feeding the bus (room > 0), for the converging-sends picture.
  const sends = $derived(
    tracks.map((t) => ({ color: t.color, send: mixerStore.roomSend(t.index) })).filter((s) => s.send > 0.001),
  );
  const decay = $derived(mixerStore.reverbDecay);
</script>

<div class="strip rev">
  <div class="strip-name"><Waves size={11} /><span class="nm">REVERB</span></div>

  <div class="sends" use:tooltip={sends.length ? `${sends.length} track${sends.length === 1 ? '' : 's'} sending` : 'No reverb sends — turn up a track’s room'}>
    {#if sends.length}
      {#each sends as s}
        <span class="send" style="--c: {s.color}; --h: {Math.round(Math.min(1, s.send) * 100)}%"></span>
      {/each}
    {:else}
      <span class="sends-empty">no sends</span>
    {/if}
  </div>

  <div class="knob">
    <Knob value={decay} min={0.1} max={6} default={REVERB_DECAY_DEFAULT} size={28} color="var(--info)"
          label="decay" ariaLabel="Reverb decay" onchange={(v) => mixerStore.setReverbDecay(v)} />
    <span class="kval">{decay.toFixed(decay < 1 ? 2 : 1)}<span class="kunit">s</span></span>
  </div>
</div>

<style>
  .strip {
    display: flex; flex-direction: column; align-items: center; gap: 6px;
    width: 84px; flex-shrink: 0; height: 100%; min-height: 0;
    padding: 6px 5px 7px; border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--info) 8%, var(--bg-elevated));
    margin-left: 4px;
  }
  .strip-name { display: flex; align-items: center; gap: 4px; color: var(--text-muted); }
  .strip-name :global(svg) { color: var(--info); }
  .nm { font-size: var(--font-size-3xs); font-weight: 600; letter-spacing: 0.6px; }

  /* Converging sends: thin track-coloured bars, height ∝ each track's room send. */
  .sends {
    flex: 1; min-height: 0; width: 100%;
    display: flex; align-items: flex-end; justify-content: center; gap: 2px;
    padding: 2px 0; overflow: hidden;
    border-radius: var(--radius-sm);
    background: var(--bg-input);
  }
  .send {
    width: 4px; height: var(--h); min-height: 2px;
    border-radius: 2px 2px 0 0;
    background: var(--c);
    transition: height 120ms linear;
  }
  .sends-empty { align-self: center; font-size: var(--font-size-3xs); color: var(--text-muted); font-family: var(--font-code); }

  .knob { display: flex; flex-direction: column; align-items: center; gap: 2px; flex-shrink: 0; }
  .kval { font-size: var(--font-size-3xs); color: var(--text-muted); font-family: var(--font-code); line-height: 1; }
  .kunit { margin-left: 1px; opacity: 0.6; font-size: var(--font-size-3xs); }
</style>
