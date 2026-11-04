import { describe, it, expect, vi, afterEach } from 'vitest';

import { TimeoutError, withTimeout } from './with-timeout';

afterEach(() => vi.useRealTimers());

describe('withTimeout', () => {
  it('passes the answer through', async () => {
    await expect(withTimeout(Promise.resolve(7), 1000)).resolves.toBe(7);
  });

  it('passes a rejection through', async () => {
    await expect(withTimeout(Promise.reject(new Error('boom')), 1000)).rejects.toThrow('boom');
  });

  it('rejects when nothing answers in time', async () => {
    vi.useFakeTimers();
    const silent = new Promise<number>(() => {});
    const result = withTimeout(silent, 5000, 'The spell backend');
    const expectation = expect(result).rejects.toBeInstanceOf(TimeoutError);
    await vi.advanceTimersByTimeAsync(5000);
    await expectation;
    await expect(result).rejects.toThrow('The spell backend did not answer within 5s');
  });

  it('ignores an answer that comes too late', async () => {
    vi.useFakeTimers();
    let answer!: (n: number) => void;
    const slow = new Promise<number>((r) => { answer = r; });
    const result = withTimeout(slow, 100);
    const expectation = expect(result).rejects.toBeInstanceOf(TimeoutError);
    await vi.advanceTimersByTimeAsync(100);
    answer(1);
    await expectation;
  });
});
