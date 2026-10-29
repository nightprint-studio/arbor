import { describe, expect, it } from 'vitest';
import { retryLine, secondsLeft, type RetryStatus } from './assistant-retry';

const status = (until: number): RetryStatus => ({ attempt: 3, max: 11, until, kind: 'offline', message: 'No connection', detail: 'dns error' });

describe('the line shown while a request is waiting to be sent again', () => {
  it('counts the seconds left, rounding up', () => {
    expect(secondsLeft(status(10_000), 2_001)).toBe(8);
    expect(secondsLeft(status(10_000), 9_999)).toBe(1);
  });

  it('never goes below zero', () => {
    expect(secondsLeft(status(10_000), 12_000)).toBe(0);
  });

  it('says what went wrong, how long is left and which attempt it is', () => {
    expect(retryLine(status(10_000), 2_001)).toBe('No connection — retrying in 8 s (attempt 3/11)');
  });

  it('says it is retrying now once the wait is over', () => {
    expect(retryLine(status(10_000), 10_500)).toBe('No connection — retrying now (attempt 3/11)');
  });
});
