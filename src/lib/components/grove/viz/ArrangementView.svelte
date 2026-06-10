<script lang="ts">
  /**
   * Read-only arrangement view — Logic Pro-style. A frozen, collapsible
   * track-header column (sticky-left) and a fixed pixels-per-cycle timeline that
   * continues into empty bars past the song (spreadsheet-like) and scrolls
   * horizontally. Section markers are integrated as tinted background bands plus
   * coloured ruler chips. A position cursor + playhead. Hovering a region shows
   * the track summary.
   *
   * Mute/solo live in the mixer; the track headers only show **read-only status
   * icons**. Right-click a header/lane to toggle mute/solo via a context menu.
   *
   * Imports only shared/ui (+ the shared ContextMenu overlay) + grove-local.
   */
  import { PanelLeftClose, PanelRightClose, VolumeX, Headphones } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import Region from './Region.svelte';
  import type { WaveKind } from './waveform';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_TRACKS, MOCK_SECTIONS, TIMELINE_CYCLES, MOCK_PROJECT } from '../mock/data';
  import { laneColor, sectionColor } from '../mock/colors';

  const PX = 26;
  const CONTENT = TIMELINE_CYCLES;
  const VIEW = 96;
  const timelineW = VIEW * PX;

  let collapsed = $state(false);
  const headW = $derived(collapsed ? 48 : 184);

  let startCycle = $state(0);
  let playCycle = $state(0);
  const playStart = $derived(startCycle < CONTENT ? startCycle : 0);

  $effect(() => {
    if (!groveStore.running) return;
    playCycle = playStart;
    let raf = 0; let last = performance.now();
    const tick = (now: number) => {
      const dt = now - last; last = now;
      playCycle += dt / 2000;
      if (playCycle >= CONTENT) playCycle = playStart;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  const soloActive = $derived(groveStore.anySolo);
  const bars = Array.from({ length: VIEW / 4 + 1 }, (_, i) => i * 4);

  function kindFor(name: string): WaveKind {
    if (name === 'drums') return 'percussive';
    if (name === 'pad') return 'sustained';
    return 'tonal';
  }

  let rulerEl = $state<HTMLElement | null>(null);
  function setStartFromEvent(e: MouseEvent) {
    if (!rulerEl) return;
    const r = rulerEl.getBoundingClientRect();
    const cyc = Math.max(0, Math.min(VIEW, (e.clientX - r.left) / PX));
    startCycle = Math.round(cyc * 4) / 4;
    if (!groveStore.running) playCycle = playStart;
  }

  const startX = $derived(headW + startCycle * PX);
  const playX = $derived(headW + playCycle * PX);

  function trackInfo(t: typeof MOCK_TRACKS[number]) {
    const parts = [t.voice, `gain ${t.gain.toFixed(2)}`];
    if (groveStore.isMuted(t.id)) parts.push('muted');
    if (groveStore.isSoloed(t.id)) parts.push('solo');
    parts.push('right-click for mute / solo');
    return { content: t.name, description: parts.join(' · ') };
  }

  // ── Right-click context menu (toggle mute / solo) ──────────────────────────
  let ctx = $state<{ x: number; y: number; trackId: string } | null>(null);
  function openMenu(e: MouseEvent, t: typeof MOCK_TRACKS[number]) {
    e.preventDefault();
    ctx = { x: e.clientX, y: e.clientY, trackId: t.id };
  }
  const ctxItems = $derived<MenuItem[]>(
    ctx ? [
      { id: 'mute', label: groveStore.isMuted(ctx.trackId) ? 'Unmute' : 'Mute', icon: VolumeX },
      { id: 'solo', label: groveStore.isSoloed(ctx.trackId) ? 'Unsolo' : 'Solo', icon: Headphones },
    ] : [],
  );
  function onCtxSelect(id: string) {
    if (!ctx) return;
    if (id === 'mute') groveStore.toggleMute(ctx.trackId);
    else if (id === 'solo') groveStore.toggleSolo(ctx.trackId);
    ctx = null;
  }
</script>

<div class="arr">
  <div class="arr-scroll">
    <div class="arr-inner" style="--head-w: {headW}px; --tl-w: {timelineW}px;">
      <!-- Ruler -->
      <div class="arr-top">
        <div class="arr-corner">
          {#if !collapsed}
            <div class="corner-id">
              <span class="corner-title">{MOCK_PROJECT.name}</span>
              <span class="corner-sub">{MOCK_TRACKS.length} tracks</span>
            </div>
          {/if}
          <button class="corner-toggle" use:tooltip={collapsed ? 'Expand tracks' : 'Collapse tracks'} aria-label="Toggle track headers" onclick={() => collapsed = !collapsed}>
            {#if collapsed}<PanelRightClose size={14} />{:else}<PanelLeftClose size={14} />{/if}
          </button>
        </div>
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="arr-ruler" bind:this={rulerEl} onclick={setStartFromEvent} use:tooltip={'Click to set playback position'}>
          {#each MOCK_SECTIONS as s, i (s.label)}
            <div class="sec-chip" style="left: {s.start * PX}px; width: {s.len * PX}px; --m: {sectionColor(i)};"><span>{s.label}</span></div>
          {/each}
          {#each bars as b (b)}
            <div class="ruler-tick" style="left: {b * PX}px;" class:strong={b % 8 === 0}><span>{b}</span></div>
          {/each}
        </div>
      </div>

      <!-- Track rows -->
      <div class="arr-rows">
        {#each MOCK_TRACKS as track, ti (track.id)}
          {@const color = laneColor(track.colorIdx)}
          {@const muted = groveStore.isMuted(track.id)}
          {@const soloed = groveStore.isSoloed(track.id)}
          {@const dimmed = muted || (soloActive && !soloed)}
          <div class="arr-row" class:selected={groveStore.selectedTrackId === track.id} style="--c: {color}">
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="arr-head" class:collapsed onclick={() => groveStore.selectTrack(track.id)} oncontextmenu={(e) => openMenu(e, track)} use:tooltip={trackInfo(track)}>
              <span class="arr-colorbar"></span>
              {#if !collapsed}
                <div class="arr-head-info">
                  <span class="arr-name">{track.name}</span>
                  <span class="arr-voice">{track.voice}</span>
                </div>
              {/if}
              <div class="arr-status">
                {#if soloed}<Headphones size={12} class="st-solo" />{/if}
                {#if muted}<VolumeX size={12} class="st-mute" />{/if}
              </div>
            </div>
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="arr-lane" onclick={() => groveStore.selectTrack(track.id)} oncontextmenu={(e) => openMenu(e, track)}>
              {#each MOCK_SECTIONS as s, i (s.label)}
                <div class="lane-band" style="left: {s.start * PX}px; width: {s.len * PX}px; --m: {sectionColor(i)};"></div>
              {/each}
              {#each bars as b (b)}
                <div class="lane-grid" style="left: {b * PX}px;" class:strong={b % 8 === 0}></div>
              {/each}
              {#each track.regions as r, i (i)}
                <Region region={r} totalCycles={VIEW} {color} kind={kindFor(track.name)} seed={ti * 97 + i * 31 + r.start} {dimmed} info={trackInfo(track)} />
              {/each}
            </div>
          </div>
        {/each}
      </div>

      <!-- Overlays -->
      {#if groveStore.running && playX > startX}
        <div class="arr-progress" style="left: {startX}px; width: {playX - startX}px;"></div>
      {/if}
      <div class="arr-cursor" style="left: {startX}px;"><span class="cursor-flag"></span></div>
      {#if groveStore.running}
        <div class="arr-playhead" style="left: {playX}px;"><span class="playhead-flag"></span></div>
      {/if}
    </div>
  </div>
</div>

{#if ctx}
  <ContextMenu items={ctxItems} x={ctx.x} y={ctx.y} onSelect={onCtxSelect} onClose={() => ctx = null} />
{/if}

<style>
  .arr {
    /* Header strip matches the editor tab bar height (32px) so the two panes'
       tops line up. */
    --ruler-h: 32px;
    display: flex;
    flex: 1; min-width: 0; min-height: 0;
    background: var(--bg-base);
  }
  .arr-scroll { flex: 1; min-width: 0; min-height: 0; overflow: auto; }
  .arr-inner { position: relative; width: calc(var(--head-w) + var(--tl-w)); }

  /* ── Ruler (bg-base) ── */
  .arr-top { display: flex; height: var(--ruler-h); }
  .arr-corner {
    width: var(--head-w); flex-shrink: 0;
    position: sticky; left: 0; z-index: 6;
    background: var(--bg-base);
    border-right: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
    display: flex; align-items: center; gap: 4px;
    padding: 0 4px 0 12px;
  }
  .corner-id { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .corner-title { font-size: 11.5px; font-weight: 600; color: var(--text-primary); line-height: 1.1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .corner-sub { font-size: 9.5px; color: var(--text-muted); }
  .corner-toggle {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; flex-shrink: 0; margin-left: auto;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .corner-toggle:hover { background: var(--bg-hover); color: var(--text-primary); }

  .arr-ruler {
    position: relative; width: var(--tl-w); flex-shrink: 0;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
    cursor: text;
  }
  /* Section chips sit in the upper half; bar numbers beneath. */
  .sec-chip {
    position: absolute; top: 5px; height: 14px;
    display: flex; align-items: center; padding: 0 6px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--m) 30%, transparent);
    border-left: 3px solid var(--m);
    overflow: hidden; pointer-events: none;
  }
  .sec-chip span { font-size: 8.5px; font-weight: 700; letter-spacing: 0.4px; color: color-mix(in srgb, var(--m) 50%, var(--text-primary)); white-space: nowrap; }
  .ruler-tick { position: absolute; top: 21px; bottom: 0; border-left: 1px solid color-mix(in srgb, var(--border-subtle) 60%, transparent); }
  .ruler-tick.strong { border-left-color: var(--border-subtle); }
  .ruler-tick span { position: absolute; bottom: 1px; left: 3px; font-size: 8.5px; color: var(--text-disabled); font-variant-numeric: tabular-nums; }

  /* ── Rows ── */
  .arr-rows { display: flex; flex-direction: column; }
  .arr-row { display: flex; height: 72px; border-bottom: 1px solid var(--border-subtle); }
  .arr-row.selected .arr-head { background: color-mix(in srgb, var(--c) 16%, var(--bg-base)); }
  .arr-row.selected .arr-lane { background: color-mix(in srgb, var(--c) 5%, transparent); }

  .arr-head {
    width: var(--head-w); flex-shrink: 0;
    position: sticky; left: 0; z-index: 5;
    display: flex; align-items: center; gap: 8px;
    padding: 0 10px 0 0;
    border-right: 1px solid var(--border-subtle);
    background: var(--bg-base);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .arr-head:hover { background: var(--bg-hover); }
  .arr-head.collapsed { padding: 0; gap: 5px; }
  .arr-colorbar { width: 4px; align-self: stretch; background: var(--c); flex-shrink: 0; }
  .arr-head-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; padding-left: 4px; }
  .arr-name { font-size: 12.5px; font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arr-voice { font-size: 10px; color: var(--text-muted); font-family: var(--font-code); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* Read-only status icons (controls live in the mixer). */
  .arr-status { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  :global(.arr-status .st-mute) { color: var(--warning); }
  :global(.arr-status .st-solo) { color: var(--info); }

  /* ── Lane ── */
  .arr-lane { position: relative; width: var(--tl-w); flex-shrink: 0; cursor: pointer; }
  .lane-band { position: absolute; top: 0; bottom: 0; background: color-mix(in srgb, var(--m) 7%, transparent); border-right: 1px solid color-mix(in srgb, var(--m) 18%, transparent); }
  .lane-grid { position: absolute; top: 0; bottom: 0; width: 1px; background: color-mix(in srgb, var(--border-subtle) 35%, transparent); }
  .lane-grid.strong { background: color-mix(in srgb, var(--border-subtle) 70%, transparent); }

  /* ── Overlays ── */
  .arr-progress { position: absolute; top: var(--ruler-h); bottom: 0; background: color-mix(in srgb, var(--accent) 7%, transparent); pointer-events: none; z-index: 3; }
  .arr-cursor { position: absolute; top: var(--ruler-h); bottom: 0; width: 1px; background: color-mix(in srgb, var(--accent) 70%, transparent); pointer-events: none; z-index: 4; }
  .cursor-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--accent); }
  .arr-playhead { position: absolute; top: var(--ruler-h); bottom: 0; width: 1.5px; background: var(--text-primary); opacity: 0.7; pointer-events: none; z-index: 4; }
  .playhead-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--text-primary); }
</style>
