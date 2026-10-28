/**
 * The per-project build lanes: different projects overlap, one project's builds line up, and a
 * task that fails never leaves its lane blocked.
 */

import { describe, expect, it, vi } from 'vitest';
import { createRootLocks, rootKey } from './build-locks';

/** A task that finishes when the test says so. */
function gate() {
  let open!: (value?: string) => void;
  const promise = new Promise<string>((resolve) => {
    open = (v = 'done') => resolve(v);
  });
  return { promise, open };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('rootKey', () => {
  it('treats every spelling of one root as the same', () => {
    expect(rootKey('C:\\Sviluppo\\App\\')).toBe(rootKey('c:/sviluppo/app'));
  });
});

describe('different projects', () => {
  /** The reported case: B must not wait for A's compile. */
  it('run at the same time', async () => {
    const locks = createRootLocks();
    const a = gate();
    const b = gate();
    const started: string[] = [];
    const runA = locks.run('a', async () => { started.push('a'); return a.promise; });
    const runB = locks.run('b', async () => { started.push('b'); return b.promise; });
    await tick();
    expect(started).toEqual(['a', 'b']);
    a.open();
    b.open();
    await Promise.all([runA, runB]);
  });
});

describe('one project', () => {
  it('queues a second task behind the first, in order', async () => {
    const locks = createRootLocks();
    const first = gate();
    const order: string[] = [];
    const one = locks.run('a', async () => { order.push('first:start'); await first.promise; order.push('first:end'); });
    const two = locks.run('a', async () => { order.push('second:start'); });
    await tick();
    expect(order).toEqual(['first:start']);
    expect(locks.waiting('a')).toBe(1);
    first.open();
    await Promise.all([one, two]);
    expect(order).toEqual(['first:start', 'first:end', 'second:start']);
  });

  it('keeps a long queue in the order it was asked', async () => {
    const locks = createRootLocks();
    const order: number[] = [];
    await Promise.all([1, 2, 3, 4].map((n) => locks.run('a', async () => { order.push(n); })));
    expect(order).toEqual([1, 2, 3, 4]);
  });

  it('hands each task its own result', async () => {
    const locks = createRootLocks();
    const [x, y] = await Promise.all([locks.run('a', async () => 1), locks.run('a', async () => 2)]);
    expect([x, y]).toEqual([1, 2]);
  });

  it('is not blocked for ever by a task that threw', async () => {
    const locks = createRootLocks();
    const failing = locks.run('a', async () => { throw new Error('boom'); });
    const next = locks.run('a', async () => 'ran');
    await expect(failing).rejects.toThrow('boom');
    await expect(next).resolves.toBe('ran');
    expect(locks.isBusy('a')).toBe(false);
  });
});

describe('busy state', () => {
  it('reports a project busy from the moment a task is asked until it is done', async () => {
    const locks = createRootLocks();
    const g = gate();
    expect(locks.isBusy('a')).toBe(false);
    const run = locks.run('a', () => g.promise);
    expect(locks.isBusy('a')).toBe(true);
    expect(locks.isBusy('b')).toBe(false);
    g.open();
    await run;
    expect(locks.isBusy('a')).toBe(false);
    expect(locks.busyKeys()).toEqual([]);
  });

  it('tells its owner whenever the set of busy projects changes', async () => {
    const seen: string[][] = [];
    const locks = createRootLocks(() => seen.push([]));
    const spy = vi.fn();
    const locks2 = createRootLocks(spy);
    await locks.run('a', async () => {});
    await locks2.run('a', async () => {});
    expect(seen.length).toBe(2); // once when it started, once when it ended
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('lists every busy project', async () => {
    const locks = createRootLocks();
    const a = gate();
    const b = gate();
    const ra = locks.run('a', () => a.promise);
    const rb = locks.run('b', () => b.promise);
    expect(locks.busyKeys().sort()).toEqual(['a', 'b']);
    a.open();
    b.open();
    await Promise.all([ra, rb]);
  });
});
