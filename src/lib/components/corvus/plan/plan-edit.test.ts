import { describe, expect, it } from 'vitest';
import type { Plan, PlanStep } from '$lib/types/corvus/plan';
import { FINALLY, columns, locate, moveAcross, moveWithin, neighbour, removeStep, setGate } from './plan-edit';

const repo = { path: '/r', name: 'r' };
const step = (id: string): PlanStep => ({ id, kind: 'pull', repo, status: 'queued' });

const plan = (): Plan => ({
  id: 'p', name: 'p', status: 'draft', template: false, created_ms: 0, updated_ms: 0,
  phases: [
    { id: 'a', name: 'A', gate: { kind: 'phase_done', state: 'idle' }, steps: [step('1'), step('2')] },
    { id: 'b', name: 'B', gate: { kind: 'phase_done', state: 'idle' }, steps: [step('3')] },
  ],
  finally: [step('4')],
});

describe('plan-edit', () => {
  it('lays out phases then finally', () => {
    expect(columns(plan()).map(c => c.id)).toEqual(['a', 'b', FINALLY]);
  });

  it('finds a step by column and row', () => {
    expect(locate(plan(), '3')).toEqual({ column: 1, index: 0 });
    expect(locate(plan(), 'nope')).toBeNull();
  });

  it('removes a step', () => {
    const p = plan();
    expect(removeStep(p, '2')).toBe(true);
    expect(p.phases[0].steps.map(s => s.id)).toEqual(['1']);
  });

  it('moves within a column and refuses to leave it', () => {
    const p = plan();
    expect(moveWithin(p, '1', 1)).toBe(true);
    expect(p.phases[0].steps.map(s => s.id)).toEqual(['2', '1']);
    expect(moveWithin(p, '1', 1)).toBe(false);
  });

  it('moves across columns, up to finally, and not past the ends', () => {
    const p = plan();
    expect(moveAcross(p, '3', 1)).toBe(true);
    expect(p.finally.map(s => s.id)).toEqual(['3', '4']);
    expect(moveAcross(p, '3', 1)).toBe(false);
    expect(moveAcross(p, '1', -1)).toBe(false);
  });

  it('replaces a gate and resets its state', () => {
    const p = plan();
    p.phases[1].gate.state = 'open';
    expect(setGate(p, 'b', { kind: 'delay', secs: 300 })).toBe(true);
    expect(p.phases[1].gate).toMatchObject({ kind: 'delay', secs: 300, state: 'idle' });
    expect(setGate(p, 'zzz', { kind: 'manual' })).toBe(false);
  });

  it('keeps the row when the arrows cross columns and skips empty ones', () => {
    const p = plan();
    expect(neighbour(p, '2', 1, 0)).toBe('3');
    p.phases[1].steps = [];
    expect(neighbour(p, '2', 1, 0)).toBe('4');
    expect(neighbour(p, '1', 0, 1)).toBe('2');
    expect(neighbour(p, '2', 0, 1)).toBe('2');
    expect(neighbour(p, null, 1, 0)).toBe('1');
  });
});
