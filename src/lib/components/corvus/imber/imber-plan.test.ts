import { describe, expect, it } from 'vitest';
import type { Plan, PlanStep } from '$lib/types/corvus/plan';
import { allSteps, headline, isActive, livePlan, progress, stepOutcome } from './imber-plan';

const repo = { path: '/p/api', name: 'api' };
const step = (id: string, status: PlanStep['status'], extra: Partial<PlanStep> = {}): PlanStep => ({ kind: 'push', branch: 'x', id, repo, status, ...extra }) as PlanStep;
const plan = (status: Plan['status'], phases: PlanStep[][], fin: PlanStep[] = [], extra: Partial<Plan> = {}): Plan => ({
  id: 'p1', name: 'Release', status, template: false, created_ms: 0, updated_ms: 0, finally: fin,
  phases: phases.map((steps, i) => ({ id: `ph${i}`, name: `Phase ${i + 1}`, gate: { kind: 'phase_done', state: 'idle' }, steps })),
  ...extra,
});

describe('livePlan', () => {
  it('prefers the store copy, which moves as steps finish, and falls back to the snapshot', () => {
    const snap = plan('draft', [[step('a', 'queued')]]);
    const live = plan('running', [[step('a', 'running')]]);
    expect(livePlan(snap, [live])).toBe(live);
    expect(livePlan(snap, [])).toBe(snap);
    expect(livePlan(snap, [{ ...live, id: 'other' }])).toBe(snap);
  });
});

describe('progress and headline', () => {
  it('counts the steps of the phases and of finally', () => {
    const p = plan('running', [[step('a', 'done'), step('b', 'running')], [step('c', 'queued')]], [step('f', 'skipped')]);
    expect(allSteps(p)).toHaveLength(4);
    expect(progress(p)).toEqual({ total: 4, settled: 2, done: 1, failed: 0, running: 1 });
    expect(headline(p)).toBe('Running · 2 of 4 done, 1 in progress');
  });

  it('says where each state stands', () => {
    expect(headline(plan('draft', []))).toBe('Draft · no steps yet');
    expect(headline(plan('draft', [[step('a', 'queued')]]))).toBe('Draft · 1 step');
    expect(headline(plan('paused', [[step('a', 'done'), step('b', 'failed')]], [], { pause_reason: 'a step failed' }))).toBe('Paused · a step failed · 1 of 2 done');
    expect(headline(plan('done', [[step('a', 'done'), step('b', 'done')]]))).toBe('Done · 2 steps');
    expect(headline(plan('failed', [[step('a', 'failed')]]))).toBe('Failed · 1 of 1 failed');
    expect(headline(plan('cancelled', [[step('a', 'done'), step('b', 'queued')]]))).toBe('Stopped · 1 of 2 done');
  });

  it('a plan is active while it runs or waits on someone', () => {
    expect(['running', 'paused'].every((s) => isActive(plan(s as Plan['status'], [])))).toBe(true);
    expect(['draft', 'done', 'failed', 'cancelled'].some((s) => isActive(plan(s as Plan['status'], [])))).toBe(false);
  });
});

describe('stepOutcome', () => {
  it('shows the error of a failed step and the MR a step opened', () => {
    expect(stepOutcome(step('a', 'failed', { error: 'rejected: behind' }))).toBe('rejected: behind');
    expect(stepOutcome(step('b', 'done', { result: { number: 42 } }))).toBe('!42');
    expect(stepOutcome(step('c', 'queued'))).toBe('');
  });
});
