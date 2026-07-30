<script lang="ts">
  import { CheckCircle, XCircle, Loader, X, StopCircle, ExternalLink } from 'lucide-svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import type { JobInfo } from '$lib/feedback/types/jobs';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    job: JobInfo;
    /** Pre-computed elapsed string — the parent owns the 1s ticker so a single
     *  interval drives every row rather than one timer per row. */
    elapsed: string;
    /** True for rows nested under a category header (extra left indent). */
    indent?: boolean;
    onOpenOutput: (job: JobInfo) => void;
  }
  let { job, elapsed, indent = false, onOpenOutput }: Props = $props();

  const icon = $derived<'running' | 'ok' | 'err' | 'cancelled'>(
    job.status.type === 'running'   ? 'running'
    : job.status.type === 'completed' ? (job.status.exit_code === 0 ? 'ok' : 'err')
    : job.status.type === 'cancelled' ? 'cancelled'
    : 'err'
  );
</script>

<div class="job-row" class:inactive={job.status.type !== 'running'} class:indent>
  <div class="job-left">
    <div class="job-icon">
      {#if icon === 'running'}
        <Loader size={13} class="spin-icon accent" />
      {:else if icon === 'ok'}
        <CheckCircle size={13} class="icon-ok" />
      {:else if icon === 'cancelled'}
        <StopCircle size={13} class="icon-muted" />
      {:else}
        <XCircle size={13} class="icon-err" />
      {/if}
    </div>
    <div class="job-progress">
      {#if job.status.type === 'running'}
        <span class="job-time">{elapsed}</span>
      {:else if job.status.type === 'completed'}
        <span class="exit-code" class:exit-ok={job.status.exit_code === 0} class:exit-err={job.status.exit_code !== 0}>
          exit {job.status.exit_code}
        </span>
      {:else if job.status.type === 'cancelled'}
        <span class="exit-cancelled">cancelled</span>
      {:else}
        <span class="exit-code exit-err">failed</span>
      {/if}
    </div>
  </div>

  <div class="job-info">
    <div class="job-name">
      {job.name}
      {#if job.status.type === 'running'}
        <span class="live-badge">LIVE</span>
      {/if}
    </div>
    <div class="job-meta">
      <span class="job-plugin">{job.plugin_name}</span>
    </div>
  </div>

  <div class="job-actions">
    {#if job.status.type === 'running'}
      {#if !job.non_cancellable}
        <button class="btn-icon danger" use:tooltip={'Stop'} onclick={() => jobsStore.cancel(job.id)}>
          <StopCircle size={12} />
        </button>
      {/if}
    {:else}
      <button class="btn-icon" use:tooltip={'Dismiss'} onclick={() => jobsStore.dismiss(job.id)}>
        <X size={12} />
      </button>
    {/if}
    <button class="btn-icon" use:tooltip={'View output'} onclick={() => onOpenOutput(job)}>
      <ExternalLink size={12} />
    </button>
  </div>
</div>

<style>
  .job-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), opacity var(--transition-base);
  }
  .job-row:hover { background: var(--bg-elevated); }

  .job-row.inactive { opacity: 0.45; }
  .job-row.inactive:hover { opacity: 0.8; }

  /* Rows nested under a category header sit indented to align with the label. */
  .job-row.indent { padding-left: 26px; }

  /* Left column: icon + progress info */
  .job-left {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    min-width: 44px;
  }

  .job-icon { display: flex; align-items: center; flex-shrink: 0; color: var(--text-muted); }

  .job-progress { display: flex; align-items: center; }

  .job-time { font-size: var(--font-size-2xs); color: var(--accent); font-variant-numeric: tabular-nums; }

  .exit-code {
    font-size: var(--font-size-3xs);
    font-family: var(--font-code);
    font-weight: 600;
    border-radius: var(--radius-sm);
    padding: 1px 4px;
  }
  .exit-code.exit-ok  { color: var(--success); background: color-mix(in srgb, var(--success) 12%, transparent); }
  .exit-code.exit-err { color: var(--error);   background: color-mix(in srgb, var(--error)   12%, transparent); }

  .exit-cancelled {
    font-size: var(--font-size-3xs);
    font-family: var(--font-code);
    font-weight: 600;
    border-radius: var(--radius-sm);
    padding: 1px 4px;
    color: var(--text-secondary);
    background: color-mix(in srgb, var(--text-muted) 14%, transparent);
  }

  .job-info { flex: 1; min-width: 0; }

  .job-name {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .live-badge {
    font-size: var(--font-size-3xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    flex-shrink: 0;
  }

  .job-meta { display: flex; gap: 5px; margin-top: 1px; }

  .job-plugin {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }

  .job-actions { display: flex; gap: 2px; flex-shrink: 0; }
</style>
