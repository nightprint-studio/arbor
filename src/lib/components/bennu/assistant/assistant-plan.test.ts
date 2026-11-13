import { describe, expect, it } from 'vitest';
import { latestPlan, planHeadline, planProgress } from './assistant-plan';
import type { AssistantMessage, PlanStep } from './assistant-types';

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

describe('the headline', () => {
  it('names the step in progress, by its place', () => {
    const p = planProgress([step('completed'), step('in_progress'), step('pending')]);
    expect(p.current).toBe(1);
    expect(planHeadline(p)).toBe('Step 2 of 3');
  });

  it('says done when every step is, and counts when nothing runs', () => {
    expect(planHeadline(planProgress([step('completed'), step('completed')]))).toBe('Done · 2 of 2');
    expect(planHeadline(planProgress([step('completed'), step('pending')]))).toBe('1 of 2 done');
  });
});

describe('the latest plan', () => {
  const msg = (id: string, blocks: AssistantMessage['blocks']) => ({ id, role: 'assistant', at: 0, blocks }) as AssistantMessage;

  it('is the last plan block of the last message that has one', () => {
    const older = { id: 'plan-r1', steps: [step('completed')] };
    const newer = { id: 'plan-r2', steps: [step('in_progress')] };
    const messages = [
      msg('a', [{ type: 'plan', plan: older }]),
      msg('b', [{ type: 'plan', plan: newer }, { type: 'text', text: 'working' }]),
      msg('c', [{ type: 'text', text: 'no plan here' }]),
    ];
    expect(latestPlan(messages)?.id).toBe('plan-r2');
  });

  it('is null for a conversation with no plan', () => {
    expect(latestPlan([msg('a', [{ type: 'text', text: 'hi' }])])).toBeNull();
  });
});
