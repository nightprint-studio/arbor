<script lang="ts">
  /**
   * Jobs — the merula window's background-job output panel. Renders the same job
   * stream the footer badge counts (offline WAV renders, sample-pack downloads),
   * now with a real listing + live output drill-in instead of just a titlebar
   * spinner / footer badge.
   *
   * Reuses the shared, per-window feedback infrastructure end-to-end: the
   * `jobsStore` (already mounted + target-routed by `<FeedbackHost id="merula">`
   * in MerulaWindow) is the single source of truth, and the output drill-in
   * composes the shared `LogStream`. Only the panel chrome is merula-local
   * because the bottom-panel routing is merula-specific (merulaStore). If a second
   * window ever needs this same "jobs as a docked panel" surface, this is the
   * candidate to lift into shared/.
   */
  import {
    Boxes, StopCircle, X, ExternalLink, ChevronLeft, Trash2,
    CheckCircle, XCircle, Loader, ArrowDownToLine, Copy, Check,
  } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import LogStream from '$lib/components/shared/ui/LogStream.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import type { JobInfo } from '$lib/feedback/types/jobs';
  import { stripAnsi } from '$lib/utils/ansi-to-html';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { merulaStore } from '../merula-store.svelte';

  // merula jobs are never hidden, but keep the showHidden filter honest in case
  // a future merula job opts in — mirror the overlay's listing semantics.
  const visibleJobs = $derived(
    jobsStore.showHidden ? jobsStore.jobs : jobsStore.jobs.filter(j => !j.hidden),
  );
  const runningCount = $derived(visibleJobs.filter(j => j.status.type === 'running').length);

  // ── Live ticker (elapsed time) ──────────────────────────────────────────────
  let tick = $state(0);
  $effect(() => {
    if (!visibleJobs.some(j => j.status.type === 'running')) return;
    const id = setInterval(() => { tick++; }, 1000);
    return () => clearInterval(id);
  });

  function elapsed(job: JobInfo): string {
    void tick; // re-evaluate each second while a job runs
    const secs = Math.floor(Date.now() / 1000) - job.started_at;
    if (secs < 60) return `${secs}s`;
    return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  }

  // ── Output drill-in ─────────────────────────────────────────────────────────
  const activeJob = $derived<JobInfo | undefined>(
    visibleJobs.find(j => j.id === jobsStore.activeJobId),
  );
  const lines = $derived<string[]>(
    jobsStore.activeJobId ? (jobsStore.outputs[jobsStore.activeJobId] ?? []) : [],
  );

  let logStream: LogStream | undefined = $state();
  let autoScroll = $state(true);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function openOutput(job: JobInfo) {
    await jobsStore.loadOutput(job.id);
    jobsStore.setActiveJob(job.id);
    autoScroll = true;
  }
  function backToList() { jobsStore.setActiveJob(null); }

  function toggleFollow() {
    if (autoScroll) autoScroll = false;
    else logStream?.scrollToBottom();
  }
  function lineClass(line: string): string | undefined {
    return line.startsWith('[stderr]') ? 'line-stderr' : undefined;
  }
  async function copyOutput() {
    if (!lines.length) return;
    await copyToClipboard(lines.map(l => stripAnsi(l)).join('\n'));
    copied = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copied = false; }, 1800);
  }

  function statusLabel(j: JobInfo): string {
    switch (j.status.type) {
      case 'running':   return 'Running…';
      case 'completed': return `Exited ${j.status.exit_code}`;
      case 'failed':    return `Failed: ${j.status.error}`;
      case 'cancelled': return 'Cancelled';
    }
  }
  function statusColor(j: JobInfo): string {
    if (j.status.type === 'running') return 'var(--accent)';
    if (j.status.type === 'completed' && j.status.exit_code === 0) return 'var(--success)';
    if (j.status.type === 'cancelled') return 'var(--text-muted)';
    return 'var(--error)';
  }
</script>

