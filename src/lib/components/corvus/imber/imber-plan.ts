/** Pure helpers for the plan card in the Imber thread. */

import type { Plan, PlanStep } from '$lib/types/corvus/plan';

/** The plan as it is now: the store's copy when it has one (it is updated as steps finish), else the snapshot in the card. */
export function livePlan(snapshot: Plan, plans: Plan[]): Plan {
  return plans.find((p) => p.id === snapshot.id) ?? snapshot;
}

export function allSteps(plan: Plan): PlanStep[] {
  return [...plan.phases.flatMap((p) => p.steps), ...plan.finally];
}

export interface Progress {
  total: number;
  /** Settled: done or skipped — nothing left to run. */
  settled: number;
  done: number;
  failed: number;
  running: number;
}

export function progress(plan: Plan): Progress {
  const steps = allSteps(plan);
  const count = (...s: PlanStep['status'][]) => steps.filter((x) => s.includes(x.status)).length;
  return { total: steps.length, settled: count('done', 'skipped'), done: count('done'), failed: count('failed'), running: count('running') };
}

/** One line under the plan's name: where it stands. */
export function headline(plan: Plan): string {
  const p = progress(plan);
  const steps = `${p.total} ${p.total === 1 ? 'step' : 'steps'}`;
  switch (plan.status) {
    case 'draft': return p.total === 0 ? 'Draft · no steps yet' : `Draft · ${steps}`;
    case 'running': return `Running · ${p.settled} of ${p.total} done${p.running ? `, ${p.running} in progress` : ''}`;
    case 'paused': return `Paused${plan.pause_reason ? ` · ${plan.pause_reason}` : ''} · ${p.settled} of ${p.total} done`;
    case 'done': return `Done · ${steps}`;
    case 'failed': return `Failed · ${p.failed} of ${p.total} failed`;
    case 'cancelled': return `Stopped · ${p.settled} of ${p.total} done`;
  }
}

/** A step's outcome in a few words: its error, or the MR it opened, or nothing. */
export function stepOutcome(step: PlanStep): string {
  if (step.status === 'failed' && step.error) return step.error;
  if (step.result?.number) return `!${step.result.number}`;
  return '';
}

/** Whether the plan is still doing something (the card keeps the Stop / Review affordances while it is). */
export function isActive(plan: Plan): boolean {
  return plan.status === 'running' || plan.status === 'paused';
}
