import { corvus } from '../rpc';
import type { Plan, PlanRepo, StepSpec } from '../../types/corvus/plan';

export const listPlans = () => corvus<Plan[]>('plan_list');

export const getPlan = (planId: string) => corvus<Plan>('plan_get', { plan_id: planId });

export const createPlan = (name?: string) => corvus<Plan>('plan_create', { name: name ?? null });

/** Replace the plan's name, phases, gates and `finally`. Refused while the plan is running. */
export const savePlan = (plan: Plan) => corvus<Plan>('plan_save', { plan });

export const deletePlan = (planId: string) => corvus<void>('plan_delete', { plan_id: planId });

/** A copy with fresh statuses — to run a template, or to keep a plan as one. */
export const copyPlan = (planId: string, opts: { name?: string; template?: boolean } = {}) =>
  corvus<Plan>('plan_copy', { plan_id: planId, name: opts.name ?? null, template: opts.template ?? null });

export interface AddStepOptions {
  /** Defaults to the plan being composed (the most recent open one, or a new one). */
  planId?: string;
  /** Defaults to the last phase. */
  phaseId?: string;
  /** Put the step in a phase of its own, after the last. */
  newPhaseAfter?: boolean;
  /** Put the step in the `finally` steps, which always run. */
  toFinally?: boolean;
}

export const addPlanStep = (repo: PlanRepo, spec: StepSpec, opts: AddStepOptions = {}) =>
  corvus<Plan>('plan_add_step', {
    plan_id: opts.planId ?? null,
    phase_id: opts.phaseId ?? null,
    new_phase_after: opts.newPhaseAfter ?? null,
    to_finally: opts.toFinally ?? null,
    repo,
    spec,
  });

export const runPlan = (planId: string) => corvus<Plan>('plan_run', { plan_id: planId });
export const stopPlan = (planId: string) => corvus<Plan>('plan_stop', { plan_id: planId });

export const retryPlanStep = (planId: string, stepId: string) =>
  corvus<Plan>('plan_retry_step', { plan_id: planId, step_id: stepId });
export const skipPlanStep = (planId: string, stepId: string) =>
  corvus<Plan>('plan_skip_step', { plan_id: planId, step_id: stepId });

/** Open the gate before a phase: the go-ahead for a manual gate, "skip the wait" for the others. */
export const releasePlanGate = (planId: string, phaseId: string) =>
  corvus<Plan>('plan_release_gate', { plan_id: planId, phase_id: phaseId });