<div class="jobs">
  {#if activeJob}
    <!-- ── Output drill-in ─────────────────────────────────────────────────── -->
    <BottomPanelHeader title={activeJob.name} onClose={() => merulaStore.toggleBottom('jobs')}>
      {#snippet icon()}
        <button class="ps-btn" use:tooltip={'Back to jobs'} onclick={backToList} aria-label="Back to jobs">
          <ChevronLeft size={14} />
        </button>
      {/snippet}
      {#snippet children()}
        <span class="job-sep"></span>
        <span class="job-status" style="color: {statusColor(activeJob)}">{statusLabel(activeJob)}</span>
      {/snippet}
      {#snippet actions()}
        <button class="ps-btn" class:ps-btn-active={autoScroll} onclick={toggleFollow}
          use:tooltip={autoScroll ? 'Following — click to pause' : 'Follow output'} aria-label="Follow output">
          <ArrowDownToLine size={13} />
        </button>
        {#if lines.length > 0}
          <button class="ps-btn" class:ps-btn-success={copied} onclick={copyOutput}
            use:tooltip={'Copy output'} aria-label="Copy output">
            {#if copied}<Check size={13} />{:else}<Copy size={13} />{/if}
          </button>
        {/if}
        {#if activeJob.status.type === 'running' && !activeJob.non_cancellable}
          <button class="ps-btn ps-btn-danger" onclick={() => activeJob && jobsStore.cancel(activeJob.id)}
            use:tooltip={'Cancel job'} aria-label="Cancel job">
            <StopCircle size={13} />
          </button>
        {/if}
      {/snippet}
    </BottomPanelHeader>

    <div class="out-cmd">
      <span class="out-cmd-label">$</span>
      <span class="out-cmd-text">{activeJob.command}</span>
    </div>
    <div class="out-body">
      <LogStream
        bind:this={logStream}
        bind:autoScroll
        {lines}
        {lineClass}
        emptyMessage="No output captured."
        waiting={activeJob.status.type === 'running'}
        waitingMessage="Waiting for output…"
      />
    </div>
  {:else}
    <!-- ── Job listing ─────────────────────────────────────────────────────── -->
    <BottomPanelHeader title="Jobs" onClose={() => merulaStore.toggleBottom('jobs')}>
      {#snippet icon()}<Boxes size={13} />{/snippet}
      {#snippet children()}
        <span class="jobs-meta">
          {#if runningCount > 0}{runningCount} running{:else}{visibleJobs.length} total{/if}
        </span>
      {/snippet}
      {#snippet actions()}
        {#if jobsStore.finishedCount > 0}
          <button class="ps-btn" onclick={() => jobsStore.clearFinished()}
            use:tooltip={'Clear finished'} aria-label="Clear finished jobs">
            <Trash2 size={13} />
          </button>
        {/if}
      {/snippet}
    </BottomPanelHeader>

    <div class="jobs-body">
      {#if visibleJobs.length === 0}
        <EmptyState message="No jobs yet — WAV renders and sample-pack downloads show up here." />
      {:else}
        <div class="job-list">
          {#each visibleJobs as job (job.id)}
            {@const running = job.status.type === 'running'}
            <button class="job-row" class:inactive={!running} onclick={() => openOutput(job)}>
              <span class="job-icon">
                {#if running}
                  <Loader size={14} class="spin-icon job-accent" />
                {:else if job.status.type === 'completed' && job.status.exit_code === 0}
                  <CheckCircle size={14} class="job-ok" />
                {:else if job.status.type === 'cancelled'}
                  <StopCircle size={14} class="job-muted" />
                {:else}
                  <XCircle size={14} class="job-err" />
                {/if}
              </span>

              <span class="job-main">
                <span class="job-name">
                  {job.name}
                  {#if job.category}<span class="job-cat">{job.category}</span>{/if}
                </span>
                <span class="job-status-line" style="color: {statusColor(job)}">{statusLabel(job)}</span>
              </span>

              {#if running}<span class="job-time">{elapsed(job)}</span>{/if}

              <span class="job-actions">
                {#if running}
                  {#if !job.non_cancellable}
                    <button class="ps-btn ps-btn-danger" use:tooltip={'Cancel'} aria-label="Cancel job"
                      onclick={(e) => { e.stopPropagation(); jobsStore.cancel(job.id); }}>
                      <StopCircle size={13} />
                    </button>
                  {/if}
                {:else}
                  <button class="ps-btn" use:tooltip={'Dismiss'} aria-label="Dismiss job"
                    onclick={(e) => { e.stopPropagation(); jobsStore.dismiss(job.id); }}>
                    <X size={13} />
                  </button>
                {/if}
                <span class="ps-btn open-hint" use:tooltip={'View output'}><ExternalLink size={13} /></span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .jobs { display: flex; flex-direction: column; height: 100%; width: 100%; overflow: hidden; background: var(--bg-base); }

  .jobs-meta {
    font-size: 11px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .jobs-body { flex: 1; min-height: 0; overflow-y: auto; }

  /* ── Listing ───────────────────────────────────────────────────────────── */
  .job-list { display: flex; flex-direction: column; padding: 4px; gap: 2px; }

  .job-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: inherit;
    transition: background var(--transition-fast), opacity var(--transition-base);
  }
  .job-row:hover { background: var(--bg-elevated); }
  .job-row.inactive { opacity: 0.6; }
  .job-row.inactive:hover { opacity: 1; }

  .job-icon { display: flex; align-items: center; flex-shrink: 0; color: var(--text-muted); }

  .job-main { display: flex; flex-direction: column; gap: 1px; flex: 1; min-width: 0; }
  .job-name {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .job-cat {
    font-size: 9px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    padding: 0 5px;
    flex-shrink: 0;
  }
  .job-status-line { font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .job-time {
    font-size: 10px; color: var(--accent);
    font-variant-numeric: tabular-nums; flex-shrink: 0;
  }

  .job-actions { display: flex; align-items: center; gap: 2px; flex-shrink: 0; }
  .open-hint { opacity: 0; pointer-events: none; }
  .job-row:hover .open-hint { opacity: 0.7; }

  :global(.job-ok)     { color: var(--success); }
  :global(.job-err)    { color: var(--error); }
  :global(.job-muted)  { color: var(--text-muted); }
  :global(.job-accent) { color: var(--accent); }
  :global(.spin-icon)  { animation: spin 1s linear infinite; }

  /* ── Output drill-in ───────────────────────────────────────────────────── */
  .job-sep { display: inline-block; width: 1px; height: 14px; background: var(--border-subtle); margin: 0 2px; flex-shrink: 0; }
  .job-status { font-size: 10px; font-weight: 500; white-space: nowrap; }

  .out-cmd {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 12px;
    background: rgba(0, 0, 0, 0.18);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 11px;
    overflow: hidden;
  }
  .out-cmd-label { color: var(--accent); font-weight: 700; flex-shrink: 0; }
  .out-cmd-text { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--text-muted); }

  .out-body { display: flex; flex-direction: column; flex: 1; min-height: 0; overflow: hidden; }

  :global(.out-body .log-line.line-stderr) { color: var(--terminal-bright-red, #e06c6c); }
</style>
