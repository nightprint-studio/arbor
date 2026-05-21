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
}

// Auto-dismiss windows.  Errors stick around longer so the user has time
// to read them before they vanish.
const AUTO_DISMISS_OK_MS  = 6_000;
const AUTO_DISMISS_ERR_MS = 14_000;

function createOperationsStore() {
  let operations = $state<Operation[]>([]);
  /** Set of op ids the user has manually dismissed — auto-dismiss timers
   *  check this so an in-flight timer doesn't try to remove an already-gone
   *  entry (no-op anyway, but avoids reactive churn). */
  const dismissedIds = new Set<string>();

  function start(op: {
    id:        string;
    title:     string;
    subtitle?: string | null;
    steps:     OperationStep[];
    current?:  string | null;
  }): void {
    // Replace any previous op with the same id (e.g. retry of the same
    // pull) — keeps the overlay deterministic instead of stacking ghosts.
    operations = [
      ...operations.filter(o => o.id !== op.id),
      {
        id:        op.id,
        title:     op.title,
        subtitle:  op.subtitle ?? null,
        steps:     op.steps,
        current:   op.current ?? op.steps[0]?.key ?? null,
        done:      false,
        error:     null,
        startedAt: Date.now(),
      },
    ];
    dismissedIds.delete(op.id);
  }

  function update(id: string, partial: Partial<Operation>): void {
    const idx = operations.findIndex(o => o.id === id);
    if (idx < 0) return;
    operations[idx] = { ...operations[idx], ...partial };
  }

  /** Update a single step (matched by key) inside an operation.  Useful when
   *  per-step `status` / `detail` arrives as discrete events. */
  function updateStep(id: string, stepKey: string, partial: Partial<OperationStep>): void {
    const idx = operations.findIndex(o => o.id === id);
    if (idx < 0) return;
    const op = operations[idx];
    const sIdx = op.steps.findIndex(s => s.key === stepKey);
    if (sIdx < 0) return;
    const newSteps = [...op.steps];
    newSteps[sIdx] = { ...newSteps[sIdx], ...partial };
    operations[idx] = { ...op, steps: newSteps };
  }

  function finish(
    id:    string,
    opts:  { summary?: string | null; error?: string | null } = {},
  ): void {
    const idx = operations.findIndex(o => o.id === id);
    if (idx < 0) return;
    operations[idx] = {
      ...operations[idx],
      done:       true,
      current:    null,
      summary:    opts.summary ?? null,
      error:      opts.error ?? null,
      finishedAt: Date.now(),
    };
    // Schedule auto-dismiss.  A subsequent finish() on the same id resets
    // both done state and the timer below — but cancelling the previous
    // timer would require tracking it; a stale timer is harmless because
    // dismiss() is a no-op once the entry has been replaced.
    const delay = opts.error ? AUTO_DISMISS_ERR_MS : AUTO_DISMISS_OK_MS;
    setTimeout(() => {
      const cur = operations.find(o => o.id === id);
      if (cur && cur.done && cur.finishedAt && Date.now() - cur.finishedAt >= delay - 50) {
        dismiss(id);
      }
    }, delay);
  }

  function dismiss(id: string): void {
    dismissedIds.add(id);
    operations = operations.filter(o => o.id !== id);
  }

  function clearFinished(): void {
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
