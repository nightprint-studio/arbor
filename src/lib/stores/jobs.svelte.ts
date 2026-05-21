import type { JobInfo } from '$lib/types/jobs';
import { listJobs, getJobOutput, cancelJob, dismissJob, clearFinishedJobs } from '$lib/ipc/job';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { coalesceBatch } from '$lib/utils/coalesce';

// Per-job accumulated output lines (ring buffer, max 2000 lines).
const MAX_LINES = 2000;

const SHOW_HIDDEN_LS_KEY = 'arbor:jobs-show-hidden';

function createJobsStore() {
  let jobs       = $state<JobInfo[]>([]);
  let outputs    = $state<Record<string, string[]>>({});
  /** Job whose output is currently shown in the jobs panel. */
  let activeJobId = $state<string | null>(null);
  /** When true, hidden jobs are also rendered in the Jobs overlay / output
   *  panel and counted in the status-bar running badge. Persisted to
   *  localStorage under SHOW_HIDDEN_LS_KEY. */
  let showHidden  = $state<boolean>(
    typeof localStorage !== 'undefined' && localStorage.getItem(SHOW_HIDDEN_LS_KEY) === 'true'
  );

  // ── Helpers ────────────────────────────────────────────────────────────────

  function upsertJob(job: JobInfo) {
    const idx = jobs.findIndex(j => j.id === job.id);
    if (idx >= 0) {
      jobs[idx] = job;
    } else {
      jobs = [job, ...jobs];
    }
  }

  /** Append a chunk of lines to a job's output buffer in a single reactive
   *  update.  Skips jobs that aren't currently active in the output panel —
   *  the backend still records every line, so opening the panel later
   *  re-fetches the full ring buffer via `loadOutput`.  Avoiding the store
   *  write for inactive jobs is what prevents the post-Alt-Tab freeze when
   *  hundreds of buffered events drain into the WebView at once. */
  function appendLines(jobId: string, lines: string[]) {
    if (!lines.length) return;
    if (activeJobId !== jobId) return;
    const existing = outputs[jobId] ?? [];
    const next = existing.length === 0 ? lines.slice() : existing.concat(lines);
    if (next.length > MAX_LINES) next.splice(0, next.length - MAX_LINES);
    outputs = { ...outputs, [jobId]: next };
  }

  // Coalesce a burst of `arbor://job-output-batch` events into one reactive
  // update per animation frame. Each batch is already a `string[]`; we group
  // batches that arrive in the same frame and apply them per job_id in one
  // store write, regardless of how many events drained from the IPC channel.
  const flushOutputBatches = coalesceBatch<{ job_id: string; lines: string[] }>((batches) => {
    if (!activeJobId) return;
    const collected: string[] = [];
    for (const b of batches) {
      if (b.job_id === activeJobId && b.lines.length) {
        for (const l of b.lines) collected.push(l);
      }
    }
    if (collected.length) appendLines(activeJobId, collected);
  });

  // ── Initial load ──────────────────────────────────────────────────────────

  async function load() {
    try {
      jobs = await listJobs();
    } catch { /* ignore — backend may not be ready */ }
  }

  // ── Tauri event listeners (called from AppShell) ───────────────────────────

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://job-started',
        handler: (e: { payload: { job_id: string; name: string; plugin_name: string; command: string; category?: string; hidden?: boolean } }) => {
          const p = e.payload;
          upsertJob({
            id:          p.job_id,
            name:        p.name,
            plugin_name: p.plugin_name,
            command:     p.command,
            started_at:  Math.floor(Date.now() / 1000),
            status:      { type: 'running' },
            category:    p.category,
            hidden:      p.hidden,
          });
          if (!outputs[p.job_id]) outputs = { ...outputs, [p.job_id]: [] };
        },
      },
      {
        // Batch event emitted by `jobs::LineBatcher`: coalesces stdout/stderr
        // lines into 50ms / 100-line chunks so an unfocused window drains
        // the IPC queue in tens of events instead of thousands.
        event: 'arbor://job-output-batch',
        handler: (e: { payload: { job_id: string; lines: string[] } }) => {
          const p = e.payload;
          if (!p?.job_id || !p.lines?.length) return;
          flushOutputBatches(p);
        },
      },
      {
        event: 'arbor://job-done',
        handler: (e: { payload: { job_id: string; success: boolean; exit_code: number; error?: string; cancelled?: boolean } }) => {
          const p = e.payload;
          const idx = jobs.findIndex(j => j.id === p.job_id);
          if (idx < 0) return;
          const updated = { ...jobs[idx] };
          if (p.cancelled) {
            updated.status = { type: 'cancelled' };
          } else if (p.success) {
            updated.status = { type: 'completed', exit_code: p.exit_code };
          } else {
            updated.status = { type: 'failed', error: p.error ?? `exit ${p.exit_code}` };
          }
          jobs[idx] = updated;

          // System jobs are auto-dismissed on successful completion.
          if (
            updated.category?.toLowerCase() === 'system' &&
            updated.status.type === 'completed' &&
            (updated.status as { type: string; exit_code?: number }).exit_code === 0
          ) {
            jobs = jobs.filter(j => j.id !== p.job_id);
            if (activeJobId === p.job_id) activeJobId = null;
          }
        },
      },
    ]);
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  async function loadOutput(jobId: string) {
    // Always re-fetch on activation: since we no longer mirror non-active
    // jobs' streaming output into the store, the backend's ring buffer is
    // the source of truth when the panel opens (or switches between jobs).
    try {
      const lines = await getJobOutput(jobId);
      outputs = { ...outputs, [jobId]: lines };
    } catch { /* ignore */ }
  }

  async function cancel(jobId: string) {
    try { await cancelJob(jobId); } catch { /* ignore */ }
    const idx = jobs.findIndex(j => j.id === jobId);
    if (idx >= 0) jobs[idx] = { ...jobs[idx], status: { type: 'cancelled' } };
  }

  function setActiveJob(id: string | null) {
    // Drop the previous job's cached lines — only the currently-viewed job
    // keeps live streaming, others are re-fetched from the backend ring
    // buffer on next activation.  Keeps memory bounded when the user cycles
    // through many jobs.
    if (id !== activeJobId && activeJobId && outputs[activeJobId]) {
      const next = { ...outputs };
      delete next[activeJobId];
      outputs = next;
    }
    activeJobId = id;
  }

  function setShowHidden(value: boolean) {
    showHidden = value;
    try { localStorage.setItem(SHOW_HIDDEN_LS_KEY, value ? 'true' : 'false'); } catch { /* ignore */ }
  }

  async function dismiss(jobId: string) {
    // Sync the backend registry first so the next list_jobs() doesn't
    // resurrect the entry; ignore the boolean — frontend state is authoritative
    // for the UI tick.
    try { await dismissJob(jobId); } catch { /* ignore */ }
    jobs = jobs.filter(j => j.id !== jobId);
    if (activeJobId === jobId) activeJobId = null;
    if (outputs[jobId]) {
      const next = { ...outputs };
      delete next[jobId];
      outputs = next;
    }
  }

  async function clearFinished() {
    try { await clearFinishedJobs(); } catch { /* ignore */ }
    const finishedIds = new Set(
      jobs.filter(j => j.status.type !== 'running').map(j => j.id)
    );
    jobs = jobs.filter(j => !finishedIds.has(j.id));
    if (activeJobId && finishedIds.has(activeJobId)) activeJobId = null;
    // Forget cached output for the removed jobs.
    const next: Record<string, string[]> = {};
    for (const id of Object.keys(outputs)) {
      if (!finishedIds.has(id)) next[id] = outputs[id];
    }
    outputs = next;
  }

  const runningCount        = $derived(
    jobs.filter(j => j.status.type === 'running' && (showHidden || !j.hidden)).length
  );
  const runningHiddenCount  = $derived(jobs.filter(j => j.status.type === 'running' &&  j.hidden).length);
  const finishedCount       = $derived(
    jobs.filter(j => j.status.type !== 'running' && (showHidden || !j.hidden)).length
  );

  return {
    get jobs()                { return jobs; },
    get outputs()             { return outputs; },
    get activeJobId()         { return activeJobId; },
    get runningCount()        { return runningCount; },
    get runningHiddenCount()  { return runningHiddenCount; },
    get finishedCount()       { return finishedCount; },
    get showHidden()          { return showHidden; },
    setShowHidden,
    load,
    setupListeners,
    loadOutput,
    cancel,
    setActiveJob,
    dismiss,
    clearFinished,
  };
}

export const jobsStore = createJobsStore();
