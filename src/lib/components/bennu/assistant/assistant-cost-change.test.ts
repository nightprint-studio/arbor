import { describe, expect, it } from 'vitest';
import { contextChange, inputEuros, modelChange } from './assistant-cost-change';

const base = { inputPricePerM: 5, eurPerUsd: 0.9 };

describe('cost of changing model or context', () => {
  it('prices tokens per million at the given factor', () => {
    expect(inputEuros(1_000_000, 5, 1, 1)).toBeCloseTo(5);
    expect(inputEuros(200_000, 5, 1.25, 0.9)).toBeCloseTo(1.125);
  });

  it('does not ask for a short conversation', () => {
    expect(modelChange({ ...base, used: 4_999 })).toBeNull();
    expect(contextChange({ ...base, used: 4_999, newSize: 5_000 })).toBeNull();
  });

  it('treats a model change as a cold cache: cache-write price, with the warm price for comparison', () => {
    const c = modelChange({ ...base, used: 100_000 })!;
    expect(c.kind).toBe('model');
    expect(c.euros).toBeCloseTo(0.5625);
    expect(c.warmEuros).toBeCloseTo(0.045);
  });

  it('a context change that leaves room does not touch the cache and does not ask', () => {
    expect(contextChange({ ...base, used: 100_000, newSize: 400_000 })).toBeNull();
  });

  it('a context size the conversation already fills forces a compaction and asks', () => {
    const c = contextChange({ ...base, used: 150_000, newSize: 160_000 })!;
    expect(c.kind).toBe('context');
    expect(c.tokens).toBe(150_000);
  });
});
