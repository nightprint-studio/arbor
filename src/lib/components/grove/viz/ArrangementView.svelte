<script lang="ts">
  /**
   * Read-only arrangement view — Logic Pro-style, driven by the **real engine**.
   * Lanes come from `grove_query` (the last-evaluated arrangement, grouped per
   * mixer strip); each lane draws its real haps (see HapLane). The playhead +
   * cursor follow the transport store (real cycle position, not a mock RAF), the
   * ruler seeks for real (`groveSeek`), and the timeline gently auto-follows the
   * playhead during playback.
   *
   * A frozen, collapsible track-header column (sticky-left) and a fixed
   * pixels-per-cycle timeline that continues into empty bars past the song
   * (spreadsheet-like). Section markers are integrated as tinted background bands
   * plus coloured ruler chips (Step-0 song structure — no BE source for these).
   *
   * Mute/solo live in the mixer (Step 3b); the headers show **read-only status
   * icons**. Right-click a header/lane toggles mute/solo via the shared store
   * (keyed by strip index) and pushes a live `grove_set_track` override so the
   * audio responds from here too.
   *
   * Imports only shared/ui (+ the shared ContextMenu overlay) + grove-local.
   */
  import { PanelLeftClose, PanelRightClose, VolumeX, Headphones } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import HapLane from './HapLane.svelte';
  import { arrangementStore, noteName, VIEW_CYCLES, type VizLane } from './arrangement.svelte';
  import { transportStore, groveEngine, diagnosticsStore } from '../stores/engine.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { groveSetTrack } from '$lib/ipc/grove';
  import { MOCK_SECTIONS } from '../mock/data';
  import { laneColor, sectionColor } from '../mock/colors';

  const PX = 26;
  const VIEW = VIEW_CYCLES;
  const timelineW = VIEW * PX;

  let collapsed = $state(false);
  const headW = $derived(collapsed ? 48 : 184);

  // ── Live data: re-query on every eval (diagnostics reassign = eval happened) ──
  $effect(() => {
    void diagnosticsStore.errors; // dep: a fresh array reference is pushed on each eval
    arrangementStore.schedule(VIEW);
  });

  const lanes = $derived(arrangementStore.lanes);

  // ── Track names: best-effort from the evaluated source (BE gives only indices).
  // The evaluated source is the active tab; `tracks(...)` order == strip order.
  const trackNames = $derived(extractTrackNames(projectStore.activeSource));
  function extractTrackNames(src: string): string[] {
    const names: string[] = [];
    const re = /\btrack\(\s*"([^"]+)"/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(src)) !== null) names.push(m[1]);
    return names;
  }

  /** Stable mute/solo key for a strip — numeric so it never collides with the
   *  Step-0 mock track ids ('t-bass', …) still seeded in the shared store. */
  const laneKey = (track: number) => String(track);

  function laneTitle(l: VizLane): string {
    return trackNames[l.track] ?? `Track ${l.track + 1}`;
  }
  function laneVoice(l: VizLane): string {
    if (l.sounds.length) return l.sounds.slice(0, 3).join(' ');
    if (l.noteCount && l.noteLo != null && l.noteHi != null) {
      return `${noteName(l.noteLo)}–${noteName(l.noteHi)} · ${l.noteCount}♪`;
    }
    if (l.hasContinuous) return 'signal';
    return '—';
  }
  function laneInfo(l: VizLane) {
    const parts = [laneVoice(l), `${l.haps.length} haps`];
    if (groveStore.isMuted(laneKey(l.track))) parts.push('muted');
    if (groveStore.isSoloed(laneKey(l.track))) parts.push('solo');
    parts.push('right-click for mute / solo');
    return { content: laneTitle(l), description: parts.join(' · ') };
  }

  // Solo dimming computed over the REAL lanes only (the shared store is also
  // seeded with stale mock solo state, which would otherwise dim everything).
  const soloActive = $derived(lanes.some((l) => groveStore.isSoloed(laneKey(l.track))));

  // ── Transport-driven playhead + seek cursor ──────────────────────────────────
  const playCycle = $derived(transportStore.cycle);
  const playing   = $derived(transportStore.playing);
  let   cursorCycle = $state(0); // seek anchor (last ruler click / arrow seek)

  const playX   = $derived(headW + playCycle * PX);
  const cursorX = $derived(headW + cursorCycle * PX);
  const endX    = $derived(headW + arrangementStore.contentEnd * PX);

  const bars = Array.from({ length: VIEW / 4 + 1 }, (_, i) => i * 4);

  let rulerEl = $state<HTMLElement | null>(null);
  function seekTo(cyc: number) {
    cursorCycle = Math.max(0, Math.round(cyc * 4) / 4);
    void groveEngine.seek(cursorCycle);
  }
  function setStartFromEvent(e: MouseEvent) {
    if (!rulerEl) return;
    const r = rulerEl.getBoundingClientRect();
    seekTo(Math.max(0, Math.min(VIEW, (e.clientX - r.left) / PX)));
  }

  // ── Auto-follow: keep the playhead in view while playing (re-armed on each
  // play start; a manual wheel pins it until the next start). ───────────────────
  let scrollEl = $state<HTMLElement | null>(null);
  let userPinned = $state(false);
  let prevPlaying = false;
  $effect(() => {
    const p = transportStore.playing;
    if (p && !prevPlaying) userPinned = false;
    prevPlaying = p;
  });
  $effect(() => {
    const x = playX; // dep — ~30 fps
    if (!playing || userPinned || !scrollEl) return;
    const vw = scrollEl.clientWidth;
    const sl = scrollEl.scrollLeft;
    if (x < sl + headW + 24 || x > sl + vw - 48) {
      scrollEl.scrollLeft = Math.max(0, x - headW - vw / 3);
    }
  });
  function onWheel() { if (playing) userPinned = true; }

  // ── Keyboard: ↑/↓ select lanes · ←/→ nudge + seek the cursor · Home → start ───
  let selectedPos = $state(0);
  function onKeydown(e: KeyboardEvent) {
    if (!lanes.length) return;
    if (e.key === 'ArrowDown')      { selectedPos = Math.min(lanes.length - 1, selectedPos + 1); e.preventDefault(); }
    else if (e.key === 'ArrowUp')   { selectedPos = Math.max(0, selectedPos - 1); e.preventDefault(); }
    else if (e.key === 'ArrowRight'){ seekTo(cursorCycle + 1); e.preventDefault(); }
    else if (e.key === 'ArrowLeft') { seekTo(cursorCycle - 1); e.preventDefault(); }
    else if (e.key === 'Home')      { seekTo(0); e.preventDefault(); }
  }

  // ── Right-click context menu (toggle mute / solo) ─────────────────────────────
  let ctx = $state<{ x: number; y: number; track: number } | null>(null);
  function openMenu(e: MouseEvent, track: number) {
    e.preventDefault();
    ctx = { x: e.clientX, y: e.clientY, track };
  }
  const ctxItems = $derived<MenuItem[]>(
    ctx
      ? [
          { id: 'mute', label: groveStore.isMuted(laneKey(ctx.track)) ? 'Unmute' : 'Mute', icon: VolumeX },
          { id: 'solo', label: groveStore.isSoloed(laneKey(ctx.track)) ? 'Unsolo' : 'Solo', icon: Headphones },
        ]
      : [],
  );
  function onCtxSelect(id: string) {
    if (!ctx) return;
    const key = laneKey(ctx.track);
    if (id === 'mute') {
      groveStore.toggleMute(key);
      void groveSetTrack('mute', ctx.track, groveStore.isMuted(key) ? 1 : 0);
    } else if (id === 'solo') {
      groveStore.toggleSolo(key);
      void groveSetTrack('solo', ctx.track, groveStore.isSoloed(key) ? 1 : 0);
    }
    ctx = null;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="arr" tabindex="0" role="group" aria-label="Arrangement" onkeydown={onKeydown}>
  <div class="arr-scroll" bind:this={scrollEl} onwheel={onWheel}>
    <div class="arr-inner" style="--head-w: {headW}px; --tl-w: {timelineW}px;">
      <!-- Ruler -->
      <div class="arr-top">
        <div class="arr-corner">
          {#if !collapsed}
            <div class="corner-id">
              <span class="corner-title">{projectStore.project?.name ?? 'grove'}</span>
              <span class="corner-sub">{lanes.length} {lanes.length === 1 ? 'track' : 'tracks'}</span>
            </div>
          {/if}
          <button class="corner-toggle" use:tooltip={collapsed ? 'Expand tracks' : 'Collapse tracks'} aria-label="Toggle track headers" onclick={() => (collapsed = !collapsed)}>
            {#if collapsed}<PanelRightClose size={14} />{:else}<PanelLeftClose size={14} />{/if}
          </button>
        </div>
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="arr-ruler" bind:this={rulerEl} onclick={setStartFromEvent} use:tooltip={'Click to seek'}>
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
        {#each lanes as lane, pos (lane.track)}
          {@const color = laneColor(lane.track)}
          {@const muted = groveStore.isMuted(laneKey(lane.track))}
          {@const soloed = groveStore.isSoloed(laneKey(lane.track))}
          {@const dimmed = muted || (soloActive && !soloed)}
          <div class="arr-row" class:selected={selectedPos === pos} style="--c: {color}">
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="arr-head" class:collapsed onclick={() => (selectedPos = pos)} oncontextmenu={(e) => openMenu(e, lane.track)} use:tooltip={laneInfo(lane)}>
              <span class="arr-colorbar"></span>
              {#if !collapsed}
                <div class="arr-head-info">
                  <span class="arr-name">{laneTitle(lane)}</span>
                  <span class="arr-voice">{laneVoice(lane)}</span>
                </div>
              {/if}
              <div class="arr-status">
                {#if soloed}<Headphones size={12} class="st-solo" />{/if}
                {#if muted}<VolumeX size={12} class="st-mute" />{/if}
              </div>
            </div>
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="arr-lane" onclick={() => (selectedPos = pos)} oncontextmenu={(e) => openMenu(e, lane.track)}>
              {#each MOCK_SECTIONS as s, i (s.label)}
                <div class="lane-band" style="left: {s.start * PX}px; width: {s.len * PX}px; --m: {sectionColor(i)};"></div>
              {/each}
              {#each bars as b (b)}
                <div class="lane-grid" style="left: {b * PX}px;" class:strong={b % 8 === 0}></div>
              {/each}
              <HapLane {lane} {color} view={VIEW} px={PX} {dimmed} {playCycle} {playing} />
            </div>
          </div>
        {/each}

        {#if !lanes.length}
          <div class="arr-empty">
            {#if arrangementStore.loaded}
              <span>No arrangement yet — evaluate a <code>.grove</code> file to see its tracks.</span>
            {:else}
              <span>Loading arrangement…</span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Overlays -->
      {#if lanes.length}
        {#if arrangementStore.contentEnd > 0 && arrangementStore.contentEnd < VIEW}
          <div class="arr-end" style="left: {endX}px;"></div>
        {/if}
        {#if playing && playX > cursorX}
          <div class="arr-progress" style="left: {cursorX}px; width: {playX - cursorX}px;"></div>
        {/if}
        <div class="arr-cursor" style="left: {cursorX}px;"><span class="cursor-flag"></span></div>
        <div class="arr-playhead" class:idle={!playing} style="left: {playX}px;"><span class="playhead-flag"></span></div>
      {/if}
    </div>
  </div>
</div>

{#if ctx}
  <ContextMenu items={ctxItems} x={ctx.x} y={ctx.y} onSelect={onCtxSelect} onClose={() => (ctx = null)} />
{/if}

<style>
  .arr {
    /* Header strip matches the editor tab bar height (32px) so the two panes'
       tops line up. */
    --ruler-h: 32px;
    display: flex;
    flex: 1; min-width: 0; min-height: 0;
    background: var(--bg-base);
    outline: none;
  }
  .arr:focus-visible { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent); }
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

  /* ── Empty state ── */
  /* Sticky-left + bounded width so it stays in view (the inner timeline is far
     wider than the viewport). */
  .arr-empty {
    position: sticky; left: 0;
    display: flex; align-items: center; justify-content: flex-start;
    width: 520px; height: 160px; padding: 0 28px;
    color: var(--text-muted); font-size: 12px;
  }
  .arr-empty code { font-family: var(--font-code); font-size: 11px; color: var(--text-secondary); }

  /* ── Overlays ── */
  .arr-end { position: absolute; top: var(--ruler-h); bottom: 0; width: 1px; background: color-mix(in srgb, var(--border-strong, var(--text-disabled)) 60%, transparent); border-left: 1px dashed color-mix(in srgb, var(--text-disabled) 60%, transparent); pointer-events: none; z-index: 2; }
  .arr-progress { position: absolute; top: var(--ruler-h); bottom: 0; background: color-mix(in srgb, var(--accent) 7%, transparent); pointer-events: none; z-index: 3; }
  .arr-cursor { position: absolute; top: var(--ruler-h); bottom: 0; width: 1px; background: color-mix(in srgb, var(--accent) 70%, transparent); pointer-events: none; z-index: 4; }
  .cursor-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--accent); }
  .arr-playhead { position: absolute; top: var(--ruler-h); bottom: 0; width: 1.5px; background: var(--text-primary); opacity: 0.7; pointer-events: none; z-index: 4; transition: opacity var(--transition-fast); }
  .arr-playhead.idle { opacity: 0.32; }
  .playhead-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--text-primary); }
</style>
