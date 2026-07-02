<script lang="ts">
  /**
   * RecordingHud — the standalone recording control surface (window `tyto-hud`).
   *
   * Shown while a video recording runs *with the Tyto window hidden* (so Tyto's UI
   * isn't captured). Two layouts the user toggles at will:
   *  • **compact** — a slim Windows-style pill (REC · elapsed · pause · stop),
   *  • **expanded** — a card that also shows the recording subject + larger controls.
   * The shell owns the window size/placement, so toggling calls `resizeRecordingHud`.
   *
   * Elapsed comes from the engine's `tyto://recording-progress` ticks; pause/stop go
   * back through the tyto-be rpc. Opaque, draggable (`data-tauri-drag-region`), and
   * content-protected by the window itself.
   */
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Square, Pause, Play, Maximize2, Minimize2 } from 'lucide-svelte';
  import { listenRecordingProgress, listenRecordingError, stopRecording, pauseRecording } from '$lib/ipc/tyto/recorder';
  import { resizeRecordingHud, getHudInit } from '$lib/ipc/tyto/hud-window';
  import { formatDuration } from '$lib/stores/tyto/recorder.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  let elapsed = $state(0);
  let paused = $state(false);
  let expanded = $state(false);
  let stopping = $state(false);
  let busy = $state(false); // guards the pause toggle round-trip
  let targetLabel = $state('');
  let lost = $state(false); // capture source went away — auto-stopping + saving

  onMount(() => {
    // Standalone window: apply the app theme/appearance so the HUD matches the main
    // window (else it falls back to hardcoded defaults).
    void themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    const uns: Array<() => void> = [];
    void listenRecordingProgress((p) => { if (!paused) elapsed = p.elapsed_ms; }).then((f) => uns.push(f));
    // The capture source was lost (monitor unplugged / resolution switch): stop now so
    // the partial recording is saved instead of freezing on the last frame forever.
    void listenRecordingError(() => { if (!lost) { lost = true; void stop(); } }).then((f) => uns.push(f));
    void getHudInit().then((i) => { targetLabel = i.target_label ?? ''; }).catch(() => {});
    // Reveal (anti-white-flash) is handled centrally in +page.svelte via window_ready.
    return () => { for (const u of uns) u(); };
  });

  async function togglePause() {
    if (stopping || busy) return;
    busy = true;
    const next = !paused;
    try {
      await pauseRecording(next);
      paused = next;
    } catch { /* engine may be mid-transition — leave state as-is */ }
    busy = false;
  }

  async function toggleExpanded() {
    expanded = !expanded;
    try { await resizeRecordingHud(expanded); } catch { /* shell handles bounds */ }
  }

  async function stop() {
    if (stopping) return;
    stopping = true;
    // Stop the engine first (finalizes + muxes the file), then tear down the HUD and
    // bring Tyto back (which refreshes its library + highlights the new capture).
    try { await stopRecording(); } catch { /* engine may already be stopping */ }
    try { await invoke('close_recording_hud'); } catch { /* shell already handled it */ }
  }

  // Keyboard-first: the HUD is a tiny focused window, so a couple of keys cover it.
  function onKeyDown(e: KeyboardEvent) {
    if (e.key === ' ' || e.code === 'Space') { e.preventDefault(); void togglePause(); }
    else if (e.key === 'Escape') { e.preventDefault(); void stop(); }
    else if (e.key.toLowerCase() === 'e') { e.preventDefault(); void toggleExpanded(); }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="hud" class:expanded data-tauri-drag-region>
  {#if expanded}
    <div class="top" data-tauri-drag-region>
      <span class="rec" class:paused>
        <span class="dot"></span>{paused ? 'PAUSED' : 'REC'}
      </span>
      {#if targetLabel}<span class="target" title={targetLabel}>{targetLabel}</span>{/if}
      <button type="button" class="icon-btn ghost" onclick={toggleExpanded} title="Compact view" aria-label="Compact view">
        <Minimize2 size={14} />
      </button>
    </div>

    <div class="big-time">{formatDuration(elapsed)}</div>

    <div class="actions">
      <button type="button" class="ctl" onclick={togglePause} disabled={stopping || busy}>
        {#if paused}<Play size={14} fill="currentColor" /> Resume{:else}<Pause size={14} fill="currentColor" /> Pause{/if}
      </button>
      <button type="button" class="ctl stop" onclick={stop} disabled={stopping} title="Stop and save">
        <Square size={13} fill="currentColor" /> Stop
      </button>
    </div>
  {:else}
    <span class="rec" class:paused>
      <span class="dot"></span>{paused ? 'PAUSED' : 'REC'}
    </span>
    <span class="time">{formatDuration(elapsed)}</span>
    <button type="button" class="icon-btn" onclick={togglePause} disabled={stopping || busy} title={paused ? 'Resume' : 'Pause'} aria-label={paused ? 'Resume' : 'Pause'}>
      {#if paused}<Play size={14} fill="currentColor" />{:else}<Pause size={14} fill="currentColor" />{/if}
    </button>
    <button type="button" class="stop pill" onclick={stop} disabled={stopping} title="Stop recording and save the file">
      <Square size={12} fill="currentColor" /> Stop
    </button>
    <button type="button" class="icon-btn ghost" onclick={toggleExpanded} title="Expanded view" aria-label="Expanded view">
      <Maximize2 size={13} />
    </button>
  {/if}

  {#if lost}
    <div class="lost"><span class="dot"></span> Source lost — saving…</div>
  {/if}
</div>

<style>
  /* The HUD window is OPAQUE (a transparent WebView2 window gets no input on Windows
     — the documented trap), so its backing surface is what shows outside any rounded
     content. A default-white backing was leaking around a rounded pill as "white
     borders"; painting html/body with the elevated colour and filling the window
     edge-to-edge removes them entirely. Windows 11 softens the outer window corners
     on its own. */
  :global(html), :global(body) {
    margin: 0; height: 100%; overflow: hidden;
    background: var(--bg-elevated, #12151d);
  }

  .hud {
    position: relative;
    box-sizing: border-box;
    width: 100vw; height: 100vh;
    background: var(--bg-elevated, #12151d);
    color: var(--text-primary, #f3f5f9);
    user-select: none; -webkit-user-select: none;
    cursor: default;
    /* A subtle top sheen + inset hairline reads as a floating bar without a rounded
       edge that would reveal the opaque backing. */
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05), inset 0 0 0 1px rgba(255, 255, 255, 0.02);
  }

  /* Compact bar */
  .hud:not(.expanded) {
    display: flex; align-items: center; gap: 11px;
    padding: 0 8px 0 14px;
  }

  /* Expanded card */
  .hud.expanded {
    display: flex; flex-direction: column;
    padding: 11px 13px 13px;
    gap: 6px;
  }

  .rec {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: 12px; font-weight: 700; letter-spacing: 0.5px;
    color: var(--error, #e5484d);
    flex-shrink: 0;
  }
  .rec.paused { color: var(--warning, #f5a623); }
  .dot {
    width: 9px; height: 9px; border-radius: 50%;
    background: currentColor;
    animation: hud-pulse 1.3s ease-in-out infinite;
  }
  .rec.paused .dot { animation: none; opacity: 0.85; }
  @keyframes hud-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.25; } }

  .time {
    flex: 1;
    font-size: 15px; font-weight: 600; font-variant-numeric: tabular-nums;
    color: var(--text-primary, #f3f5f9);
  }

  /* Expanded header + big timer */
  .top { display: flex; align-items: center; gap: 8px; }
  .target {
    flex: 1; min-width: 0;
    font-size: 11.5px; color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .big-time {
    flex: 1;
    display: flex; align-items: center; justify-content: center;
    font-size: 40px; font-weight: 700; font-variant-numeric: tabular-nums;
    letter-spacing: 1px;
    color: var(--text-primary);
  }
  .actions { display: flex; gap: 8px; }

  /* Buttons */
  .icon-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 30px; height: 30px; flex-shrink: 0;
    border: none; border-radius: 999px; cursor: pointer;
    background: var(--bg-hover, #20242e); color: var(--text-secondary);
    transition: background var(--transition-fast, 0.12s), color var(--transition-fast, 0.12s);
  }
  .icon-btn:hover:not(:disabled) { background: var(--bg-overlay, #2a2f3a); color: var(--text-primary); }
  .icon-btn.ghost { background: transparent; }
  .icon-btn.ghost:hover:not(:disabled) { background: var(--bg-hover, #20242e); }
  .icon-btn:disabled { opacity: 0.5; cursor: default; }

  .stop {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    border: none; cursor: pointer;
    background: var(--error, #e5484d); color: #fff;
    font-size: 12px; font-weight: 650;
    transition: filter var(--transition-fast, 0.12s);
  }
  .stop.pill { height: 30px; padding: 0 14px; border-radius: 999px; flex-shrink: 0; }
  .stop:hover:not(:disabled) { filter: brightness(1.12); }
  .stop:disabled { opacity: 0.6; cursor: default; }

  .ctl {
    flex: 1;
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    height: 34px; border-radius: var(--radius-md, 9px);
    border: 1px solid var(--border, #2a2f3a); cursor: pointer;
    background: var(--bg-hover, #20242e); color: var(--text-primary);
    font-size: 12.5px; font-weight: 600;
    transition: background var(--transition-fast, 0.12s), filter var(--transition-fast, 0.12s);
  }
  .ctl:hover:not(:disabled) { background: var(--bg-overlay, #2a2f3a); }
  .ctl:disabled { opacity: 0.55; cursor: default; }
  /* Two classes → beats the single-class `.ctl`/`.stop` backgrounds regardless of
     source order, so the expanded Stop stays red. */
  .ctl.stop { border-color: transparent; background: var(--error, #e5484d); color: #fff; }
  .ctl.stop:hover:not(:disabled) { background: var(--error, #e5484d); filter: brightness(1.12); }

  /* Capture-lost overlay: covers the bar while the partial recording is saved. */
  .lost {
    position: absolute; inset: 0;
    display: flex; align-items: center; justify-content: center; gap: 8px;
    background: var(--bg-elevated, #12151d);
    color: var(--warning, #f5a623);
    font-size: 12px; font-weight: 650;
  }
  .lost .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--warning, #f5a623); animation: hud-pulse 1s ease-in-out infinite; }
</style>
