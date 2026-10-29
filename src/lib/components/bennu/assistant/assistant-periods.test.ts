import { describe, expect, it } from 'vitest';
import { periodStarts } from './assistant-periods';

const at = (iso: string) => new Date(iso);
const local = (y: number, m: number, d: number) => new Date(y, m - 1, d).getTime();

describe('where the spending windows begin', () => {
  it('today begins at local midnight', () => {
    // Wednesday 14 October 2026, mid-afternoon.
    const p = periodStarts(at('2026-10-14T15:30:00'));
    expect(p.day).toBe(local(2026, 10, 14));
  });

  it('the week begins on the Monday before', () => {
    expect(periodStarts(at('2026-10-14T15:30:00')).week).toBe(local(2026, 10, 12));
  });

  it('a Monday is the start of its own week and a Sunday belongs to the week before', () => {
    expect(periodStarts(at('2026-10-12T08:00:00')).week).toBe(local(2026, 10, 12));
    expect(periodStarts(at('2026-10-18T23:59:00')).week).toBe(local(2026, 10, 12));
  });

  it('the week can begin in the previous month', () => {
    // Thursday 1 October 2026: its Monday was 28 September.
    const p = periodStarts(at('2026-10-01T10:00:00'));
    expect(p.week).toBe(local(2026, 9, 28));
    expect(p.month).toBe(local(2026, 10, 1));
  });

  it('the month begins on the first', () => {
    expect(periodStarts(at('2026-02-27T10:00:00')).month).toBe(local(2026, 2, 1));
  });

  it('each window starts no later than the one inside it', () => {
    const p = periodStarts(at('2026-10-14T15:30:00'));
    expect(p.month).toBeLessThanOrEqual(p.week);
    expect(p.week).toBeLessThanOrEqual(p.day);
  });
});
