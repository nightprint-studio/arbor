<script lang="ts">
  /** Inspector — rich view of the selected track/voice: identity, mix (with
   *  visual bars + pan indicator), pattern stats, and the round-trip note. */
  import { Crosshair, Disc3, Volume2 } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_TRACKS } from '../mock/data';
  import { laneColor } from '../mock/colors';

  const track = $derived(MOCK_TRACKS.find(t => t.id === groveStore.selectedTrackId) ?? null);
  const pct = (x: number) => `${Math.round(x * 100)}%`;
  function panLabel(p: number) {
    if (Math.abs(p - 0.5) < 0.02) return 'C';
    return p < 0.5 ? `L ${Math.round((0.5 - p) * 200)}` : `R ${Math.round((p - 0.5) * 200)}`;
  }
</script>

<PanelShell title="Inspector">
  {#snippet icon()}<Crosshair size={13} />{/snippet}

  {#if !track}
    <EmptyState message="Select a track (in the arrangement or mixer) to inspect it." />
  {:else}
    {@const color = laneColor(track.colorIdx)}
    <div class="insp" style="--c: {color}">
      <!-- Identity -->
      <div class="insp-head">
        <span class="insp-swatch"></span>
        <div class="insp-id">
          <span class="insp-name">{track.name}</span>
          <span class="insp-voice"><Disc3 size={11} /> {track.voice}</span>
        </div>
        <div class="insp-flags">
          {#if groveStore.isSoloed(track.id)}<Badge variant="tone" tone="info" size="sm" label="solo" />{/if}
          {#if groveStore.isMuted(track.id)}<Badge variant="tone" tone="neutral" size="sm" label="muted" />{/if}
        </div>
      </div>

      <!-- Live meter -->
      <div class="insp-meter-row">
        <Volume2 size={12} />
        <div class="insp-meter"><span class="insp-meter-fill" style="width: {pct(groveStore.isMuted(track.id) ? 0 : track.meter)}"></span></div>
        <span class="insp-meter-val">{pct(track.meter)}</span>
      </div>

      <div class="insp-section">Mix</div>
      <div class="insp-ctl">
        <span class="insp-ctl-label">gain</span>
        <div class="bar"><span class="bar-fill" style="width: {pct(track.gain)}"></span></div>
        <code>{track.gain.toFixed(2)}</code>
      </div>
      <div class="insp-ctl">
        <span class="insp-ctl-label">pan</span>
        <div class="pan"><span class="pan-mid"></span><span class="pan-dot" style="left: {pct(track.pan)}"></span></div>
        <code>{panLabel(track.pan)}</code>
      </div>
      <div class="insp-ctl">
        <span class="insp-ctl-label">room</span>
        <div class="bar"><span class="bar-fill" style="width: {pct(track.room)}"></span></div>
        <code>{pct(track.room)}</code>
      </div>

      <div class="insp-section">Pattern</div>
      <div class="insp-row"><span>events / cycle</span><code>{track.notes.length}</code></div>
      <div class="insp-row"><span>regions</span><code>{track.regions.length}</code></div>
      <div class="insp-row"><span>pitch range</span><code>{track.rollRows} rows</code></div>

      <p class="insp-hint">
        In the real app these values round-trip to the source literals
        (e.g. <code>.gain({track.gain.toFixed(2)})</code>) via Tree-sitter spans.
      </p>
    </div>
  {/if}
</PanelShell>

<style>
  .insp { padding: 6px 0 14px; }

  .insp-head { display: flex; align-items: center; gap: 9px; padding: 6px 12px 12px; }
  .insp-swatch { width: 10px; height: 28px; border-radius: 3px; background: var(--c); flex-shrink: 0; }
  .insp-id { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .insp-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
  .insp-voice { display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--text-muted); font-family: var(--font-code); }
  .insp-voice :global(svg) { color: var(--c); }
  .insp-flags { display: flex; gap: 4px; flex-shrink: 0; }

  .insp-meter-row { display: flex; align-items: center; gap: 7px; padding: 0 12px 6px; color: var(--text-muted); }
  .insp-meter { flex: 1; height: 6px; border-radius: 3px; background: var(--bg-input); overflow: hidden; }
  .insp-meter-fill { display: block; height: 100%; background: linear-gradient(90deg, var(--c), color-mix(in srgb, var(--c) 55%, #fff)); transition: width 120ms linear; }
  .insp-meter-val { font-size: 10px; font-family: var(--font-code); color: var(--text-muted); width: 30px; text-align: right; }

  .insp-section {
    padding: 12px 12px 5px;
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px;
    color: var(--text-muted);
  }

  .insp-ctl { display: flex; align-items: center; gap: 8px; padding: 4px 12px; }
  .insp-ctl-label { width: 34px; flex-shrink: 0; font-size: 11px; color: var(--text-secondary); }
  .insp-ctl code { width: 40px; flex-shrink: 0; text-align: right; font-family: var(--font-code); font-size: 11px; color: var(--text-primary); }

  .bar { flex: 1; height: 6px; border-radius: 3px; background: var(--bg-input); overflow: hidden; }
  .bar-fill { display: block; height: 100%; background: var(--c); border-radius: 3px; }

  .pan { position: relative; flex: 1; height: 6px; border-radius: 3px; background: var(--bg-input); }
  .pan-mid { position: absolute; left: 50%; top: -2px; bottom: -2px; width: 1px; background: var(--border); }
  .pan-dot { position: absolute; top: 50%; width: 9px; height: 9px; border-radius: 50%; background: var(--c); transform: translate(-50%, -50%); box-shadow: 0 0 0 2px var(--bg-base); }

  .insp-row { display: flex; align-items: center; justify-content: space-between; padding: 3px 12px; font-size: 12px; color: var(--text-secondary); }
  .insp-row code { font-family: var(--font-code); font-size: 11px; color: var(--text-primary); }

  .insp-hint {
    margin: 12px 12px 0; padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
    font-size: 11px; line-height: 1.5; color: var(--text-muted);
  }
  .insp-hint code { font-family: var(--font-code); color: var(--text-secondary); }
</style>
