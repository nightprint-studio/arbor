/**
 * Pure edits on a plan draft. The modal clones the plan, applies one of these and saves it; the
 * backend validates the result, so nothing here guards against a reference that stops making sense
 * (an MR moved after the step that merges it) — the refusal arrives as a toast.
 */
import type { GateSpec, Plan, PlanStep } from '$lib/types/corvus/plan';

/** The id of the `finally` column — it has no phase behind it. */
export const FINALLY = 'finally';

export interface Column {
  /** The phase id, or {@link FINALLY}. */
  id: string;
  steps: PlanStep[];
}

/** Phases in order, then `finally` — the left-to-right layout of the board. */
export function columns(plan: Plan): Column[] {
  return [...plan.phases.map(p => ({ id: p.id, steps: p.steps })), { id: FINALLY, steps: plan.finally }];
}

export function locate(plan: Plan, stepId: string): { column: number; index: number } | null {
  const cols = columns(plan);
  for (let column = 0; column < cols.length; column++) {
    const index = cols[column].steps.findIndex(s => s.id === stepId);
    if (index >= 0) return { column, index };
  }
  return null;
}

export function removeStep(plan: Plan, stepId: string): boolean {
  const at = locate(plan, stepId);
  if (!at) return false;
  columns(plan)[at.column].steps.splice(at.index, 1);
  return true;
}

/** Move a step up or down inside its column. */
export function moveWithin(plan: Plan, stepId: string, delta: -1 | 1): boolean {
  const at = locate(plan, stepId);
  if (!at) return false;
  const steps = columns(plan)[at.column].steps;
  const to = at.index + delta;
  if (to < 0 || to >= steps.length) return false;
  const [step] = steps.splice(at.index, 1);
  steps.splice(to, 0, step);
  return true;
}

/** Move a step to the neighbouring column, keeping its row where there is one. */
export function moveAcross(plan: Plan, stepId: string, delta: -1 | 1): boolean {
  const at = locate(plan, stepId);
  if (!at) return false;
  const cols = columns(plan);
  const target = cols[at.column + delta];
  if (!target) return false;
  const [step] = cols[at.column].steps.splice(at.index, 1);
  target.steps.splice(Math.min(at.index, target.steps.length), 0, step);
  return true;
}

export function setGate(plan: Plan, phaseId: string, spec: GateSpec): boolean {
  const phase = plan.phases.find(p => p.id === phaseId);
  if (!phase) return false;
  phase.gate = { ...spec, state: 'idle', since_ms: null };
  return true;
}

/**
 * The step to select after `current` is removed or the arrow `dx`/`dy` is pressed: arrows keep the
 * row across columns (clamped), skipping empty columns.
 */
export function neighbour(plan: Plan, current: string | null, dx: number, dy: number): string | null {
  const cols = columns(plan);
  if (!current) return cols.flatMap(c => c.steps)[0]?.id ?? null;
  const at = locate(plan, current);
  if (!at) return null;
  if (dy) return cols[at.column].steps[at.index + dy]?.id ?? current;
  let c = at.column + dx;
  while (cols[c] && cols[c].steps.length === 0) c += dx;
  const col = cols[c];
  return col ? col.steps[Math.min(at.index, col.steps.length - 1)].id : current;
}
