<script lang="ts">
  import {
    GitBranch, Clock, ExternalLink, RotateCcw,
    CheckCircle, XCircle, Circle, Ban, AlertCircle,
    ChevronRight, Link2,
  } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import BrandIcon from '$lib/components/shared/ui/BrandIcon.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { fetchCiJobs } from '$lib/ipc/pipeline';
  import type { CiRun, CiJob } from '$lib/types/pipeline';
  import { pipelinesStore } from '$lib/stores/pipelines.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    run:          CiRun;
    tabId:        string;
    onClose:      () => void;
    onRetrigger?: () => void;
  }

  let { run: initialRun, tabId, onClose, onRetrigger }: Props = $props();

  // Track the latest version of the run from the store so status / duration
  // update live when the parent refreshes the CI runs (or when our own poll
  // does). Falls back to the prop snapshot if the store hasn't loaded it.
  const run = $derived(
    pipelinesStore.ciRuns.find(r => r.id === initialRun.id) ?? initialRun,
  );

  const RunStatusIcon = $derived(statusIcon(run.status));

  function isTerminal(s: string): boolean {
    return s === 'success' || s === 'failed' || s === 'cancelled';
  }

  // ── Job loading ────────────────────────────────────────────────────────────
  let jobs      = $state<CiJob[]>([]);
  let loading   = $state(true);
  let loadError = $state<string | null>(null);

  async function loadJobs(showSpinner: boolean) {
    if (showSpinner) loading = true;
    try {
      jobs = await fetchCiJobs(tabId, initialRun.id);
      loadError = null;
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-load whenever the run ID changes (e.g. retrigger refreshed the list).
    initialRun.id;
    loadJobs(true);
  });

  // While the run is still in-flight, refresh the jobs list AND the parent's
  // CI runs every few seconds so running/pending stages animate forward
  // instead of being frozen at the snapshot taken when the modal opened.
  // Paused while the window is unfocused — same rationale as the panel-level
  // poll: don't burn CI API rate limit when the user can't see the modal.
  $effect(() => {
    if (isTerminal(run.status)) return;
    if (!uiStore.appFocused) return;
    const id = setInterval(() => {
      loadJobs(false);
      pipelinesStore.refreshCiRuns(tabId).catch(() => { /* ignore */ });
    }, 4000);
    return () => clearInterval(id);
  });

  // ── Stage grouping ─────────────────────────────────────────────────────────
  interface StageGroup { name: string; jobs: CiJob[]; status: string; }

  const stages = $derived.by<StageGroup[]>(() => {
    // GitHub/GitLab return jobs newest-first. Sort ascending by numeric ID
    // so the lowest-ID (earliest) job is seen first; Map preserves insertion
    // order, which means the first-executed stage ends up as the first
    // entry — rendering leftmost in the flow row. NO `.reverse()` needed:
    // a previous version had one and inverted the row in the wrong direction.
    const sorted = [...jobs].sort((a, b) => (parseInt(a.id) || 0) - (parseInt(b.id) || 0));
    const map = new Map<string, CiJob[]>();
    for (const j of sorted) {
      if (!map.has(j.stage)) map.set(j.stage, []);
      map.get(j.stage)!.push(j);
    }
    return Array.from(map.entries())
      .map(([name, stageJobs]) => ({
        name,
        jobs: stageJobs,
        status: aggregateStatus(stageJobs.map(j => j.status)),
      }));
  });

  // ── Helpers ────────────────────────────────────────────────────────────────
  function aggregateStatus(statuses: string[]): string {
    if (statuses.some(s => s === 'failed'))    return 'failed';
    if (statuses.some(s => s === 'running'))   return 'running';
    if (statuses.some(s => s === 'pending'))   return 'pending';
    if (statuses.every(s => s === 'success'))  return 'success';
    if (statuses.every(s => s === 'cancelled')) return 'cancelled';
    return 'pending';
  }

  function statusLabel(s: string): string {
    switch (s) {
      case 'success':   return 'Passed';
      case 'failed':    return 'Failed';
      case 'running':   return 'Running';
      case 'cancelled': return 'Cancelled';
      default:          return 'Pending';
    }
  }

  function statusIcon(s: string) {
    switch (s) {
      case 'success':   return CheckCircle;
      case 'failed':    return XCircle;
      // 'running' is rendered with <Spinner> instead of an icon.
      case 'cancelled': return Ban;
      default:          return Circle;
    }
  }

  function formatDuration(secs: number | null): string {
    if (secs == null) return '';
    if (secs < 60)   return `${secs.toFixed(0)}s`;
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}m ${s.toString().padStart(2, '0')}s`;
  }

  function timeAgo(iso: string): string {
    const ms = Date.now() - new Date(iso).getTime();
    if (ms < 60_000)     return 'just now';
    if (ms < 3_600_000)  return `${Math.floor(ms / 60_000)}m ago`;
    if (ms < 86_400_000) return `${Math.floor(ms / 3_600_000)}h ago`;
    return `${Math.floor(ms / 86_400_000)}d ago`;
  }
</script>

<Modal {onClose} width="min(90vw, 900px)" height="80vh" ariaLabel="Pipeline detail">
  {#snippet header()}
    <ModalHeader {onClose}>
      <!-- Provider icon — GitHub follows currentColor (themable), GitLab uses
           the absolute brand orange via `.provider-icon-gitlab`. <BrandIcon>
           inherits the wrapper's `color` so the rule still applies. -->
      <span class="provider-icon" class:provider-icon-gitlab={run.provider === 'gitlab'}>
        <BrandIcon brand={run.provider} size={16} />
      </span>

      <div class="modal-title-block">
        <span class="modal-run-name">{run.name}</span>
        <div class="modal-meta">
          <span class="meta-chip branch-chip">
            <GitBranch size={10} />
            {run.branch}
          </span>
          <span class="meta-chip sha-chip">
            <ChevronRight size={10} />
            {run.commit_sha}
          </span>
          {#if run.duration_secs}
            <span class="meta-chip dur-chip">
              <Clock size={10} />
              {formatDuration(run.duration_secs)}
            </span>
          {/if}
          <span class="meta-chip time-chip">{timeAgo(run.created_at)}</span>
        </div>
      </div>

      <span class="status-badge status-{run.status}">
        {#if run.status === 'running'}
          <Spinner size={12} color="currentColor" />
        {:else}
          <RunStatusIcon size={12} />
        {/if}
        {statusLabel(run.status)}
      </span>

      {#snippet actions()}
        {#if onRetrigger}
          <button class="hdr-btn" use:tooltip={'Re-trigger'} onclick={onRetrigger}>
            <RotateCcw size={13} />
            Re-run
          </button>
        {/if}
        <button
          class="hdr-btn hdr-btn-link"
          type="button"
          use:tooltip={'Copy arbor:// link to this run'}
          onclick={() => copyDeepLink({ kind: 'pipeline_open', runId: String(run.id) }, tabId)}
          aria-label="Copy arbor:// link"
        >
          <Link2 size={13} />
        </button>
        <button
          class="hdr-btn hdr-btn-link"
          type="button"
          use:tooltip={'Open in browser'}
          onclick={() => openUrl(run.web_url).catch(() => {})}
        >
          <ExternalLink size={13} />
          Open
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <!-- Body: stage/job graph -->
  {#if loading}
    <div class="body-state">
      <Spinner size="lg" label="Loading jobs…" block />
    </div>

  {:else if loadError}
    <div class="body-state">
      <AlertCircle size={22} class="state-icon state-warn" />
      <span class="state-hint">{loadError}</span>
    </div>

  {:else if stages.length === 0}
    <div class="body-state">
      <Circle size={22} class="state-icon state-muted" />
      <span class="state-hint">No jobs found for this run.</span>
    </div>

  {:else}
    <div class="stage-flow">
      {#each stages as stage, i (stage.name)}
        <!-- Arrow connector -->
        {#if i > 0}
          <div class="stage-arrow" aria-hidden="true">
            <svg width="24" height="24" viewBox="0 0 24 24">
              <line x1="2" y1="12" x2="18" y2="12" stroke="var(--text-muted)" stroke-width="1.5"/>
              <polygon points="22,12 16,8 16,16" fill="var(--text-muted)"/>
            </svg>
          </div>
        {/if}

        <!-- Stage column -->
        <div class="stage-col status-border-{stage.status}">
          <div class="stage-header">
            <span class="stage-name">{stage.name}</span>
            <span class="stage-status-dot dot-{stage.status}"></span>
          </div>

          <div class="job-list">
            {#each stage.jobs as job (job.id)}
              {@const JobStatusIcon = statusIcon(job.status)}
              <button
                class="job-card"
                class:job-allow-fail={job.allow_failure}
                type="button"
                onclick={() => openUrl(job.web_url).catch(() => {})}
                use:tooltip={job.allow_failure ? { content: job.name, description: 'Allowed to fail' } : job.name}
              >
                <span class="job-status-icon status-{job.status}">
                  {#if job.status === 'running'}
                    <Spinner size={13} color="currentColor" />
                  {:else}
                    <JobStatusIcon size={13} />
                  {/if}
                </span>
                <span class="job-name">{job.name}</span>
                <div class="job-right">
                  {#if job.allow_failure && job.status === 'failed'}
                    <span class="job-allow-badge" use:tooltip={'Allowed to fail'}>!</span>
                  {/if}
                  {#if job.duration_secs}
                    <span class="job-duration">{formatDuration(job.duration_secs)}</span>
                  {/if}
                  <ExternalLink size={10} class="job-link-icon" />
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</Modal>

<style>
  .provider-icon {
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }
  /* GitLab brand orange — matches the provider badge in PipelinesPanel
     so the icon reads as the same surface across list & detail. */
  .provider-icon-gitlab { color: var(--brand-gitlab); }

  .modal-title-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  .modal-run-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .modal-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .meta-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .branch-chip { color: var(--accent); background: var(--accent-subtle); }
  .sha-chip    { font-family: var(--font-code); font-size: 10px; }
  .dur-chip    { color: var(--text-muted); }
  .time-chip   { color: var(--text-muted); }

  /* ── Status badge ─────────────────────────────────────────────────────── */
  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 20px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .status-success  { background: color-mix(in srgb, var(--success) 18%, transparent); color: var(--success); border: 1px solid color-mix(in srgb, var(--success) 35%, transparent); }
  .status-failed   { background: color-mix(in srgb, var(--error) 18%, transparent);   color: var(--error);   border: 1px solid color-mix(in srgb, var(--error) 35%, transparent); }
  .status-running  { background: var(--accent-subtle); color: var(--accent); border: 1px solid var(--accent); }
  .status-cancelled{ background: rgba(120,120,120,0.1); color: var(--text-muted); border: 1px solid var(--border); }
  .status-pending  { background: rgba(120,120,120,0.08); color: var(--text-disabled); border: 1px solid var(--border-subtle); }

  .hdr-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: var(--radius-md);
    font-size: 12px;
    font-family: var(--font-ui-sans);
    font-weight: 500;
    cursor: pointer;
    text-decoration: none;
    border: 1px solid var(--border);
    background: var(--bg-base);
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Empty / loading states ───────────────────────────────────────────── */
  .body-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 40px 20px;
    color: var(--text-muted);
  }
  .state-hint  { font-size: 13px; }
  :global(.state-icon)      { color: var(--text-muted); }
  :global(.state-icon.state-warn) { color: var(--error); }

  /* ── Stage flow ───────────────────────────────────────────────────────── */
  .stage-flow {
    display: flex;
    align-items: flex-start;
    gap: 0;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  .stage-arrow {
    display: flex;
    align-items: center;
    padding: 0 2px;
    margin-top: 36px;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  /* ── Stage column ─────────────────────────────────────────────────────── */
  .stage-col {
    min-width: 160px;
    max-width: 220px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    flex-shrink: 0;
    background: var(--bg-base);
  }

  .status-border-success  { border-color: rgba(74,222,128,0.35); }
  .status-border-failed   { border-color: rgba(248,113,113,0.35); }
  .status-border-running  { border-color: var(--accent); }
  .status-border-cancelled{ border-color: var(--border); }

  .stage-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 10px;
    background: var(--bg-overlay);
    border-bottom: 1px solid var(--border-subtle);
  }

  .stage-name {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--text-secondary);
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .stage-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .dot-success   { background: var(--success); }
  .dot-failed    { background: var(--error); }
  .dot-running   { background: var(--accent); }
  .dot-cancelled { background: var(--text-muted); }
  .dot-pending   { background: var(--border); }

  .job-list {
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* ── Job card ─────────────────────────────────────────────────────────── */
  .job-card {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 8px;
    border-radius: 5px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    text-decoration: none;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    min-width: 0;
    width: 100%;
    text-align: left;
    font: inherit;
    color: inherit;
  }
  .job-card:hover {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  .job-card:hover :global(.job-link-icon) { opacity: 1; }

  .job-allow-fail { opacity: 0.75; }

  .job-status-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  /* Status colours applied to the icon span inside the card */
  .job-card .status-success   { color: var(--success); }
  .job-card .status-failed    { color: var(--error); }
  .job-card .status-running   { color: var(--accent); }
  .job-card .status-cancelled { color: var(--text-muted); }
  .job-card .status-pending   { color: var(--text-disabled); }

  .job-name {
    font-size: 11px;
    color: var(--text-primary);
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .job-right {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .job-duration {
    font-size: 10px;
    font-family: var(--font-code);
    color: var(--text-muted);
    white-space: nowrap;
  }

  .job-allow-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: rgba(248,113,113,0.2);
    color: var(--error);
    font-size: 9px;
    font-weight: 700;
  }

  :global(.job-link-icon) {
    color: var(--text-muted);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }

  /* `.spin` keyframes/class come from app.css — no local override here, the
     local one shadowed (and broke) the global rule via Svelte's keyframe
     scoping. */
</style>
