<script lang="ts">
  /**
   * Read-only arrangement view — Logic Pro-style, driven by the **real engine**.
   * Lanes come from `merula_query` (the last-evaluated arrangement, grouped per
   * mixer strip); each lane draws its real haps (see HapLane). The playhead +
   * cursor follow the transport store (real cycle position, not a mock RAF), the
   * ruler seeks for real (`merulaSeek`), and the timeline gently auto-follows the
   * playhead during playback.
   *
   * A frozen, collapsible track-header column (sticky-left) and a fixed
   * pixels-per-cycle timeline that continues into empty bars past the song
   * (spreadsheet-like). Named sections (`section("INTRO", …)` in the source) are
   * drawn as tinted per-lane background bands plus coloured ruler chips, tiled
   * across the timeline by `merula_query`.
   *
   * Mute/solo live in the mixer (Step 3b); the headers show **read-only status
   * icons**. Right-click a header/lane toggles mute/solo via the shared store
   * (keyed by strip index) and pushes a live `merula_set_track` override so the
   * audio responds from here too.
   *
   * Imports only shared/ui (+ the shared ContextMenu overlay) + merula-local.
   */
  import { PanelLeftClose, PanelRightClose, VolumeX, Headphones, FileInput, Pencil, Trash2, MapPin } from 'lucide-svelte';
  import MarkerRenameModal from '../shell/MarkerRenameModal.svelte';
  import type { Marker } from '../stores/transport-ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { importActions } from '../stores/import-actions.svelte';
  import ContextMenu from '$lib/components/shared/ContextMenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { tick } from 'svelte';
  import HapLane from './HapLane.svelte';
  import ArrangementToolbar from './ArrangementToolbar.svelte';
  import Minimap from './Minimap.svelte';
  import { arrangementStore, noteName, VIEW_CYCLES, type VizLane } from './arrangement.svelte';
  import { arrViewOptions, ZOOM_STEP } from './arr-view-options.svelte';
  import { laneSizes } from './lane-sizes.svelte';
  import { transportStore, merulaEngine, diagnosticsStore } from '../stores/engine.svelte';
  import { transportUiStore } from '../stores/transport-ui.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { merulaStore } from '../merula-store.svelte';
  import { mixerStore } from '../stores/mixer.svelte';
  import { inspectStore } from '../stores/inspect.svelte';
  import { symbolHighlightStore } from '../stores/symbol-highlight.svelte';
  import { editorSelectionStore } from '../stores/editor-selection.svelte';
  import { laneColor, sectionColor } from '../palette';
  import { makeByteToU16 } from '../editor/merula-lang';
  import type { MerulaQueryHap } from '$lib/ipc/merula/merula';

  // Pixels-per-cycle = a fixed base scaled by the live horizontal zoom. Reactive,
  // so every `* PX` position (ruler, lanes, overlays, HapLane) re-lays-out on zoom.
  const BASE_PX = 26;
  const PX = $derived(BASE_PX * arrViewOptions.zoom);
  // Discovery window for the query (wide enough to detect any reasonable period);
  // the store clips the result to ONE period, so the drawn timeline is a single
  // pass of the song — never N repetitions.
  const QUERY_CYCLES = VIEW_CYCLES;
  // Drawn width = the song's one-pass length (period / content), rounded up to a
  // 4-cycle bar with a readable minimum. Falls back to the discovery window only
  // while nothing has been evaluated (so the empty grid still has sensible bars).
  const VIEW = $derived.by(() => {
    const span = arrangementStore.loopCycles || arrangementStore.contentEnd;
    if (span <= 0) return QUERY_CYCLES;
    return Math.max(8, Math.ceil(span / 4) * 4);
  });
  const timelineW = $derived(VIEW * PX);

  let collapsed = $state(false);
  const headW = $derived(collapsed ? 48 : 184);

  // ── Live data: re-query on every eval (diagnostics reassign = eval happened) ──
  $effect(() => {
    void diagnosticsStore.errors; // dep: a fresh array reference is pushed on each eval
    arrangementStore.schedule(QUERY_CYCLES);
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
    if (merulaStore.isMuted(laneKey(l.track))) parts.push('muted');
    if (merulaStore.isSoloed(laneKey(l.track))) parts.push('solo');
    parts.push('right-click for mute / solo');
    return { content: laneTitle(l), description: parts.join(' · ') };
  }

  // Solo dimming computed over the REAL lanes only (the shared store is also
  // seeded with stale mock solo state, which would otherwise dim everything).
  const soloActive = $derived(lanes.some((l) => merulaStore.isSoloed(laneKey(l.track))));

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
  // Seek anchor (last ruler click / arrow seek) — global so play-from-cursor and
  // the loop watch (MerulaShell) share it.
  const cursorCycle = $derived(transportUiStore.cursor);

  // Markers (named jump points), drawn as ruler pins + faint lane guides.
  const markers = $derived(transportUiStore.markers);

  // Loop region (in arrangement cycles), drawn on the ruler + across the lanes.
  const loop = $derived(transportUiStore.loop);
  const loopActive = $derived(transportUiStore.loopActive);
  const loopStartX = $derived(loop ? headW + loop.start * PX : 0);
  const loopW = $derived(loop ? (loop.end - loop.start) * PX : 0);

  const playX   = $derived(headW + displayCycle * PX);
  const cursorX = $derived(headW + cursorCycle * PX);
  const endX    = $derived(headW + arrangementStore.contentEnd * PX);

  // ── Minimap geometry (cycle-space; the component maps cycle→percent itself) ──
  // Reactive mirror of the scroll geometry (driven by `syncView`), for the minimap
  // viewport box. `viewportW` also tracks panel resizes via a ResizeObserver.
  let scrollLeftPx = $state(0);
  let viewportW = $state(0);
  const mapCycles = $derived(Math.max(arrangementStore.contentEnd, loopCycles, 4));
  const viewStartCycle = $derived(PX > 0 ? scrollLeftPx / PX : 0);
  const viewEndCycle   = $derived(PX > 0 ? (scrollLeftPx + viewportW - headW) / PX : 0);

  const bars = $derived(Array.from({ length: Math.floor(VIEW / 4) + 1 }, (_, i) => i * 4));

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
    const snapped = Math.max(0, Math.round(cyc * 4) / 4);
    transportUiStore.setCursor(snapped);
    void merulaEngine.seek(snapped);
  }
  /** Pixel → cycle under the ruler, clamped to the visible timeline. */
  function rulerCycleAt(clientX: number): number {
    if (!rulerEl) return 0;
    const r = rulerEl.getBoundingClientRect();
    return Math.max(0, Math.min(VIEW, (clientX - r.left) / PX));
  }
  // Press-and-drag scrub: seek continuously while the mouse is down (DAW-style),
  // not just on a single click. We snap to quarter-cycles and only re-seek when
  // the snapped position actually changes, so a drag fires at most one IPC per
  // crossed beat — never a flood of per-pixel seeks. **Alt+drag sets the loop
  // region** instead of scrubbing (snapped to whole cycles / bars).
  function startScrub(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    if (e.altKey) { startLoopDrag(e); return; }
    let last = -1;
    const apply = (clientX: number) => {
      const snapped = Math.max(0, Math.round(rulerCycleAt(clientX) * 4) / 4);
      if (snapped === last) return;
      last = snapped;
      transportUiStore.setCursor(snapped);
      void merulaEngine.seek(snapped);
    };
    apply(e.clientX);
    const onMove = (ev: MouseEvent) => apply(ev.clientX);
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }
  /** Alt+drag on the ruler → define the loop region (snapped to whole cycles). */
  function startLoopDrag(e: MouseEvent) {
    const anchor = Math.round(rulerCycleAt(e.clientX));
    const apply = (clientX: number) => {
      const cur = Math.round(rulerCycleAt(clientX));
      transportUiStore.setLoop(anchor, cur);
    };
    apply(e.clientX);
    const onMove = (ev: MouseEvent) => apply(ev.clientX);
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }
  /** Wheel over the time axis = horizontal navigation (no reaching for the
   *  scrollbar). The dominant axis wins, so a trackpad's horizontal swipe still
   *  pans naturally. Shift+wheel anywhere in the timeline does the same.
   *  **Ctrl/Cmd+wheel zooms** (centred on the pointer). */
  function wheelToHorizontal(e: WheelEvent) {
    if (e.ctrlKey || e.metaKey) { void zoomAtPointer(e); return; }
    if (!scrollEl) return;
    const d = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
    if (d === 0) return;
    e.preventDefault();
    scrollEl.scrollLeft += d;
    syncView();
  }
  function onArrWheel(e: WheelEvent) {
    if (e.ctrlKey || e.metaKey) { void zoomAtPointer(e); return; }
    if (e.shiftKey) wheelToHorizontal(e);
  }

  /** Ctrl/Cmd+wheel zoom, keeping the cycle under the pointer pinned in place. */
  async function zoomAtPointer(e: WheelEvent) {
    if (!scrollEl) return;
    e.preventDefault();
    const r = scrollEl.getBoundingClientRect();
    const localX = e.clientX - r.left;                          // px within the viewport
    const c = (scrollEl.scrollLeft + localX - headW) / PX;      // cycle under the pointer
    arrViewOptions.zoomBy(e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
    await tick();                                               // let the timeline width settle
    const newPx = BASE_PX * arrViewOptions.zoom;
    scrollEl.scrollLeft = Math.max(0, headW + c * newPx - localX);
    syncView();
  }

  /** Mirror the scroll element's geometry into reactive state for the minimap. */
  function syncView() {
    if (!scrollEl) return;
    scrollLeftPx = scrollEl.scrollLeft;
    viewportW = scrollEl.clientWidth;
  }
  /** Pan so the main view is centred on `centerCycle` (from the minimap). */
  function panTo(centerCycle: number) {
    if (!scrollEl) return;
    const viewWCycles = Math.max(0, viewportW - headW) / PX;
    const targetStart = Math.max(0, centerCycle - viewWCycles / 2);
    scrollEl.scrollLeft = targetStart * PX;
    syncView();
  }

  // ── Lane resize: drag the bottom edge of a track header to grow/shrink its lane
  // (the piano-roll spreads vertically). Double-click the handle resets it. ───────
  function startLaneResize(e: MouseEvent, track: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation(); // don't let the header click select / drag-scrub
    const startY = e.clientY;
    const startH = laneSizes.height(track);
    const onMove = (ev: MouseEvent) => laneSizes.setHeight(track, startH + (ev.clientY - startY));
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  // ── Auto-follow: keep the playhead in view while playing (re-armed on each
  // play start; ANY manual horizontal scroll pins it until the next start). ──────
  let scrollEl = $state<HTMLElement | null>(null);
  let userPinned = $state(false);
  let prevPlaying = false;
  // Keep the geometry mirror fresh when the scroll element mounts / resizes.
  $effect(() => {
    if (!scrollEl) return;
    syncView();
    const ro = new ResizeObserver(() => syncView());
    ro.observe(scrollEl);
    return () => ro.disconnect();
  });
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
  // Explicit seeks WHILE STOPPED — skip-to-start / skip-to-end / step / marker jumps
  // move the cursor but the play-follow above is gated on `playing`, so the view
  // wouldn't scroll. Bring the cursor into view when it lands off-screen (e.g.
  // "skip to start" scrolls the timeline back to cycle 0). Scrubbing within the
  // viewport keeps the cursor in view, so this won't fight a drag.
  $effect(() => {
    const x = cursorX; // dep
    if (playing || !scrollEl) return;
    const vw = scrollEl.clientWidth;
    const sl = scrollEl.scrollLeft;
    if (x < sl + headW || x > sl + vw - 16) {
      scrollEl.scrollLeft = Math.max(0, x - headW - vw / 3);
      lastAutoScrollLeft = scrollEl.scrollLeft;
      syncView();
    }
  });

  /** Pin the follow when the user scrolls horizontally themselves — covers the
   *  scrollbar / trackpad-pan / keyboard cases that `wheel` alone misses. Our own
   *  programmatic scrolls match `lastAutoScrollLeft`, so they don't pin. */
  function onScroll() {
    if (playing && !userPinned && scrollEl && Math.abs(scrollEl.scrollLeft - lastAutoScrollLeft) > 2) {
      userPinned = true;
    }
    syncView();
  }

  // ── Selection: shared with the mixer + inspector via the merula store (keyed by
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
  function pickHap(lane: VizLane, hap: MerulaQueryHap) {
    mixerStore.select(lane.track);
    inspectStore.select(lane.track, hap);
    merulaStore.showRight('inspector');
  }
  // Byte→UTF-16 mapper for the active source (rebuilt when the source changes).
  // Hap spans are UTF-8 byte ranges; the editor + JS string slicing need UTF-16.
  const byteToU16 = $derived(makeByteToU16(projectStore.activeSource));

  /** Ctrl/Cmd+click a hap → reveal the source span that produced it. */
  function gotoHapSource(hap: MerulaQueryHap) {
    if (hap.span_start == null) return;
    merulaStore.requestGoto(byteToU16(hap.span_start), 0);
  }

  // ── Editor→DAW selection link ────────────────────────────────────────────────
  // Box every hap whose source span overlaps any selected region (the selected
  // text, plus a selected variable's `let` value range). Hap spans are UTF-8 bytes
  // → map to UTF-16 (the selection's space) via `byteToU16`.
  const selActive = $derived(editorSelectionStore.active);
  function hapInSelection(hap: MerulaQueryHap): boolean {
    if (!selActive || hap.span_start == null || hap.span_end == null) return false;
    return editorSelectionStore.overlaps(byteToU16(hap.span_start), byteToU16(hap.span_end));
  }

  const NOTE_PC: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };
  /** MIDI of a host note literal (`c4`=60; `s`=sharp, `f`=flat), or null. Mirrors
   *  the language's pitch convention so we can tell whether a transform moved it. */
  function noteLiteralToMidi(tok: string): number | null {
    const m = tok.toLowerCase().match(/^([a-g])([sf]*)(-?\d+)$/);
    if (!m) return null;
    let semis = NOTE_PC[m[1]];
    for (const acc of m[2]) semis += acc === 's' ? 1 : -1;
    return (parseInt(m[3], 10) + 1) * 12 + semis;
  }
  /** The written note literal behind a hap, but ONLY when a transform (e.g.
   *  `.add(-24)`) shifted its sounding pitch — so the tooltip can clarify
   *  "MIDI 33 · written a3". Returns null when the written token already matches
   *  the sounding pitch, or the span isn't a plain note literal (degrees, chords,
   *  generated notes). */
  function writtenNote(hap: MerulaQueryHap): string | null {
    if (hap.note == null || hap.span_start == null || hap.span_end == null) return null;
    const raw = projectStore.activeSource.slice(byteToU16(hap.span_start), byteToU16(hap.span_end));
    const m = raw.match(/[a-gA-G][sSfF]*-?\d+/);
    if (!m) return null;
    const tok = m[0].toLowerCase();
    const midi = noteLiteralToMidi(tok);
    if (midi == null || midi === Math.round(hap.note)) return null;
    return tok;
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
    // Ctrl+←/→ jumps between markers; plain ←/→ nudges the cursor by a cycle.
    else if (e.key === 'ArrowRight' && (e.ctrlKey || e.metaKey)) { transportUiStore.seekNextMarker(); e.preventDefault(); }
    else if (e.key === 'ArrowLeft'  && (e.ctrlKey || e.metaKey)) { transportUiStore.seekPrevMarker(); e.preventDefault(); }
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
          { id: 'mute', label: merulaStore.isMuted(laneKey(ctx.track)) ? 'Unmute' : 'Mute', icon: VolumeX },
          { id: 'solo', label: merulaStore.isSoloed(laneKey(ctx.track)) ? 'Unsolo' : 'Solo', icon: Headphones },
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

  // ── Markers: ruler pins (click → seek), context menu (rename / delete), and
  // right-click on the empty ruler to drop one. ─────────────────────────────────
  let markerCtx = $state<{ x: number; y: number; marker: Marker } | null>(null);
  let renamingMarker = $state<Marker | null>(null);
  const markerCtxItems = $derived<MenuItem[]>(
    markerCtx
      ? [
          { id: 'rename', label: 'Rename marker…', icon: Pencil },
          { id: 'delete', label: 'Delete marker', icon: Trash2 },
        ]
      : [],
  );
  function openMarkerMenu(e: MouseEvent, marker: Marker) {
    e.preventDefault();
    e.stopPropagation(); // don't also trigger the ruler's "add marker here"
    markerCtx = { x: e.clientX, y: e.clientY, marker };
  }
  function onMarkerCtxSelect(id: string) {
    if (!markerCtx) return;
    if (id === 'rename') renamingMarker = markerCtx.marker;
    else if (id === 'delete') transportUiStore.removeMarker(markerCtx.marker.id);
    markerCtx = null;
  }
  /** Right-click the ruler (not a pin) → add a marker at that cycle. */
  function addMarkerAt(e: MouseEvent) {
    e.preventDefault();
    transportUiStore.addMarker(Math.round(rulerCycleAt(e.clientX) * 4) / 4);
  }
</script>

<!-- Focusable keyboard-nav region: arrows move the selected track / scrub the
     cursor, Ctrl+arrows jump markers (see `onKeydown`). It's a grouping container,
     not a single widget, so role="group" + a roving tabindex is the right shape —
     the noninteractive-listener / tabindex rules are false positives here. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="arr" tabindex="0" role="group" aria-label="Arrangement" onkeydown={onKeydown}>
  <div class="arr-scroll" class:hide-hbar={lanes.length && arrViewOptions.minimap} bind:this={scrollEl} onscroll={onScroll} onwheel={onArrWheel}>
    <div class="arr-inner" style="--head-w: {headW}px; --tl-w: {timelineW}px;">
      <!-- View toolbar (sticky-left, stays put as the timeline scrolls) -->
      <div class="arr-toolbar-row">
        <div class="arr-toolbar-anchor">
          <button class="tb-import" onclick={() => importActions.start()}
                  use:tooltip={{ content: 'Import audio / MIDI', description: 'Transcribe a WAV or bring a .mid in as an editable .merula file' }}>
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
              <span class="corner-title">{projectStore.project?.name ?? 'merula'}</span>
              <span class="corner-sub">{lanes.length} {lanes.length === 1 ? 'track' : 'tracks'}</span>
            </div>
          {/if}
          <button class="corner-toggle" use:tooltip={collapsed ? 'Expand tracks' : 'Collapse tracks'} aria-label="Toggle track headers" onclick={() => (collapsed = !collapsed)}>
            {#if collapsed}<PanelRightClose size={14} />{:else}<PanelLeftClose size={14} />{/if}
          </button>
        </div>
        <!-- Scrub strip: drag seeks the cursor (keyboard scrubbing is handled by
             the container's arrow-key nav). role="slider" reflects that to AT. -->
        <div class="arr-ruler" bind:this={rulerEl} onmousedown={startScrub} oncontextmenu={addMarkerAt} onwheel={wheelToHorizontal}
             role="slider" aria-label="Playback cursor" aria-valuemin={0} aria-valuemax={VIEW} aria-valuenow={cursorCycle} tabindex="-1"
             use:tooltip={'Drag to scrub · Alt-drag to set a loop · right-click to add a marker · wheel to scroll'}>
          {#if loop}
            <div class="ruler-loop" class:off={!loopActive} style="left: {loop.start * PX}px; width: {loopW}px;"></div>
          {/if}
          {#each markers as m (m.id)}
            <button class="ruler-marker" style="left: {m.cycle * PX}px;"
                    onmousedown={(e) => e.stopPropagation()}
                    onclick={() => seekTo(m.cycle)}
                    oncontextmenu={(e) => openMarkerMenu(e, m)}
                    use:tooltip={`Marker “${m.label}” · click to seek · right-click to rename / delete`}
                    aria-label={`Marker ${m.label}`}>
              <MapPin size={9} /><span class="ml">{m.label}</span>
            </button>
          {/each}
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
          {@const muted = merulaStore.isMuted(laneKey(lane.track))}
          {@const soloed = merulaStore.isSoloed(laneKey(lane.track))}
          {@const dimmed = muted || (soloActive && !soloed)}
          <div class="arr-row" class:selected={selectedTrack === lane.track} class:sym-hl={symbolHighlightStore.has(lane.track)} style="--c: {color}; height: {laneSizes.height(lane.track)}px">
            <div class="arr-head" class:collapsed
                 role="button" tabindex="0"
                 onclick={() => selectTrack(lane.track)}
                 onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); selectTrack(lane.track); } }}
                 oncontextmenu={(e) => openMenu(e, lane.track)} use:tooltip={laneInfo(lane)}>
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
              <!-- Resize the lane: drag the bottom edge; double-click resets it. -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="arr-resize" class:custom={laneSizes.isCustom(lane.track)}
                   onmousedown={(e) => startLaneResize(e, lane.track)}
                   ondblclick={(e) => { e.stopPropagation(); laneSizes.reset(lane.track); }}
                   use:tooltip={'Drag to resize the lane · double-click to reset'}></div>
            </div>
            <!-- The lane body is a backdrop whose click is a convenience track-select
                 (also on the head button + container arrows). Its real interactive
                 targets are the hap blocks inside (HapLane), so it must NOT take a
                 button role itself — that would nest interactive roles. -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
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
                       selectedKey={selectedKeyFor(lane)} inSelection={hapInSelection}
                       onpick={(h) => pickHap(lane, h)}
                       ongoto={gotoHapSource} {writtenNote} />
            </div>
          </div>
        {/each}

        {#if !lanes.length}
          <div class="arr-empty">
            {#if arrangementStore.loaded}
              <span>No arrangement yet — evaluate a <code>.merula</code> file to see its tracks.</span>
            {:else}
              <span>Loading arrangement…</span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Overlays -->
      {#if lanes.length}
        {#if loop}
          <div class="arr-loop" class:off={!loopActive} style="left: {loopStartX}px; width: {loopW}px;"></div>
          <div class="arr-loop-edge" style="left: {loopStartX}px;"></div>
          <div class="arr-loop-edge" style="left: {loopStartX + loopW}px;"></div>
        {/if}
        {#each markers as m (m.id)}
          <div class="arr-marker-guide" style="left: {headW + m.cycle * PX}px;"></div>
        {/each}
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

  <!-- Minimap: a fixed (non-scrolling) overview strip with a draggable viewport. -->
  {#if lanes.length && arrViewOptions.minimap}
    <div class="arr-minimap">
      <Minimap {lanes} {mapCycles} viewStart={viewStartCycle} viewEnd={viewEndCycle}
               playCycle={playing ? displayCycle : -1} {cursorCycle} {loop} onPan={panTo} />
    </div>
  {/if}
</div>

{#if ctx}
  <ContextMenu items={ctxItems} x={ctx.x} y={ctx.y} onSelect={onCtxSelect} onClose={() => (ctx = null)} />
{/if}

{#if markerCtx}
  <ContextMenu items={markerCtxItems} x={markerCtx.x} y={markerCtx.y} onSelect={onMarkerCtxSelect} onClose={() => (markerCtx = null)} />
{/if}

{#if renamingMarker}
  <MarkerRenameModal marker={renamingMarker} onClose={() => (renamingMarker = null)} />
{/if}

<style>
  .arr {
    /* Header strip matches the editor tab bar height (32px) so the two panes'
       tops line up. */
    --ruler-h: 32px;
    /* View-toolbar row above the ruler (offsets the absolute overlays below). */
    --tb-h: 30px;
    display: flex;
    flex-direction: column;
    flex: 1; min-width: 0; min-height: 0;
    background: var(--bg-base);
    outline: none;
  }
  .arr:focus-visible { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent); }
  .arr-scroll { flex: 1; min-width: 0; min-height: 0; overflow: auto; }
  /* Hide the native HORIZONTAL scrollbar ONLY while the minimap is shown — then the
     minimap (drag its viewport box) is the horizontal scroll control, so a raw
     scrollbar wedged between the timeline and the minimap is redundant. With the
     minimap off, the native scrollbar comes back so horizontal scrolling stays
     reachable. Native scrolling (wheel / trackpad / programmatic) works either way;
     vertical keeps the app's themed scrollbar. */
  .arr-scroll.hide-hbar::-webkit-scrollbar:horizontal { height: 0; }
  /* Minimap: a fixed-height overview strip directly below the timeline (now flush,
     with no scrollbar between them). */
  .arr-minimap {
    flex-shrink: 0;
    height: 40px;
    padding: 4px 6px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
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
    font-size: var(--font-size-xs); font-weight: 600; cursor: pointer;
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
  .corner-title { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); line-height: 1.1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .corner-sub { font-size: var(--font-size-3xs); color: var(--text-muted); }
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
    cursor: ew-resize;
    user-select: none;
  }
  .ruler-tick { position: absolute; top: 21px; bottom: 0; border-left: 1px solid color-mix(in srgb, var(--border-subtle) 60%, transparent); }
  .ruler-tick.strong { border-left-color: var(--border-subtle); }
  .ruler-tick.hide-line { border-left-color: transparent; }
  .ruler-tick span { position: absolute; bottom: 1px; left: 3px; font-size: var(--font-size-3xs); color: var(--text-disabled); font-variant-numeric: tabular-nums; }

  /* Named-section chips: a coloured label strip along the top of the ruler. */
  .ruler-chip {
    position: absolute; top: 2px; height: 13px;
    background: color-mix(in srgb, var(--sc) 26%, transparent);
    border-left: 2px solid var(--sc); border-radius: 0 3px 3px 0;
    overflow: hidden; pointer-events: none;
  }
  .ruler-chip span {
    display: block; padding: 0 5px; line-height: 13px;
    font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase;
    color: var(--text-secondary); white-space: nowrap;
  }

  /* Loop region band along the bottom of the ruler (visual only — Alt-drag sets
     it, the toolbar toggle enables/disables, Esc clears). Dimmed when switched off. */
  .ruler-loop {
    position: absolute; left: 0; bottom: 0; height: 5px;
    background: var(--accent); border-radius: 2px;
    pointer-events: none; opacity: 0.9;
  }
  .ruler-loop.off { opacity: 0.3; }

  /* Marker pin on the ruler (click → seek, right-click → rename / delete). */
  .ruler-marker {
    position: absolute; top: 1px; z-index: 2;
    display: inline-flex; align-items: center; gap: 2px;
    height: 13px; padding: 0 3px 0 1px;
    background: color-mix(in srgb, var(--info) 28%, var(--bg-base));
    border: none; border-left: 2px solid var(--info); border-radius: 0 3px 3px 0;
    color: var(--info); cursor: pointer; white-space: nowrap;
    transition: background var(--transition-fast);
  }
  .ruler-marker:hover { background: color-mix(in srgb, var(--info) 42%, var(--bg-base)); }
  .ruler-marker:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .ruler-marker .ml { font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.3px; color: var(--text-secondary); }

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
  /* Neon track spine: the lane accent glows so the DAW reads vivid, not flat. */
  .arr-colorbar { width: 4px; align-self: stretch; background: var(--c); flex-shrink: 0; box-shadow: 0 0 8px color-mix(in srgb, var(--c) 70%, transparent); }
  /* Lane resize grip — a thin zone along the header's bottom edge. Invisible until
     hovered; accent-tinted once the lane has a custom height. */
  .arr-resize {
    position: absolute;
    left: 0; right: 0; bottom: 0;
    height: 6px;
    cursor: row-resize;
    z-index: 6;
  }
  .arr-resize::after {
    content: '';
    position: absolute;
    left: 0; right: 0; bottom: 0;
    height: 2px;
    background: transparent;
    transition: background var(--transition-fast);
  }
  .arr-resize:hover::after { background: color-mix(in srgb, var(--accent) 60%, transparent); }
  .arr-resize.custom::after { background: color-mix(in srgb, var(--accent) 28%, transparent); }
  .arr-resize.custom:hover::after { background: color-mix(in srgb, var(--accent) 60%, transparent); }
  .arr-head-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; padding-left: 4px; }
  .arr-name { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arr-voice { font-size: var(--font-size-2xs); color: var(--text-muted); font-family: var(--font-code); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

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
    color: var(--text-muted); font-size: var(--font-size-sm);
  }
  .arr-empty code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); }

  /* ── Overlays ── */
  /* Start below the toolbar row + ruler so the playhead / cursor span only the
     lane area (the absolute origin is the whole `.arr-inner`). */
  .arr-end { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1px; background: color-mix(in srgb, var(--border-strong, var(--text-disabled)) 60%, transparent); border-left: 1px dashed color-mix(in srgb, var(--text-disabled) 60%, transparent); pointer-events: none; z-index: 2; }
  .arr-progress { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; background: color-mix(in srgb, var(--accent) 7%, transparent); pointer-events: none; z-index: 3; }
  /* Loop region overlay across the lanes — a faint accent wash + bright edges. */
  .arr-loop { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; background: color-mix(in srgb, var(--accent) 9%, transparent); pointer-events: none; z-index: 2; }
  .arr-loop.off { background: color-mix(in srgb, var(--text-disabled) 6%, transparent); }
  .arr-loop-edge { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1px; background: color-mix(in srgb, var(--accent) 55%, transparent); pointer-events: none; z-index: 3; }
  /* Marker guide line through the lanes (paired with the ruler pin above it). */
  .arr-marker-guide { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1px; background: color-mix(in srgb, var(--info) 35%, transparent); pointer-events: none; z-index: 2; }
  .arr-cursor { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1px; background: color-mix(in srgb, var(--accent) 70%, transparent); pointer-events: none; z-index: 4; }
  .cursor-flag { position: absolute; top: 0; left: -4px; width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 6px solid var(--accent); }
  .arr-playhead { position: absolute; top: calc(var(--tb-h) + var(--ruler-h)); bottom: 0; width: 1.5px; background: var(--accent); opacity: 0.85; pointer-events: none; z-index: 4; transition: opacity var(--transition-fast); box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 80%, transparent); }
  .arr-playhead.idle { opacity: 0.4; box-shadow: none; }
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
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-disabled);
  }
  .arr-time-track { position: relative; width: var(--tl-w); flex-shrink: 0; }
  .time-tick { position: absolute; top: 0; bottom: 0; border-left: 1px solid color-mix(in srgb, var(--border-subtle) 50%, transparent); }
  .time-tick.strong { border-left-color: var(--border-subtle); }
  .time-tick span { position: absolute; bottom: 3px; left: 3px; font-size: var(--font-size-3xs); color: var(--text-disabled); font-variant-numeric: tabular-nums; }
</style>
