<script lang="ts">
  /**
   * TransfersOverlay — a floating panel listing in-flight (and just-finished)
   * downloads & exports with a real progress bar each, distinct from the generic
   * Jobs overlay (which has no percent). Driven entirely by the shared
   * `transfersStore`; any window that mounts it via <FeedbackHost> gets the same
   * surface. Merula is the first consumer (sample-pack downloads + WAV exports).
   *
   * Shared/top-level: an Arbor-specific overlay opened from a footer/status badge
   * (`uiStore.transfersOverlayOpen`), like JobsOverlay / NotificationsOverlay.
   */
  import {
    Download, FileDown, FileInput, CheckCircle, XCircle, StopCircle, Trash2, FolderOpen,
  } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { transfersStore, type Transfer } from '$lib/feedback/stores/transfers.svelte';
  import { revealFile, openFolder } from '$lib/utils/reveal';

  // A 1 Hz clock so elapsed time + ETA tick while the overlay is open (it's only
  // mounted when open, so the interval lives exactly that long).
  let now = $state(Date.now());
  onMount(() => {
    const id = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  /** `m:ss` (seconds zero-padded). */
  function fmtDur(secs: number): string {
    const t = Math.max(0, Math.round(secs));
    return `${Math.floor(t / 60)}:${(t % 60).toString().padStart(2, '0')}`;
  }
  /** Seconds since the transfer started (frozen at its terminal time). */
  function elapsed(t: Transfer): number {
    return ((t.endedAt ?? now) - t.startedAt) / 1000;
  }
  /** Rate-based estimate of seconds remaining, or `null` when not yet estimable
   *  (indeterminate, just started, or already finishing). Refines as progress
   *  streams in — no upfront guess needed. */
  function eta(t: Transfer): number | null {
    if (t.state !== 'active' || t.progress == null || t.progress < 3 || t.progress >= 100) return null;
    const el = elapsed(t);
    return Math.max(0, (el / (t.progress / 100)) - el);
  }

  // Reveal a finished transfer in the file explorer — the exported WAV is
  // selected in its folder; an installed pack opens its folder. Honours the
  // built-in-vs-OS explorer choice (Settings → File Explorer).
  function reveal(t: Transfer) {
    if (!t.path) return;
    // Exports and imports point at a file (the WAV / the .mid) → select it; a
    // download points at an installed folder → open it.
    void (t.kind === 'download' ? openFolder(t.path) : revealFile(t.path));
  }

  const transfers = $derived(transfersStore.transfers);

  function phaseLabel(t: Transfer): string {
    if (t.state === 'done') return t.kind === 'download' ? 'Installed' : t.kind === 'import' ? 'Imported' : 'Exported';
    if (t.state === 'failed') return t.error ? `Failed — ${t.error}` : 'Failed';
    if (t.state === 'cancelled') return 'Cancelled';
    if (t.sublabel) return t.sublabel;
    return t.kind === 'download' ? 'Downloading…' : 'Exporting…';
  }
  function phaseColor(t: Transfer): string {
    if (t.state === 'done') return 'var(--success)';
    if (t.state === 'failed') return 'var(--error)';
    if (t.state === 'cancelled') return 'var(--text-muted)';
    return 'var(--text-secondary)';
  }
</script>

<button type="button" aria-label="Close overlay" class="overlay-backdrop"
        onclick={() => uiStore.setTransfersOverlayOpen(false)}></button>

<div class="overlay-panel transfers-overlay" role="dialog" aria-label="Downloads & Exports">
  <div class="overlay-header">
    <span class="overlay-title">Downloads &amp; Exports</span>
    <div class="header-actions">
      {#if transfersStore.finishedCount > 0}
        <button class="clear-btn" onclick={() => transfersStore.clearFinished()} use:tooltip={'Clear finished'}>
          <Trash2 size={13} /><span>Clear</span>
        </button>
      {/if}
      <button class="close-btn" onclick={() => uiStore.setTransfersOverlayOpen(false)}
              use:tooltip={'Close'} aria-label="Close"></button>
    </div>
  </div>

  {#if transfers.length === 0}
    <div class="empty-state">No downloads or exports yet</div>
  {:else}
    <div class="t-list">
      {#each transfers as t (t.id)}
        {@const active = t.state === 'active'}
        {@const remain = eta(t)}
        <div class="t-row" class:inactive={!active}>
          <span class="t-icon">
            {#if t.state === 'done'}<CheckCircle size={15} class="t-ok" />
            {:else if t.state === 'failed'}<XCircle size={15} class="t-err" />
            {:else if t.state === 'cancelled'}<StopCircle size={15} class="t-muted" />
            {:else if t.kind === 'download'}<Download size={15} class="t-accent" />
            {:else if t.kind === 'import'}<FileInput size={15} class="t-accent" />
            {:else}<FileDown size={15} class="t-accent" />{/if}
          </span>

          <div class="t-body">
            <div class="t-head">
              <span class="t-label" title={t.label}>{t.label}</span>
              {#if active && t.progress != null}
                <span class="t-pct">{Math.round(t.progress)}%</span>
              {/if}
            </div>
            {#if active}
              <ProgressBar pct={t.progress ?? undefined} indeterminate={t.progress == null}
                           ariaLabel={`${t.label} progress`} />
            {/if}
            <div class="t-foot">
              <span class="t-phase" style="color: {phaseColor(t)}">{phaseLabel(t)}</span>
              <span class="t-time">{fmtDur(elapsed(t))}{#if remain != null} · ~{fmtDur(remain)} left{/if}</span>
            </div>
          </div>

          <div class="t-actions">
            {#if t.state === 'done' && t.path}
              <button class="t-btn" use:tooltip={'Reveal in file explorer'} aria-label="Reveal in file explorer"
                      onclick={() => reveal(t)}><FolderOpen size={13} /></button>
            {/if}
            {#if active && t.cancel}
              <button class="t-btn danger" use:tooltip={'Stop'} aria-label="Stop transfer"
                      onclick={() => transfersStore.requestCancel(t.id)}><StopCircle size={13} /></button>
            {:else if !active}
              <button class="t-btn" use:tooltip={'Dismiss'} aria-label="Dismiss"
                      onclick={() => transfersStore.remove(t.id)}><Trash2 size={12} /></button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .transfers-overlay { width: 320px; max-height: 440px; background: var(--bg-base); }

  .header-actions { display: flex; align-items: center; gap: 4px; }
  .clear-btn {
    display: flex; align-items: center; gap: 4px;
    height: 22px; padding: 0 6px;
    border: none; background: transparent; color: var(--text-muted);
    border-radius: var(--radius-sm); cursor: pointer;
    font-size: var(--font-size-xs); font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .clear-btn:hover { background: var(--bg-elevated); color: var(--text-primary); }

  .t-list {
    overflow-y: auto; flex: 1 1 auto; min-height: 0;
    padding: 4px; display: flex; flex-direction: column; gap: 2px;
  }

  .t-row {
    display: flex; align-items: flex-start; gap: 10px;
    padding: 8px 8px;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), opacity var(--transition-base);
  }
  .t-row:hover { background: var(--bg-elevated); }
  .t-row.inactive { opacity: 0.7; }
  .t-row.inactive:hover { opacity: 1; }

  .t-icon { display: flex; align-items: center; flex-shrink: 0; margin-top: 1px; }

  .t-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .t-head { display: flex; align-items: baseline; gap: 8px; }
  .t-label {
    flex: 1; min-width: 0;
    font-size: var(--font-size-sm); color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .t-pct { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; flex-shrink: 0; }
  .t-foot { display: flex; align-items: baseline; gap: 8px; }
  .t-phase { flex: 1; min-width: 0; font-size: var(--font-size-2xs); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .t-time { flex-shrink: 0; font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .t-actions { display: flex; align-items: center; flex-shrink: 0; }
  .t-btn {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .t-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .t-btn.danger:hover { color: var(--error); }

  :global(.t-ok)     { color: var(--success); }
  :global(.t-err)    { color: var(--error); }
  :global(.t-muted)  { color: var(--text-muted); }
  :global(.t-accent) { color: var(--accent); }
</style>
