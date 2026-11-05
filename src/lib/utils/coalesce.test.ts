import { describe, expect, it, vi, afterEach } from 'vitest';
import { coalesceBatch } from './coalesce';

// `schedule` picks `requestAnimationFrame` when it exists; the test environment may or may not have one,
// so every case drains by hand with `flush()` and never waits for a frame.

afterEach(() => vi.restoreAllMocks());

describe('coalesceBatch', () => {
  it('hands the payloads over in order, once', () => {
    const seen: number[][] = [];
    const push = coalesceBatch<number>((b) => seen.push(b));
    [1, 2, 3].forEach((n) => push(n));
    push.flush();
    expect(seen).toEqual([[1, 2, 3]]);
  });

  it('keeps only the newest payloads beyond `max`, oldest first out', () => {
    const seen: number[][] = [];
    const push = coalesceBatch<number>((b) => seen.push(b), { max: 3 });
    for (let n = 1; n <= 10; n++) push(n);
    push.flush();
    expect(seen).toEqual([[8, 9, 10]]);
  });

  it('never drops a payload `keep` names, and still trims the rest', () => {
    type E = { kind: 'out'; n: number } | { kind: 'exit' };
    const seen: E[][] = [];
    const push = coalesceBatch<E>((b) => seen.push(b), { max: 3, keep: (e) => e.kind === 'exit' });
    push({ kind: 'out', n: 1 });
    push({ kind: 'exit' });
    for (let n = 2; n <= 9; n++) push({ kind: 'out', n });
    push.flush();
    const flat = seen.flat();
    expect(flat.filter((e) => e.kind === 'exit')).toHaveLength(1);
    expect(flat.filter((e) => e.kind === 'out').map((e) => (e as { n: number }).n)).toEqual([8, 9]); // the exit counts toward the cap, and stays
  });

  it('bounds memory while nothing drains', () => {
    const seen: number[][] = [];
    const push = coalesceBatch<number>((b) => seen.push(b), { max: 100 });
    for (let n = 0; n < 5000; n++) push(n);
    push.flush();
    expect(seen[0]).toHaveLength(100);
    expect(seen[0][99]).toBe(4999);
  });
});
