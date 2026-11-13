import { describe, expect, it } from 'vitest';
import { clampMaxSteps, DEFAULT_MAX_STEPS, MAX_MAX_STEPS, MIN_MAX_STEPS } from './assistant-steps';

describe('clampMaxSteps', () => {
  it('reads unset as the default and keeps the rest in range', () => {
    expect(clampMaxSteps(0)).toBe(DEFAULT_MAX_STEPS);
    expect(clampMaxSteps(Number.NaN)).toBe(DEFAULT_MAX_STEPS);
    expect(clampMaxSteps(2)).toBe(MIN_MAX_STEPS);
    expect(clampMaxSteps(80.4)).toBe(80);
    expect(clampMaxSteps(5000)).toBe(MAX_MAX_STEPS);
  });
});
