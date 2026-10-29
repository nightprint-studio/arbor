import { describe, expect, it } from 'vitest';
import {
  contextTone, estimateTokens, formatCost, formatEur, formatPrice, formatTokens, percentUsed, relativeTime, totalTokens,
} from './assistant-format';

describe('formatTokens', () => {
  it('writes small numbers whole and larger ones in K and M', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(842)).toBe('842');
    expect(formatTokens(1000)).toBe('1K');
    expect(formatTokens(37_500)).toBe('37.5K');
    expect(formatTokens(200_000)).toBe('200K');
    expect(formatTokens(1_000_000)).toBe('1M');
    expect(formatTokens(1_250_000)).toBe('1.3M');
  });

  it('drops the decimal once it stops mattering', () => {
    expect(formatTokens(120_400)).toBe('120K');
    expect(formatTokens(9_960)).toBe('10K');
  });
});

describe('the estimate', () => {
  it('counts four characters to a token, rounding up', () => {
    expect(estimateTokens('')).toBe(0);
    expect(estimateTokens('abcd')).toBe(1);
    expect(estimateTokens('abcde')).toBe(2);
  });

  it('sums a breakdown', () => {
    expect(totalTokens({ system: 6000, project: 18_000, conversation: 13_000 })).toBe(37_000);
  });
});

describe('how full the window is', () => {
  it('is a whole percentage, capped, and 0 for an empty window', () => {
    expect(percentUsed(37_500, 200_000)).toBe(19);
    expect(percentUsed(300_000, 200_000)).toBe(100);
    expect(percentUsed(10, 0)).toBe(0);
  });

  it('is calm, then worth watching, then full', () => {
    expect(contextTone(40)).toBe('ok');
    expect(contextTone(75)).toBe('warn');
    expect(contextTone(89)).toBe('warn');
    expect(contextTone(90)).toBe('full');
  });
});

describe('money', () => {
  it('writes cents, and marks a fraction of one', () => {
    expect(formatCost(0)).toBe('$0.00');
    expect(formatCost(0.004)).toBe('<$0.01');
    expect(formatCost(0.42)).toBe('$0.42');
    expect(formatCost(12)).toBe('$12.00');
  });

  it('writes a price as input / output', () => {
    expect(formatPrice(4, 20)).toBe('$4 / $20');
    expect(formatPrice(0.1, 0.5)).toBe('$0.10 / $0.50');
    expect(formatPrice(2, 10)).toBe('$2 / $10');
  });
});

describe('relativeTime', () => {
  const now = Date.UTC(2026, 9, 28, 12, 0, 0);
  const ago = (ms: number) => relativeTime(now - ms, now);

  it('reads like a person would say it', () => {
    expect(ago(10_000)).toBe('just now');
    expect(ago(5 * 60_000)).toBe('5 min ago');
    expect(ago(3 * 3_600_000)).toBe('3 h ago');
    expect(ago(30 * 3_600_000)).toBe('yesterday');
    expect(ago(4 * 86_400_000)).toBe('4 days ago');
  });

  it('never goes negative for a time a moment in the future', () => {
    expect(relativeTime(now + 5000, now)).toBe('just now');
  });
});

describe('formatEur', () => {
  it('shows euros, with a floor under a cent', () => {
    expect(formatEur(0)).toBe('€0.00');
    expect(formatEur(0.004)).toBe('<€0.01');
    expect(formatEur(0.42)).toBe('€0.42');
    expect(formatEur(12)).toBe('€12.00');
  });
});
