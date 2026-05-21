<script lang="ts">
  /**
   * Aggregate progress modal for the cloud-storage plugin's bulk operations.
   *
   * Listens to:
   *   - `arbor://cloud-many-progress` — per-file + aggregate state, fired by
   *     the Rust download_many task AND by chunk-handler plugins during
   *     their merge phase (via `arbor.cloud.report_progress`).
   *   - `arbor://cloud-many-done` — terminal state with the final ok flag and
   *     the list of local paths (the chunk-handler plugin uses the same
   *     `cloud-storage:download-many-done` Lua hook for its own bookkeeping;
   *     this Tauri event is the modal-only side of the dual emit).
   *
   * Lifecycle: mounts itself via `arbor://cloud-many-progress` (auto-open on
   * first progress payload), closes 1.5s after `ok=true` done, stays open on
   * error until the user clicks "Close". Cancel button flips the cloud
   * cancellation flag for the stream id — the Rust task tears down at the
   * next chunk boundary and the chunk-handler service polls the same flag
   * via `arbor.cloud.is_cancelled`.
   */
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { X, Download, Combine, CheckCircle, AlertCircle, Loader, FileText, Maximize2, Minimize2 } from 'lucide-svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { CloudManyFileState, CloudManyAggregate } from '$lib/types/cloud';

  type Phase = 'download' | 'merge';

  interface ProgressPayload {
    stream_id:   string;
    op_label?:   string;
    phase?:      Phase;
    files?:      CloudManyFileState[];
    aggregate?:  CloudManyAggregate;
    merge_note?: string | null;
  }
  interface DonePayload {
    stream_id:   string;
    ok:          boolean;
    error?:      string | null;
    local_paths: string[];
  }

  let visible      = $state(false);
  let collapsed    = $state(false);
  let streamId     = $state<string | null>(null);
  let opLabel      = $state('');
  let phase        = $state<Phase>('download');
  let files        = $state<CloudManyFileState[]>([]);
  let aggregate    = $state<CloudManyAggregate | null>(null);
  let mergeNote    = $state<string | null>(null);
  let done         = $state(false);
  let ok           = $state(false);
  let errorMessage = $state<string | null>(null);

  // Speed/ETA derived from delta between two consecutive progress reads.
  // Server doesn't compute them so we don't need to redo a rolling window
  // on the JS side — last sample is good enough at the 4 Hz emit cadence.
  let lastBytes  = $state(0);
  let lastTime   = $state(0);
  let speedBps   = $state(0);

  function reset() {
    visible      = false;
    collapsed    = false;
    streamId     = null;
    opLabel      = '';
    phase        = 'download';
    files        = [];
    aggregate    = null;
    mergeNote    = null;
    done         = false;
    ok           = false;
    errorMessage = null;
    lastBytes    = 0;
    lastTime     = 0;
    speedBps     = 0;
  }

  function onProgress(p: ProgressPayload) {
    // If a new stream begins, take over the modal.
    if (!visible || (streamId && p.stream_id !== streamId && !done)) {
      reset();
    }
    streamId = p.stream_id;
    visible  = true;
    if (p.op_label) opLabel = p.op_label;
    if (p.phase)    phase   = p.phase;
    if (p.files)    files   = p.files;
    if (p.merge_note !== undefined) mergeNote = p.merge_note ?? null;
    if (p.aggregate) {
      const now = Date.now();
      if (lastTime > 0) {
        const dt = (now - lastTime) / 1000;
        const db = p.aggregate.bytes_done - lastBytes;
        if (dt > 0) speedBps = Math.max(0, Math.round(db / dt));
      }
      lastBytes = p.aggregate.bytes_done;
      lastTime  = now;
      aggregate = p.aggregate;
    }
    // Reset auto-close timer if we get a late update after `done`.
    if (done && p.phase === 'merge') {
      done         = false;
      errorMessage = null;
    }
  }

  function onDone(p: DonePayload) {
    if (streamId && p.stream_id !== streamId) return;
    done = true;
    ok   = p.ok;
    if (!p.ok) errorMessage = p.error ?? 'unknown error';
    if (p.ok && phase === 'download') {
      // Don't auto-close yet — the chunk-handler may switch us into the
      // merge phase right after. The plugin's `report_progress` flips
      // `done` back to false. If no merge phase comes within 1.5s, auto-close.
      setTimeout(() => { if (done && ok && phase === 'download') reset(); }, 1500);
    } else if (p.ok) {
      // Already in merge phase + ok → final close.
      setTimeout(() => { if (done && ok) reset(); }, 1500);
    }
  }

  async function cancel() {
    if (!streamId) return;
    try { await invoke('cloud_is_cancelled', { streamId }); } catch { /* nop */ }
    try { await invoke('cancel_job', { jobId: streamId }); } catch { /* nop */ }
    // The aggregate's cancel flag is also registered under stream_id directly,
    // so flip via the cloud-specific helper as a safety net.
    try { await invoke('cloud_cancel', { streamId }); } catch { /* nop */ }
  }

  // ── Lifecycle ────────────────────────────────────────────────────────────

  let unlistenProgress: UnlistenFn | null = null;
  let unlistenDone:     UnlistenFn | null = null;

  onMount(() => {
    let alive = true;
    (async () => {
      unlistenProgress = await listen<ProgressPayload>('arbor://cloud-many-progress', e => {
        if (alive) onProgress(e.payload);
      });
      unlistenDone = await listen<DonePayload>('arbor://cloud-many-done', e => {
        if (alive) onDone(e.payload);
      });
    })();
    return () => {
      alive = false;
      unlistenProgress?.();
      unlistenDone?.();
    };
  });

  // ── Formatting ───────────────────────────────────────────────────────────

  function humanBytes(n: number): string {
    if (!n || n <= 0) return '0 B';
    const u = ['B','KB','MB','GB','TB'];
    let i = 0; let v = n;
    while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
    return `${v.toFixed(i === 0 ? 0 : 1)} ${u[i]}`;
  }
  function formatEta(): string {
    if (!aggregate || speedBps <= 0) return '';
    const remaining = aggregate.bytes_total - aggregate.bytes_done;
    if (remaining <= 0) return '';
    const secs = Math.round(remaining / speedBps);
    if (secs < 60)  return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs/60)}m ${secs%60}s`;
    return `${Math.floor(secs/3600)}h ${Math.floor((secs%3600)/60)}m`;
  }
  function percent(done: number, total: number): number {
    if (total <= 0) return 0;
    return Math.min(100, Math.round((done / total) * 100));
  }
</script>

{#if visible}
  <!-- Floating non-blocking widget anchored bottom-left. NOT role="dialog";
       the rest of the app stays clickable while the transfer runs. The
       chip-row shows minimal state when collapsed (label + percent + cancel);
       the full body expands per-file rows on demand. -->
  <div class="cdp-floater" role="status" aria-live="polite" class:collapsed>
    <header class="cdp-head">
      <button class="cdp-toggle"
              onclick={() => collapsed = !collapsed}
              aria-label={collapsed ? 'Expand' : 'Collapse'}>
        {#if done && ok}
          <CheckCircle size={14} class="cdp-success" />
        {:else if done && !ok}
          <AlertCircle size={14} class="cdp-error" />
        {:else if phase === 'merge'}
          <Combine size={14} class="cdp-accent" />
        {:else}
          <Download size={14} class="cdp-accent" />
        {/if}
      </button>
      <div class="cdp-head-text">
        <div class="cdp-title">{opLabel || (phase === 'merge' ? 'Merging…' : 'Downloading…')}</div>
        <div class="cdp-sub">
          {#if done && ok}
            Done · {aggregate ? humanBytes(aggregate.bytes_total) : ''}
          {:else if done && !ok}
            Failed · {errorMessage}
          {:else if aggregate}
            {humanBytes(aggregate.bytes_done)} / {humanBytes(aggregate.bytes_total)}
            {#if speedBps > 0} · {humanBytes(speedBps)}/s{/if}
            {#if formatEta()} · ETA {formatEta()}{/if}
          {/if}
        </div>
      </div>
      {#if !done}
        <button class="cdp-cancel" onclick={cancel} aria-label="Cancel">
          Cancel
        </button>
      {:else}
        <button class="cdp-close" onclick={reset} aria-label="Close">
          <X size={12} />
        </button>
      {/if}
      <button class="cdp-min"
              onclick={() => collapsed = !collapsed}
              aria-label={collapsed ? 'Expand' : 'Collapse'}>
        {#if collapsed}
          <Maximize2 size={11} />
        {:else}
          <Minimize2 size={11} />
        {/if}
      </button>
    </header>

    {#if aggregate}
      <div class="cdp-aggregate-bar">
        <div class="cdp-aggregate-fill"
             style="width: {percent(aggregate.bytes_done, aggregate.bytes_total)}%"></div>
      </div>
    {/if}

    {#if !collapsed}
      <!-- Phase indicator: shown when a chunk-handler is involved. -->
      <div class="cdp-phase" class:cdp-phase-merge={phase === 'merge'}>
        <div class="cdp-phase-step" class:cdp-phase-active={phase === 'download'}
                                    class:cdp-phase-done={phase !== 'download'}>
          <Download size={11} /> Download
        </div>
        <div class="cdp-phase-arrow">→</div>
        <div class="cdp-phase-step" class:cdp-phase-active={phase === 'merge'}>
          <Combine size={11} /> Merge
        </div>
      </div>

      {#if phase === 'merge' && mergeNote}
        <div class="cdp-merge-note">{mergeNote}</div>
      {/if}

      {#if phase === 'download' && files.length > 0}
        <ul class="cdp-files">
          {#each files as f (f.index)}
            <li class="cdp-file cdp-file-{f.status}">
              <FileText size={11} class="cdp-file-icon" />
              <span class="cdp-file-name" title={f.basename}>{f.basename}</span>
              <span class="cdp-file-status">
                {#if f.status === 'downloading'}
                  <Loader size={10} class="cdp-spin" />
                  {percent(f.bytes_done, f.bytes_total)}%
                {:else if f.status === 'done'}
                  <CheckCircle size={10} class="cdp-success" />
                {:else if f.status === 'failed'}
                  <AlertCircle size={10} class="cdp-error" />
                {:else if f.status === 'cancelled'}
                  cancelled
                {:else}
                  queued
                {/if}
              </span>
              <div class="cdp-file-bar">
                <div class="cdp-file-bar-fill"
                     style="width: {percent(f.bytes_done, f.bytes_total)}%"></div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>
{/if}

<style>
  /* Floating bottom-left widget — sits ABOVE the status bar (~28px) so it
     doesn't get clipped, well below modal z-index so dialogs still hide it.
     Width is generous enough to show file names but not modal-large. */
  .cdp-floater {
    position: fixed;
    left: 12px;
    bottom: 36px;
    z-index: 900;
    width: 380px; max-width: calc(100vw - 24px);
    max-height: 60vh;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 22px rgba(0,0,0,.35);
    overflow: hidden;
    display: flex; flex-direction: column;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    transition: max-height var(--transition-fast);
  }
  .cdp-floater.collapsed {
    max-height: 56px;
  }
  .cdp-head {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .cdp-floater.collapsed .cdp-head { border-bottom: none; }
  .cdp-toggle {
    background: transparent; border: none; padding: 0;
    color: var(--text-secondary); cursor: pointer;
    display: inline-flex; align-items: center;
  }
  .cdp-head-text { flex: 1; min-width: 0; }
  .cdp-title {
    font-size: 12px; font-weight: 600;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .cdp-sub {
    font-size: 10px; color: var(--text-secondary);
    margin-top: 1px; font-family: var(--font-code);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .cdp-cancel, .cdp-close {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    cursor: pointer;
    font-size: 10px;
  }
  .cdp-cancel:hover { color: var(--error); border-color: var(--error); }
  .cdp-close { padding: 2px 4px; }
  .cdp-close:hover { color: var(--text-primary); }
  .cdp-min {
    background: transparent; border: none;
    color: var(--text-muted); cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm);
    display: inline-flex; align-items: center;
  }
  .cdp-min:hover { color: var(--text-primary); background: var(--bg-hover); }

  :global(.cdp-accent)  { color: var(--accent); }
  :global(.cdp-success) { color: var(--success); }
  :global(.cdp-error)   { color: var(--error); }

  /* ── Phase indicator ─────────────────────────────────────────────────── */
  .cdp-phase {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 16px;
    font-size: 10px; font-weight: 600; letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .cdp-phase-step {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 6px;
    border-radius: 999px;
    background: var(--bg-overlay);
  }
  .cdp-phase-active { color: var(--accent); background: var(--accent-subtle); }
  .cdp-phase-done   { color: var(--success); background: color-mix(in srgb, var(--success) 12%, transparent); }
  .cdp-phase-arrow  { color: var(--text-disabled); font-size: 11px; }

  /* ── Aggregate bar ───────────────────────────────────────────────────── */
  /* Stays visible even when collapsed so the chip itself doubles as a
     progress indicator. */
  .cdp-aggregate-bar {
    margin: 0;
    height: 3px;
    background: var(--bg-overlay);
    overflow: hidden;
  }
  .cdp-floater:not(.collapsed) .cdp-aggregate-bar {
    margin: 8px 10px;
    border-radius: 3px;
  }
  .cdp-aggregate-fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms linear;
  }

  .cdp-merge-note {
    padding: 0 16px 12px;
    font-size: 11px; color: var(--text-secondary);
    font-family: var(--font-code);
  }

  /* ── Per-file rows ───────────────────────────────────────────────────── */
  .cdp-files {
    list-style: none; margin: 0;
    padding: 0 16px 14px;
    overflow-y: auto;
    flex: 1; min-height: 0;
  }
  .cdp-file {
    display: grid;
    grid-template-columns: 14px 1fr auto;
    grid-template-rows: auto 3px;
    gap: 2px 6px;
    align-items: center;
    padding: 4px 0;
    font-size: 11px;
    border-bottom: 1px solid var(--border-subtle, transparent);
  }
  :global(.cdp-file-icon) { color: var(--text-muted); }
  .cdp-file-name {
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    color: var(--text-primary);
  }
  .cdp-file-status {
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10px;
    display: inline-flex; align-items: center; gap: 4px;
    white-space: nowrap;
  }
  .cdp-file-bar {
    grid-column: 1 / -1;
    height: 2px;
    background: var(--bg-overlay);
    border-radius: 2px;
    overflow: hidden;
  }
  .cdp-file-bar-fill {
    height: 100%; background: var(--accent);
    transition: width 150ms linear;
  }
  .cdp-file-done    .cdp-file-name   { color: var(--text-secondary); }
  .cdp-file-failed  .cdp-file-name   { color: var(--error); }
  .cdp-file-cancelled .cdp-file-name { color: var(--text-disabled); }

  :global(.cdp-spin) { animation: cdp-spin 1.1s linear infinite; }
  @keyframes cdp-spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
