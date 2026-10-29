import { describe, expect, it } from 'vitest';
import { planProgress } from './assistant-plan';
import type { PlanStep } from './assistant-types';

const step = (status: PlanStep['status']): PlanStep => ({ text: 'x', status });

describe('counting a plan', () => {
  it('counts done and in-progress steps and rounds the percentage', () => {
    const p = planProgress([step('completed'), step('completed'), step('in_progress'), step('pending'), step('pending'), step('pending'), step('pending')]);
    expect(p).toMatchObject({ total: 7, done: 2, inProgress: 1, percent: 29, finished: false });
  });

  it('is finished only when every step is done', () => {
    expect(planProgress([step('completed'), step('completed')])).toMatchObject({ percent: 100, finished: true });
    expect(planProgress([step('completed'), step('pending')]).finished).toBe(false);
  });

  it('has no percentage for an empty plan, and is not finished', () => {
    expect(planProgress([])).toMatchObject({ total: 0, percent: 0, finished: false });
  });

  it('goes down when the plan grows — a step added is work still to do', () => {
    const before = planProgress([step('completed'), step('completed')]).percent;
    const after = planProgress([step('completed'), step('completed'), step('pending')]).percent;
    expect(after).toBeLessThan(before);
  });
});
