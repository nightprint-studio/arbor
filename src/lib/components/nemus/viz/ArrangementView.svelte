<script lang="ts">
  /**
   * Read-only arrangement view — Logic Pro-style, driven by the **real engine**.
   * Lanes come from `nemus_query` (the last-evaluated arrangement, grouped per
   * mixer strip); each lane draws its real haps (see HapLane). The playhead +
   * cursor follow the transport store (real cycle position, not a mock RAF), the
   * ruler seeks for real (`nemusSeek`), and the timeline gently auto-follows the
   * playhead during playback.
   *
   * A frozen, collapsible track-header column (sticky-left) and a fixed
   * pixels-per-cycle timeline that continues into empty bars past the song
   * (spreadsheet-like). Named sections (`section("INTRO", …)` in the source) are
   * drawn as tinted per-lane background bands plus coloured ruler chips, tiled
   * across the timeline by `nemus_query`.
   *
   * Mute/solo live in the mixer (Step 3b); the headers show **read-only status
   * icons**. Right-click a header/lane toggles mute/solo via the shared store
   * (keyed by strip index) and pushes a live `nemus_set_track` override so the
   * audio responds from here too.
   *
   * Imports only shared/ui (+ the shared ContextMenu overlay) + nemus-local.
   */
  import { PanelLeftClose, PanelRightClose, VolumeX, Headphones, FileInput } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { importActions } from '../stores/import-actions.svelte';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import HapLane from './HapLane.svelte';
  import ArrangementToolbar from './ArrangementToolbar.svelte';
  import { arrangementStore, noteName, VIEW_CYCLES, type VizLane } from './arrangement.svelte';
  import { arrViewOptions } from './arr-view-options.svelte';
  import { transportStore, nemusEngine, diagnosticsStore } from '../stores/engine.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import { mixerStore } from '../stores/mixer.svelte';
  import { inspectStore } from '../stores/inspect.svelte';
  import { symbolHighlightStore } from '../stores/symbol-highlight.svelte';
  import { laneColor, sectionColor } from '../palette';
  import { makeByteToU16 } from '../editor/nemus-lang';
  import type { NemusQueryHap } from '$lib/ipc/nemus';

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
    if (nemusStore.isMuted(laneKey(l.track))) parts.push('muted');
    if (nemusStore.isSoloed(laneKey(l.track))) parts.push('solo');
    parts.push('right-click for mute / solo');
    return { content: laneTitle(l), description: parts.join(' · ') };
  }

  // Solo dimming computed over the REAL lanes only (the shared store is also
  // seeded with stale mock solo state, which would otherwise dim everything).
  const soloActive = $derived(lanes.some((l) => nemusStore.isSoloed(laneKey(l.track))));

  // ── Transport-driven playhead + seek cursor ──────────────────────────────────
  // The backend transport clock free-runs (monotonic), so its raw cycle marches
  // forever — past the song it would leave the drawn timeline into empty bars.
  // The arrangement loops by periodicity (audio restarts every `loopCycles`), so
  // we WRAP the displayed playhead at that period: it returns to the start when
  // the song repeats, matching what's actually sounding. Falls back to the raw
  // cycle when no loop is known (nothing evaluated yet).
  const playCycle = $derived(transportStore.cycle);
  const playing   = $derived(transportStore.playing);
  const loopCycles = $derived(arrangementStore.loopCycles);
  const displayCycle = $derived(loopCycles > 0 ? playCycle % loopCycles : playCycle);
  let   cursorCycle = $state(0); // seek anchor (last ruler click / arrow seek)

  const playX   = $derived(headW + displayCycle * PX);
  const cursorX = $derived(headW + cursorCycle * PX);
  const endX    = $derived(headW + arrangementStore.contentEnd * PX);

  const bars = Array.from({ length: VIEW / 4 + 1 }, (_, i) => i * 4);

  // ── Time ruler (footer): cycles → wall-clock. One cycle = 1/cps seconds; the
  // live transport cps drives it, falling back to a sane default before the first
  // eval/play so the strip still reads sensibly. ──────────────────────────────
  const cps = $derived(transportStore.cps > 0 ? transportStore.cps : 0.5);
  function fmtClock(cycle: number): string {
    const sec = cycle / cps;
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  let rulerEl = $state<HTMLElement | null>(null);
  function seekTo(cyc: number) {
    cursorCycle = Math.max(0, Math.round(cyc * 4) / 4);
    void nemusEngine.seek(cursorCycle);
  }
  function setStartFromEvent(e: MouseEvent) {
    if (!rulerEl) return;
    const r = rulerEl.getBoundingClientRect();
    seekTo(Math.max(0, Math.min(VIEW, (e.clientX - r.left) / PX)));
  }

  // ── Auto-follow: keep the playhead in view while playing (re-armed on each
  // play start; ANY manual horizontal scroll pins it until the next start). ──────
  let scrollEl = $state<HTMLElement | null>(null);
  let userPinned = $state(false);
  let prevPlaying = false;
  // The scrollLeft we last set programmatically. A `scroll` event whose position
  // differs from this is a user scroll (scrollbar, trackpad pan, keyboard) — that
  // pins the follow so the playhead stops yanking the view back (the glitch).
  let lastAutoScrollLeft = 0;
  $effect(() => {
    const p = transportStore.playing;
    if (p && !prevPlaying) { userPinned = false; if (scrollEl) lastAutoScrollLeft = scrollEl.scrollLeft; }
    prevPlaying = p;
  });
  $effect(() => {
    const x = playX; // dep — ~30 fps
    if (!playing || userPinned || !arrViewOptions.follow || !scrollEl) return;
    const vw = scrollEl.clientWidth;
    const sl = scrollEl.scrollLeft;
    if (x < sl + headW + 24 || x > sl + vw - 48) {
      scrollEl.scrollLeft = Math.max(0, x - headW - vw / 3);
      lastAutoScrollLeft = scrollEl.scrollLeft; // remember the clamped value we set
    }
  });
  /** Pin the follow when the user scrolls horizontally themselves — covers the
   *  scrollbar / trackpad-pan / keyboard cases that `wheel` alone misses. Our own
   *  programmatic scrolls match `lastAutoScrollLeft`, so they don't pin. */
  function onScroll() {
    if (playing && !userPinned && scrollEl && Math.abs(scrollEl.scrollLeft - lastAutoScrollLeft) > 2) {
      userPinned = true;
    }
  }

  // ── Selection: shared with the mixer + inspector via the nemus store (keyed by
  // strip index), so clicking/▲▼ here drives the Inspector too. ↑/↓ move the
  // selection; ←/→ nudge + seek the cursor; Home → start. ───────────────────────
  const selectedTrack = $derived(mixerStore.selectedIndex);
  function selectLaneAt(pos: number) {
    const lane = lanes[pos];
    if (lane) selectTrack(lane.track);
  }
  /** Select a whole track (header / lane bg click). Clears any finer event pick
   *  that belonged to a different track so the Inspector doesn't show a stale one. */
  function selectTrack(track: number) {
    mixerStore.select(track);
    inspectStore.clearIfNotTrack(track);
  }

  // ── Event pick: clicking a hap selects its track AND the single event, then
  // surfaces it in the Inspector (opening that rail if it's hidden). ─────────────
  function pickHap(lane: VizLane, hap: NemusQueryHap) {
    mixerStore.select(lane.track);
    inspectStore.select(lane.track, hap);
    nemusStore.showRight('inspector');
  }
  /** Ctrl/Cmd+click a hap → reveal the source span that produced it. The hap span
   *  is a UTF-8 byte range (backend coordinate); convert to the editor's UTF-16
   *  offset before relaying the jump (no-op on pure-ASCII source). */
  function gotoHapSource(hap: NemusQueryHap) {
    if (hap.span_start == null) return;
    const offset = makeByteToU16(projectStore.activeSource)(hap.span_start);
    nemusStore.requestGoto(offset, 0);
  }
  /** Key of the selected event within a given lane, for the block highlight. */
  function selectedKeyFor(lane: VizLane): string | null {
    const sel = inspectStore.selected;
    if (!sel || sel.track !== lane.track) return null;
    if (!sel.has_onset) return `${lane.track}:cont`;
    const i = lane.haps.findIndex(
      (h) => h.start === sel.start && h.end === sel.end && h.note === sel.note && h.sound === sel.sound,
    );
    return i >= 0 ? `${lane.track}:${i}` : null;
  }
  function onKeydown(e: KeyboardEvent) {
    if (!lanes.length) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const cur  = lanes.findIndex((l) => l.track === selectedTrack);
      const base = cur < 0 ? 0 : cur;
      selectLaneAt(e.key === 'ArrowDown'
        ? Math.min(lanes.length - 1, base + 1)
        : Math.max(0, base - 1));
      e.preventDefault();
    }
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
          { id: 'mute', label: nemusStore.isMuted(laneKey(ctx.track)) ? 'Unmute' : 'Mute', icon: VolumeX },
          { id: 'solo', label: nemusStore.isSoloed(laneKey(ctx.track)) ? 'Unsolo' : 'Solo', icon: Headphones },
        ]
      : [],
  );
  function onCtxSelect(id: string) {
    if (!ctx) return;
    // Route through the mixer store so mute writes `.gain(0)` into the source (and
    // unmute restores it) — the single mute/solo entry point shared with the strips.
    if (id === 'mute') mixerStore.toggleMute(ctx.track);
    else if (id === 'solo') mixerStore.toggleSolo(ctx.track);
    ctx = null;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="arr" tabindex="0" role="group" aria-label="Arrangement" onkeydown={onKeydown}>
  <div class="arr-scroll" bind:this={scrollEl} onscroll={onScroll}>
    <div class="arr-inner" style="--head-w: {headW}px; --tl-w: {timelineW}px;">
      <!-- View toolbar (sticky-left, stays put as the timeline scrolls) -->
      <div class="arr-toolbar-row">
        <div class="arr-toolbar-anchor">
          <button class="tb-import" onclick={() => importActions.start()}
                  use:tooltip={{ content: 'Import audio / MIDI', description: 'Transcribe a WAV or bring a .mid in as an editable .nemus file' }}>
            <FileInput size={13} />
            <span>Import</span>
          </button>
          <span class="tb-sep"></span>
          <ArrangementToolbar />
        </div>
      </div>

      <!-- Ruler -->
      <div class="arr-top">
        <div class="arr-corner">
          {#if !collapsed}
            <div class="corner-id">
              <span class="corner-title">{projectStore.project?.name ?? 'nemus'}</span>
              <span class="corner-sub">{lanes.length} {lanes.length === 1 ? 'track' : 'tracks'}</span>
            </div>
          {/if}
          <button class="corner-toggle" use:tooltip={collapsed ? 'Expand tracks' : 'Collapse tracks'} aria-label="Toggle track headers" onclick={() => (collapsed = !collapsed)}>
            {#if collapsed}<PanelRightClose size={14} />{:else}<PanelLeftClose size={14} />{/if}
          </button>
        </div>
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div class="arr-ruler" bind:this={rulerEl} onclick={setStartFromEvent} use:tooltip={'Click to seek'}>
          {#each arrangementStore.rulerSections as s (s.name + '@' + s.start)}
            <div class="ruler-chip" style="left: {s.start * PX}px; width: {(s.end - s.start) * PX}px; --sc: {sectionColor(s.name)}">
              <span>{s.name}</span>
            </div>
          {/each}
          {#each bars as b (b)}
            <div class="ruler-tick" style="left: {b * PX}px;" class:strong={b % 8 === 0} class:hide-line={!arrViewOptions.grid}><span>{b}</span></div>
          {/each}
        </div>
      </div>

      <!-- Track rows -->
      <div class="arr-rows">
        {#each lanes as lane (lane.track)}
          {@const color = laneColor(lane.track)}
          {@const muted = nemusStore.isMuted(laneKey(lane.track))}
          {@const soloed = nemusStore.isSoloed(laneKey(lane.track))}
          {@const dimmed = muted || (soloActive && !soloed)}
          <div class="arr-row" class:selected={selectedTrack === lane.track} class:sym-hl={symbolHighlightStore.has(lane.track)} style="--c: {color}">
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div class="arr-head" class:collapsed onclick={() => selectTrack(lane.track)} oncontextmenu={(e) => openMenu(e, lane.track)} use:tooltip={laneInfo(lane)}>
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
            <div class="arr-lane" onclick={() => selectTrack(lane.track)} oncontextmenu={(e) => openMenu(e, lane.track)}>
              {#each lane.sections as s (s.name + '@' + s.start)}
                <div class="lane-band" style="left: {s.start * PX}px; width: {(s.end - s.start) * PX}px; --sc: {sectionColor(s.name)}"></div>
              {/each}
              {#if arrViewOptions.grid}
                {#each bars as b (b)}
                  <div class="lane-grid" style="left: {b * PX}px;" class:strong={b % 8 === 0}></div>
                {/each}
              {/if}
              <HapLane {lane} {color} view={VIEW} px={PX} {dimmed} {playCycle} {playing}
                       selectedKey={selectedKeyFor(lane)} onpick={(h) => pickHap(lane, h)}
                       ongoto={gotoHapSource} />
            </div>
          </div>
        {/each}

        {#if !lanes.length}
          <div class="arr-empty">
            {#if arrangementStore.loaded}
              <span>No arrangement yet — evaluate a <code>.nemus</code> file to see its tracks.</span>
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

      <!-- Time ruler (footer): MM:SS in parallel to the bars/beats above; sticks
           to the bottom and scrolls horizontally with the timeline. -->
      <div class="arr-time">
        <div class="arr-time-corner">time</div>
        <div class="arr-time-track">
          {#each bars as b (b)}
            <div class="time-tick" style="left: {b * PX}px;" class:strong={b % 8 === 0}><span>{fmtClock(b)}</span></div>
          {/each}
        </div>
      </div>
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
    /* View-toolbar row above the ruler (offsets the absolute overlays below). */
    --tb-h: 30px;
    display: flex;
    flex: 1; min-width: 0; min-height: 0;
    background: var(--bg-base);
    outline: none;
  }
  .arr:focus-visible { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent); }
  .arr-scroll { flex: 1; min-width: 0; min-height: 0; overflow: auto; }
  .arr-inner { position: relative; width: calc(var(--head-w) + var(--tl-w)); }

  /* ── View toolbar row (sticky-left strip above the ruler) ── */
  .arr-toolbar-row {
    position: relative; z-index: 7;
    height: 30px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
  }
  .arr-toolbar-anchor {
    position: sticky; left: 0;
    display: inline-flex; align-items: center; height: 100%;
    width: max-content;
    padding: 0 8px;
  }

  /* Import action — sits left of the view toggles. */
  .tb-import {
    display: inline-flex; align-items: center; gap: 5px;
    height: 22px; padding: 0 9px;
    background: var(--bg-hover); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-secondary);
    font-size: 11px; font-weight: 600; cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .tb-import:hover {
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-hover));
    color: var(--text-primary);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border-subtle));
  }
  .tb-sep { width: 1px; height: 16px; background: var(--border-subtle); margin: 0 8px; }

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
  .ruler-tick { position: absolute; top: 21px; bottom: 0; border-left: 1px solid color-mix(in srgb, var(--border-subtle) 60%, transparent); }
  .ruler-tick.strong { border-left-color: var(--border-subtle); }
  .ruler-tick.hide-line { border-left-color: transparent; }
  .ruler-tick span { position: absolute; bottom: 1px; left: 3px; font-size: 8.5px; color: var(--text-disabled); font-variant-numeric: tabular-nums; }

  /* Named-section chips: a coloured label strip along the top of the ruler. */
  .ruler-chip {
    position: absolute; top: 2px; height: 13px;
    background: color-mix(in srgb, var(--sc) 26%, transparent);
    border-left: 2px solid var(--sc); border-radius: 0 3px 3px 0;
    overflow: hidden; pointer-events: none;
  }
  .ruler-chip span {
    display: block; padding: 0 5px; line-height: 13px;
    font-size: 8.5px; font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase;
    color: var(--text-secondary); white-space: nowrap;
  }

  /* ── Rows ── */
  .arr-rows { display: flex; flex-direction: column; }
  .arr-row { display: flex; height: 72px; border-bottom: 1px solid var(--border-subtle); }
  .arr-row.selected .arr-head { background: color-mix(in srgb, var(--c) 16%, var(--bg-base)); }
  .arr-row.selected .arr-lane { background: color-mix(in srgb, var(--c) 5%, transparent); }

  /* Symbol highlight: tracks whose pattern references the identifier under the
     editor caret. Accent-tinted (distinct from the per-track selection colour) so
     it reads as "these lanes use this phrase". */
  .arr-row.sym-hl .arr-head { box-shadow: inset 3px 0 0 var(--accent); background: color-mix(in srgb, var(--accent) 13%, var(--bg-base)); }
  .arr-row.sym-hl .arr-lane { background: color-mix(in srgb, var(--accent) 6%, transparent); }

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
  /* Section backdrop band — drawn first (DOM order) so it sits under the grid +
     haps without needing an explicit z-index. */
  .lane-band { position: absolute; top: 0; bottom: 0; pointer-events: none;
    background: color-mix(in srgb, var(--sc) 8%, transparent);
    border-left: 1px solid color-mix(in srgb, var(--sc) 38%, transparent); }
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
  /* Start below the toolbar row + ruler so the playhead / cursor span only the
     lane area (the absolute origin is the whole `.arr-inner`). */
  .arr-end { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1px; background: color-mix(in srgb, var(--border-strong, var(--text-disabled)) 60%, transparent); border-left: 1px dashed color-mix(in srgb, var(--text-disabled) 60%, transparent); pointer-events: none; z-index: 2; }
  .arr-progress { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; background: color-mix(in srgb, var(--accent) 7%, transparent); pointer-events: none; z-index: 3; }
  .arr-cursor { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1px; background: color-mix(in srgb, var(--accent) 70%, transparent); pointer-events: none; z-index: 4; }
  .cursor-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--accent); }
  .arr-playhead { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1.5px; background: var(--text-primary); opacity: 0.7; pointer-events: none; z-index: 4; transition: opacity var(--transition-fast); }
  .arr-playhead.idle { opacity: 0.32; }
  .playhead-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--text-primary); }

  /* ── Time ruler (footer) ── */
  /* Sticky to the bottom of the scroll viewport; scrolls horizontally with the
     timeline. Shows MM:SS in parallel to the bars/beats ruler at the top. */
  .arr-time {
    position: sticky; bottom: 0; z-index: 7;
    display: flex; height: 20px;
    background: var(--bg-base);
    border-top: 1px solid var(--border-subtle);
  }
  .arr-time-corner {
    width: var(--head-w); flex-shrink: 0;
    position: sticky; left: 0; z-index: 1;
    display: flex; align-items: center; padding: 0 12px;
    background: var(--bg-base);
    border-right: 1px solid var(--border-subtle);
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-disabled);
  }
  .arr-time-track { position: relative; width: var(--tl-w); flex-shrink: 0; }
  .time-tick { position: absolute; top: 0; bottom: 0; border-left: 1px solid color-mix(in srgb, var(--border-subtle) 50%, transparent); }
  .time-tick.strong { border-left-color: var(--border-subtle); }
  .time-tick span { position: absolute; bottom: 3px; left: 3px; font-size: 8.5px; color: var(--text-disabled); font-variant-numeric: tabular-nums; }
</style>
