import type { StepStatus } from '$lib/components/shared/ui/ProgressStepper.svelte';

// ---------------------------------------------------------------------------
// Operation = one user-visible long-running thing (Pull, Fetch-all, link
// sync, …).  Each one renders as a card inside `<OperationsOverlay>`.
// ---------------------------------------------------------------------------

export interface OperationStep {
  key:    string;
  label:  string;
  detail?: string | null;
  /** Per-step override of the ProgressStepper position-derived status. */
  status?: StepStatus;
}

export interface Operation {
  id:        string;
  /** Card title (e.g. "Pull develop", "Fetch workspace (12 repos)"). */
  title:     string;
  /** Short subtitle rendered under the title (e.g. workspace name). */
  subtitle?: string | null;
  steps:     OperationStep[];
  /** Key of the currently running step, or null when none / done. */
  current:   string | null;
  /** Detail text for the active step (overrides `step.detail`). */
  activeDetail?: string | null;
  /** True once all phases are finished — the card becomes a static summary
   *  for a few seconds then auto-dismisses.  Set via `finish()`. */
  done:      boolean;
  /** Top-level error message — shown only when `done === true` AND no
   *  per-step error already covers it. */
  error:     string | null;
  /** Final summary line shown under the stepper when `done === true`. */
  summary?:  string | null;
  startedAt: number;
  finishedAt?: number;
  /** Window-routing target. `null`/absent → main window; a value routes the
   *  card to the matching feedback host. Built-in flows (pull/fetch/sync) are
   *  pre-created in the window that triggered them, so they leave this unset. */
  target?:   string | null;
}

// Auto-dismiss windows.  Errors stick around longer so the user has time
// to read them before they vanish.
const AUTO_DISMISS_OK_MS  = 6_000;
const AUTO_DISMISS_ERR_MS = 14_000;

function createOperationsStore() {
  let operations = $state<Operation[]>([]);
  /** Per-op auto-dismiss timers, so a retry (`start` with the same id), a second
   *  `finish`, or a manual `dismiss` can CANCEL the pending timer instead of
   *  letting a stale one fire later and yank the wrong (re-created) card. This
   *  replaces the old wall-clock guard, which was timing-fragile. */
  const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function cancelTimer(id: string): void {
    const t = dismissTimers.get(id);
    if (t !== undefined) {
      clearTimeout(t);
      dismissTimers.delete(id);
    }
  }

  function start(op: {
    id:        string;
    title:     string;
    subtitle?: string | null;
    steps:     OperationStep[];
    current?:  string | null;
    target?:   string | null;
  }): void {
    // A restart of the same id cancels any auto-dismiss still pending from a
    // previous run, so the fresh card can't be removed by a ghost timer.
    cancelTimer(op.id);
    // Replace any previous op with the same id (e.g. retry of the same
    // pull) — keeps the overlay deterministic instead of stacking ghosts.
    operations = [
      ...operations.filter(o => o.id !== op.id),
      {
        id:        op.id,
        title:     op.title,
        subtitle:  op.subtitle ?? null,
        steps:     op.steps ?? [],
        current:   op.current ?? op.steps?.[0]?.key ?? null,
        done:      false,
        error:     null,
        startedAt: Date.now(),
        target:    op.target ?? null,
      },
    ];
  }

  function update(id: string, partial: Partial<Operation>): void {
    const idx = operations.findIndex(o => o.id === id);
    if (idx < 0) return;
    // Reassign the whole array so the keyed `{#each}` sees the change reliably
    // (index mutation on the $state proxy is reactive, but a fresh array is the
    // least surprising and matches start/dismiss).
    const next = [...operations];
    next[idx] = { ...next[idx], ...partial };
    operations = next;
  }

  /** Update a single step (matched by key) inside an operation.  Useful when
   *  per-step `status` / `detail` arrives as discrete events. A no-op when the
   *  op or the step is unknown (an out-of-order event is dropped, never throws). */
  function updateStep(id: string, stepKey: string, partial: Partial<OperationStep>): void {
    const idx = operations.findIndex(o => o.id === id);
    if (idx < 0) return;
    const op = operations[idx];
    const sIdx = op.steps.findIndex(s => s.key === stepKey);
    if (sIdx < 0) return;
    const newSteps = [...op.steps];
    newSteps[sIdx] = { ...newSteps[sIdx], ...partial };
    const next = [...operations];
    next[idx] = { ...op, steps: newSteps };
    operations = next;
  }

  function finish(
    id:    string,
    opts:  { summary?: string | null; error?: string | null } = {},
  ): void {
    const idx = operations.findIndex(o => o.id === id);
    if (idx < 0) return;
    const next = [...operations];
    next[idx] = {
      ...next[idx],
      done:       true,
      current:    null,
      summary:    opts.summary ?? null,
      error:      opts.error ?? null,
      finishedAt: Date.now(),
    };
    operations = next;
    // Schedule (or reschedule) auto-dismiss — cancelling any prior timer for
    // this id first, so a second finish() doesn't leave two timers racing.
    cancelTimer(id);
    const delay = opts.error ? AUTO_DISMISS_ERR_MS : AUTO_DISMISS_OK_MS;
    dismissTimers.set(id, setTimeout(() => {
      dismissTimers.delete(id);
      dismiss(id);
    }, delay));
  }

  function dismiss(id: string): void {
    cancelTimer(id);
    operations = operations.filter(o => o.id !== id);
  }

  function clearFinished(): void {
    for (const o of operations) {
      if (o.done) cancelTimer(o.id);
    }
    operations = operations.filter(o => !o.done);
  }

  return {
    get operations() { return operations; },
    start,
    update,
    updateStep,
    finish,
    dismiss,
    clearFinished,
  };
}

export const operationsStore = createOperationsStore();
