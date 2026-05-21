import type { PipelineDef, PipelineRun, RunStatus, CiProviderInfo, CiRun } from '$lib/types/pipeline';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { coalesceLatestByKey } from '$lib/utils/coalesce';
import { withLoading } from '$lib/utils/async-state';
import { cacheStore } from './cache.svelte';
import { listPipelineDefs, listPipelineRuns } from '$lib/ipc/pipeline';
import { uiStore } from './ui.svelte';
import { notificationsStore } from './notifications.svelte';

// ──────────────────────────────────────────────────────────────────────────
// Pipeline step-output coalescing
//
// The Rust-side reader (`StepLogSink::emit_batch`) now drains the child's
// stdout/stderr pipe in 4 KB chunks and emits ONE Tauri event per chunk
// with the lines split locally on the reader thread (mirrors the integrated
// terminal's read loop).  A `mvn clean package` therefore arrives as 5–20
// events instead of thousands — already a huge win on IPC + reactivity.
//
// We still coalesce on the frontend as a second-stage defence: if two
// chunks arrive in quick succession, or a single chunk carries hundreds of
// lines, we want at most one Svelte reactivity tick + reflow per
// COALESCE_MS window per (run, stage, step) pair, regardless of the burst
// shape.  Without this any 500-line burst would still trigger a single
// `step.output.push(...lines)` that re-renders the whole DOM list at once,
// freezing the UI for the duration of the layout pass.
// ──────────────────────────────────────────────────────────────────────────
const COALESCE_MS = 50;
type StepKey = string; // `${run_id}\0${stage_id}\0${step_id}`
const pendingOutput = new Map<StepKey, string[]>();
let flushScheduled = false;

function makeKey(run_id: string, stage_id: string, step_id: string): StepKey {
  return `${run_id}\0${stage_id}\0${step_id}`;
}

