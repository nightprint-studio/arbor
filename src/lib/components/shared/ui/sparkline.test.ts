import { describe, expect, it } from 'vitest';
import { sparkGeometry } from './sparkline';

describe('sparkGeometry', () => {
  it('puts the largest value at the top and zero at the bottom', () => {
    const g = sparkGeometry([0, 10], 100, 50, undefined, 0);
    expect(g.points).toBe('0.0,50.0 100.0,0.0');
    expect(g.max).toBe(10);
  });

  it('draws nothing for fewer than two values', () => {
    expect(sparkGeometry([], 100, 50).points).toBe('');
    expect(sparkGeometry([5], 100, 50).points).toBe('');
  });

  it('widens the scale to include a reference line', () => {
    const g = sparkGeometry([5, 5], 100, 50, 20, 0);
    expect(g.max).toBe(20);
    expect(g.referenceY).toBe(0);
    expect(g.points).toBe('0.0,37.5 100.0,37.5');
  });

  it('treats a non-finite value as zero rather than breaking the line', () => {
    const g = sparkGeometry([Number.NaN, 4], 10, 10, undefined, 0);
    expect(g.points.startsWith('0.0,10.0')).toBe(true);
  });
});
