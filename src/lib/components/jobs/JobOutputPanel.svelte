<script lang="ts">
  import { StopCircle, ChevronLeft, Copy, Check, ArrowDownToLine } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import LogStream from '$lib/components/shared/ui/LogStream.svelte';
  import { jobsStore } from '$lib/stores/jobs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { JobInfo } from '$lib/types/jobs';
  import { stripAnsi } from '$lib/utils/ansi-to-html';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';

  const job = $derived<JobInfo | undefined>(
    jobsStore.jobs.find(j => j.id === jobsStore.activeJobId)
  );

  // Back-button target: when a plugin panel (e.g. run-monitor) drilled
  // INTO the job output, return to that panel. Falls back to the legacy
  // behaviour (open the host JobsOverlay, close the bottom slot) when we
  // got here directly from the overlay or via a deep-link.
  function goBack() {
    jobsStore.setActiveJob(null);
    const prev = uiStore.previousBottomSection;
    if (typeof prev === 'string' && prev.startsWith('plugin:')) {
      uiStore.setActiveBottomSection(prev);
    } else {
      uiStore.setJobsOverlayOpen(true);
      uiStore.setActiveBottomSection(null);
    }
  }
  const lines = $derived<string[]>(
    jobsStore.activeJobId ? (jobsStore.outputs[jobsStore.activeJobId] ?? []) : []
  );

  let logStream: LogStream | undefined = $state();
  let autoScroll = $state(true);

  function toggleFollow() {
    if (autoScroll) {
      autoScroll = false;
    } else {
      logStream?.scrollToBottom();
    }
  }

  function lineClass(line: string): string | undefined {
    return line.startsWith('[stderr]') ? 'line-stderr' : undefined;
  }

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyOutput() {
    if (!lines.length) return;
    const text = lines.map(l => stripAnsi(l)).join('\n');
    await copyToClipboard(text);
    copied = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copied = false; }, 1800);
  }

  function statusLabel(j: JobInfo): string {
    switch (j.status.type) {
      case 'running':   return 'Running…';
      case 'completed': return `Exited ${(j.status as any).exit_code}`;
      case 'failed':    return `Failed: ${(j.status as any).error}`;
      case 'cancelled': return 'Cancelled';
    }
  }

  function statusColor(j: JobInfo): string {
    if (j.status.type === 'running') return 'var(--accent)';
    if (j.status.type === 'completed' && (j.status as any).exit_code === 0) return 'var(--success)';
    return 'var(--error)';
  }
</script>

<div class="jop-root">
  <BottomPanelHeader title={job?.name ?? 'Output'}>
    {#snippet icon()}
      <button
        class="ps-btn"
        use:tooltip={'Back'}
        onclick={goBack}
      >
        <ChevronLeft size={14} />
      </button>
    {/snippet}
    {#if job}
      <span class="jop-sep"></span>
      <span class="jop-status" style="color: {statusColor(job)}">{statusLabel(job)}</span>
      <span class="jop-plugin">{job.plugin_name}</span>
    {/if}
    {#snippet actions()}
      <button
        class="jop-action-btn follow-btn"
        class:active={autoScroll}
        use:tooltip={autoScroll ? 'Following — click to pause' : 'Follow output'}
        onclick={toggleFollow}
      >
        <ArrowDownToLine size={13} />
        <span>Follow</span>
      </button>
      {#if lines.length > 0}
        <button
          class="jop-action-btn"
          class:copied
          use:tooltip={'Copy output'}
          onclick={copyOutput}
        >
          {#if copied}
            <Check size={13} />
            <span>Copied</span>
          {:else}
            <Copy size={13} />
            <span>Copy</span>
          {/if}
        </button>
      {/if}
      {#if job?.status.type === 'running' && !job?.non_cancellable}
        <button
          class="jop-action-btn"
          use:tooltip={'Cancel job'}
          onclick={() => job && jobsStore.cancel(job.id)}
        >
          <StopCircle size={13} />
          <span>Cancel</span>
        </button>
      {/if}
    {/snippet}
  </BottomPanelHeader>

  <div class="jop-body">
    {#if job}
      <div class="jop-cmd">
        <span class="jop-cmd-label">$</span>
        <span class="jop-cmd-text">{job.command}</span>
      </div>
    {/if}

    <LogStream
      bind:this={logStream}
      bind:autoScroll
      {lines}
      {lineClass}
      emptyMessage="No output captured."
      waiting={job?.status.type === 'running'}
      waitingMessage="Waiting for output…"
    />
  </div>
</div>

<style>
  .jop-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }

  .jop-sep {
    display: inline-block;
    width: 1px;
    height: 14px;
    background: var(--border-subtle);
    flex-shrink: 0;
    margin: 0 2px;
  }

  .jop-status {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 500;
    white-space: nowrap;
  }

  .jop-plugin {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--text-muted);
    background: var(--bg-base);
    border-radius: var(--radius-sm);
    padding: 0 5px;
    white-space: nowrap;
  }

  .jop-action-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 7px;
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .jop-action-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .jop-action-btn.copied { color: var(--success); }
  .follow-btn { color: var(--text-disabled); }
  .follow-btn.active { color: var(--accent); background: var(--accent-subtle); }
  .follow-btn.active:hover { background: var(--accent-subtle); color: var(--accent-hover); }

  .jop-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }

  .jop-cmd {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 12px;
    background: rgba(0,0,0,0.18);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
  }

  .jop-cmd-label {
    color: var(--accent);
    font-weight: 700;
    flex-shrink: 0;
  }

  .jop-cmd-text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-muted);
  }

  /* stderr lines get a subtle red tint on text not already coloured by ANSI spans */
  :global(.jop-body .log-line.line-stderr) {
    color: var(--terminal-bright-red, #e06c6c);
  }
</style>