function createPipelinesStore() {
  let defs    = $state<PipelineDef[]>([]);
  let runs    = $state<PipelineRun[]>([]);
  /** The run currently shown in the detail/graph view. */
  let activeRunId = $state<string | null>(null);

  // Last status observed per run_id, used to detect pending→running and
  // running→terminal transitions on `arbor://pipeline-update` so we can
  // raise the host's automatic start-toast / done-notification exactly
  // once. Seeded on load/reload so previously-finished runs restored
  // from disk don't fire bogus notifications. Cleared on discard.
  const lastStatus = new Map<string, RunStatus>();
  const TERMINAL: ReadonlySet<RunStatus> = new Set(['success', 'failed', 'cancelled']);

  function seedStatuses(rs: PipelineRun[]) {
    lastStatus.clear();
    for (const r of rs) lastStatus.set(r.id, r.status);
  }

  // Buffer a batch of lines for later flush. Schedules a single flush per
  // coalesce window — repeated batches within the window are appended to
  // the existing bucket. Empty / missing batches are ignored.
  function enqueueOutputLines(run_id: string, stage_id: string, step_id: string, lines: string[]) {
    if (!lines || lines.length === 0) return;
    const key = makeKey(run_id, stage_id, step_id);
    const arr = pendingOutput.get(key);
    if (arr) {
      for (const l of lines) arr.push(l);
    } else {
      pendingOutput.set(key, lines.slice());
    }
    if (flushScheduled) return;
    flushScheduled = true;
    setTimeout(flushPendingOutput, COALESCE_MS);
  }

  // Drain all buckets into their corresponding `step.output` arrays in one
  // pass. ONE `push(...lines)` per bucket ⇒ ONE reactive notification per
  // step ⇒ at most one reflow per modified step per coalesce window.
  function flushPendingOutput() {
    flushScheduled = false;
    if (pendingOutput.size === 0) return;
    // Snapshot + clear before mutating so reentrant emits during reactivity
    // don't disturb the iteration.
    const batch = Array.from(pendingOutput.entries());
    pendingOutput.clear();
    for (const [key, lines] of batch) {
      const sep1 = key.indexOf('\0');
      const sep2 = key.indexOf('\0', sep1 + 1);
      if (sep1 < 0 || sep2 < 0) continue;
      const rid  = key.slice(0, sep1);
      const sid  = key.slice(sep1 + 1, sep2);
      const stid = key.slice(sep2 + 1);
      const run = runs.find(r => r.id === rid);
      if (!run) continue;
      const stage = run.stages.find(s => s.def_id === sid);
      if (!stage) continue;
      const step = stage.steps.find(st => st.def_id === stid);
      if (!step) continue;
      step.output.push(...lines);
    }
  }

  // ── CI/CD state ────────────────────────────────────────────────────────────
  let ciProvider  = $state<CiProviderInfo | null>(null);
  let ciRuns      = $state<CiRun[]>([]);
  let ciLoading   = $state(false);
  let ciError     = $state<string | null>(null);

  function activeRun(): PipelineRun | null {
    return runs.find(r => r.id === activeRunId) ?? null;
  }

  async function load(tabId?: string) {
    try {
      const data = await cacheStore.loadPipelineData(tabId);
      defs = data.defs;
      runs = data.runs;
      seedStatuses(runs);
    } catch { /* ignore */ }
  }

  /**
   * Force-refresh defs + runs from the backend, bypassing the cache. Called
   * automatically whenever `arbor.pipeline.define` registers a new def (plugin
   * reloads, profile creation, repo switch…) so the panel picks up the change
   * even if it has already rendered an empty list.
   */
  async function reload() {
    try {
      const [newDefs, newRuns] = await Promise.all([listPipelineDefs(), listPipelineRuns()]);
      defs = newDefs;
      runs = newRuns;
      seedStatuses(runs);
    } catch { /* ignore */ }
  }

  /**
   * Fire the host's automatic notifications on observed status transitions.
   * Skips entirely when the run was started with `silent = true` so plugins
   * that surface their own start/done messages don't get duplicated.
   *
   *   pending  → running          → transient toast    "Pipeline X started"
   *   running  → success/failed/cancelled → bell notify with deep-link
   */
  function maybeNotifyTransition(updated: PipelineRun) {
    if (updated.silent) {
      lastStatus.set(updated.id, updated.status);
      return;
    }
    const prev = lastStatus.get(updated.id);
    lastStatus.set(updated.id, updated.status);
    if (prev === updated.status) return;

    const label = updated.name || updated.pipeline_id;
    const openAction = { label: 'Open', onClick: () => setActiveRun(updated.id) };

    // Start: fire on the first sighting in `running`, OR on a clean
    // pending→running transition. Resumes (failed→running) also count
    // as "started" — the user usually wants a heads-up.
    if (updated.status === 'running' &&
        (prev === undefined || prev === 'pending' || TERMINAL.has(prev))) {
      uiStore.showToast(`Pipeline "${label}" started`, 'info', 4000, openAction);
      return;
    }

    // Done: only when transitioning OUT of running/paused, so we don't
    // fire on bare disk-recovered runs whose first sighting is already
    // terminal (those statuses were seeded by load()/reload()).
    if (TERMINAL.has(updated.status) && (prev === 'running' || prev === 'paused')) {
      const action = { kind: 'open-pipeline-run' as const, label: 'Open', run_id: updated.id };
      switch (updated.status) {
        case 'success':
          notificationsStore.add(
            'Pipeline succeeded', `"${label}" finished ✓`,
            'success', updated.plugin, action,
          );
          break;
        case 'failed':
          notificationsStore.add(
            'Pipeline failed', `"${label}" — open the run to inspect logs`,
            'error', updated.plugin, action,
          );
          break;
        case 'cancelled':
          notificationsStore.add(
            'Pipeline cancelled', `"${label}" was stopped`,
            'warning', updated.plugin, action,
          );
          break;
      }
    }
  }

  // Coalesce the `runs` array write per run_id: a fast-firing pipeline
  // (many step transitions per second, or a backlog drained after focus
  // returns) collapses to one reactive assignment per frame.  We still
  // observe every event for `maybeNotifyTransition` — the transient
  // toasts / bell notifications depend on seeing each transition.
  const applyRunUpdate = coalesceLatestByKey<PipelineRun>(
    (updated) => {
      const idx = runs.findIndex(r => r.id === updated.id);
      if (idx !== -1) {
        runs[idx] = updated;
      } else {
        runs = [updated, ...runs];
      }
    },
    (r) => r.id,
  );

  /** Listen for real-time run updates emitted by the orchestrator. */
  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://pipeline-update',
        handler: (e: { payload: PipelineRun }) => {
          const updated = e.payload;
          maybeNotifyTransition(updated);
          applyRunUpdate(updated);
        },
      },
      {
        event: 'arbor://pipeline-discarded',
        handler: (e: { payload: { run_id: string } }) => {
          const id = e.payload?.run_id;
          if (!id) return;
          runs = runs.filter(r => r.id !== id);
          lastStatus.delete(id);
          if (activeRunId === id) activeRunId = null;
        },
      },
      {
        // Live batch append from the pipeline orchestrator. Emitted by
        // `StepLogSink::emit_batch` once per drained 4 KB chunk on the
        // Rust-side stdout/stderr reader thread. Payload is `lines:
        // string[]` already pre-split on `\n` (with trailing `\r`
        // stripped) so the frontend only has to push them onto
        // `step.output`. We then coalesce across consecutive chunks via
        // `enqueueOutputLines` so even a burst of multiple chunks
        // produces at most one reactivity tick per COALESCE_MS.
        event: 'arbor://pipeline-step-output',
        handler: (e: { payload: { run_id: string; stage_id: string; step_id: string; lines: string[] } }) => {
          const { run_id, stage_id, step_id, lines } = e.payload ?? ({} as any);
          if (!run_id || !stage_id || !step_id) return;
          enqueueOutputLines(run_id, stage_id, step_id, lines);
        },
      },
      {
        // Fired by `arbor.pipeline.define` (plugin registers a new pipeline or
        // replaces an existing one). The initial `load()` ran before the
        // plugin was up, so force a fresh fetch here.
        event: 'arbor://pipeline-def-registered',
        handler: () => { reload(); },
      },
    ]);
  }

  function setActiveRun(id: string | null) {
    activeRunId = id;
  }

  /** Returns runs for a specific pipeline_id, most recent first. */
  function runsFor(pipelineId: string): PipelineRun[] {
    return runs.filter(r => r.pipeline_id === pipelineId);
  }

  /** Detect CI provider for the given tab and load runs if a token is available.
   *  Clears stale provider/runs from the previous tab and flips `ciLoading`
   *  immediately so the panel shows a spinner during the round-trip rather
   *  than the previous tab's data — clicking a stale run while the new tab's
   *  CI fetch is still in flight would fire `fetch_ci_jobs(newTab, oldRunId)`
   *  and error out. */
  async function loadCi(tabId: string | null) {
    ciProvider = null;
    ciRuns = [];
    ciError = null;
    if (!tabId) { ciLoading = false; return; }
    ciLoading = true;
    try {
      try {
        ciProvider = await cacheStore.loadCiProvider(tabId);
      } catch {
        ciProvider = null;
      }
      if (ciProvider?.has_token) {
        await refreshCiRuns(tabId);
      }
    } finally {
      ciLoading = false;
    }
  }

  /** Re-fetch CI runs for the current tab, bypassing the cache. The CI cache
   *  is populated by `loadCi` on first entry to the panel; explicit refresh
   *  (refresh button, modal polling) must hit the live API or the run status
   *  stays frozen at the snapshot taken when the panel first opened. */
  async function refreshCiRuns(tabId: string) {
    const result = await withLoading(
      v => { ciLoading = v; },
      v => { ciError = v; },
      () => cacheStore.loadCiRuns(tabId, true),
    );
    ciRuns = result ?? [];
  }

  return {
    get defs()        { return defs; },
    get runs()        { return runs; },
    get activeRunId() { return activeRunId; },
    get ciProvider()  { return ciProvider; },
    get ciRuns()      { return ciRuns; },
    get ciLoading()   { return ciLoading; },
    get ciError()     { return ciError; },
    activeRun,
    load,
    reload,
    setupListeners,
    setActiveRun,
    runsFor,
    loadCi,
    refreshCiRuns,
  };
}

export const pipelinesStore = createPipelinesStore();
