<script lang="ts">
  /**
   * Mixer — one strip per track, docked at the BOTTOM (Logic-style): a row of
   * compact channel strips with meter + fader + pan/room + mute/solo. The home
   * of surgical edit-knobs in the real app (gain/pan/room round-trip to the
   * source literals via spans). Mocked: sliders move local state, no write-back.
   * Selecting a strip drives the Inspector, mirroring the arrangement lanes.
   */
  import { SlidersHorizontal, VolumeX, Headphones } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Knob from '$lib/components/shared/ui/Knob.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_TRACKS } from '../mock/data';
  import { laneColor } from '../mock/colors';

  // Gain/pan/room/meter are local mock values; mute/solo live in the store so
  // they stay in sync with the arrangement headers.
  let strips = $state(MOCK_TRACKS.map(t => ({
    id: t.id, name: t.name, colorIdx: t.colorIdx, voice: t.voice,
    gain: t.gain, pan: t.pan, room: t.room, meter: t.meter,
  })));
</script>

<PanelShell title="Mixer" count={strips.length}>
  {#snippet icon()}<SlidersHorizontal size={13} />{/snippet}

  <div class="mix">
    {#each strips as s (s.id)}
      {@const color = laneColor(s.colorIdx)}
      {@const dimmed = groveStore.isMuted(s.id) || (groveStore.anySolo && !groveStore.isSoloed(s.id))}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        class="strip"
        class:selected={groveStore.selectedTrackId === s.id}
        style="--lane-color: {color}"
        onclick={() => groveStore.selectTrack(s.id)}
      >
        <div class="strip-name"><span class="dot"></span>{s.name}</div>

        <div class="strip-body">
          <div class="meter" aria-hidden="true"><span class="meter-fill" style="height: {Math.round((dimmed ? 0 : s.meter) * 100)}%"></span></div>
          <input class="fader" type="range" min="0" max="1" step="0.01" bind:value={s.gain} aria-label="{s.name} gain" onclick={(e) => e.stopPropagation()} />
        </div>
        <div class="gainval">{s.gain.toFixed(2)}</div>

        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="knobs" onpointerdown={(e) => e.stopPropagation()}>
          <Knob bind:value={s.pan} bipolar default={0.5} size={28} color={color} label="pan" ariaLabel="{s.name} pan" />
          <Knob bind:value={s.room} default={0} size={28} color={color} label="room" ariaLabel="{s.name} room" />
        </div>

        <div class="ms-row">
          <button class="ms" class:on={groveStore.isMuted(s.id)} use:tooltip={'Mute'} aria-label="Mute" onclick={(e) => { e.stopPropagation(); groveStore.toggleMute(s.id); }}><VolumeX size={11} /></button>
          <button class="ms solo" class:on={groveStore.isSoloed(s.id)} use:tooltip={'Solo'} aria-label="Solo" onclick={(e) => { e.stopPropagation(); groveStore.toggleSolo(s.id); }}><Headphones size={11} /></button>
        </div>
      </div>
    {/each}
  </div>
</PanelShell>

<style>
  .mix { display: flex; gap: 4px; padding: 6px 8px; height: 100%; overflow-x: auto; align-items: stretch; }
  .strip {
    display: flex; flex-direction: column; align-items: center; gap: 3px;
    width: 72px; flex-shrink: 0;
    padding: 5px 4px 6px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    cursor: pointer;
    transition: background var(--transition-fast), box-shadow var(--transition-fast);
  }
  .strip:hover { background: color-mix(in srgb, var(--bg-hover) 60%, var(--bg-elevated)); }
  .strip.selected { box-shadow: inset 0 0 0 1px var(--lane-color); }

  .strip-name {
    display: flex; align-items: center; gap: 4px;
    font-size: 11px; font-weight: 600; color: var(--text-primary);
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .dot { width: 7px; height: 7px; border-radius: 2px; background: var(--lane-color); flex-shrink: 0; }

  .strip-body { display: flex; align-items: stretch; gap: 6px; flex: 1; min-height: 56px; }
  .meter { width: 6px; border-radius: 3px; background: var(--bg-input); display: flex; flex-direction: column-reverse; overflow: hidden; }
  .meter-fill { width: 100%; background: linear-gradient(0deg, var(--lane-color), color-mix(in srgb, var(--lane-color) 55%, #fff)); transition: height 120ms linear; }

  .fader {
    -webkit-appearance: slider-vertical;
    writing-mode: vertical-lr; direction: rtl;
    width: 16px; accent-color: var(--lane-color); cursor: pointer;
  }
  .gainval { font-size: 9.5px; color: var(--text-muted); font-family: var(--font-code); }

  .knobs { display: flex; gap: 8px; justify-content: center; margin-top: 1px; }

  .ms-row { display: flex; gap: 3px; margin-top: 1px; }
  .ms {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 18px; border: 1px solid var(--border-subtle);
    background: var(--bg-input); border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ms:hover { color: var(--text-primary); }
  .ms.on { background: var(--warning); color: #1a1b1e; border-color: transparent; }
  .ms.solo.on { background: var(--info); color: #fff; }
</style>
