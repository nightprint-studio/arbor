/**
 * The focus-aware timeout: a task that comes due in the background waits for focus, a cancelled
 * one never runs, and what is waiting is at most the latest task of each effect.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createFocusedTimers, type FocusEnv } from './focused-timeout';

/** A window whose focus the test controls. */
function fakeEnv(active = true) {
  let listener: (() => void) | null = null;
  const env: FocusEnv & { active: boolean; listening(): boolean; focus(): void; fire(): void } = {
    active,
    isActive() {
      return env.active;
    },
    onActivate(cb) {
      listener = cb;
      return () => {
        listener = null;
      };
    },
    listening: () => listener !== null,
    focus() {
      env.active = true;
      listener?.();
    },
    /** An activation event with the window's state left as it was. */
    fire() {
      listener?.();
    },
  };
  return env;
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('while the window is focused', () => {
  it('behaves like setTimeout', () => {
    const t = createFocusedTimers(fakeEnv(true));
    const task = vi.fn();
    t.setFocusedTimeout(task, 100);
    vi.advanceTimersByTime(99);
    expect(task).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(task).toHaveBeenCalledTimes(1);
  });

  it('can be cancelled before it fires', () => {
    const t = createFocusedTimers(fakeEnv(true));
    const task = vi.fn();
    const id = t.setFocusedTimeout(task, 100);
    t.clearFocusedTimeout(id);
    vi.advanceTimersByTime(500);
    expect(task).not.toHaveBeenCalled();
  });

  it('hands out ids that cannot be mistaken for a real timer', () => {
    const t = createFocusedTimers(fakeEnv(true));
    expect(t.setFocusedTimeout(() => {}, 1)).toBeLessThan(0);
  });
});

describe('while the window is in the background', () => {
  it('holds a due task until the window is focused', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const task = vi.fn();
    t.setFocusedTimeout(task, 100);
    vi.advanceTimersByTime(1000);
    expect(task).not.toHaveBeenCalled();
    expect(t.pending()).toBe(1);

    env.focus();
    expect(task).toHaveBeenCalledTimes(1);
    expect(t.pending()).toBe(0);
  });

  it('does not run a task that is not due yet when focus returns', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const early = vi.fn();
    const late = vi.fn();
    t.setFocusedTimeout(early, 100);
    t.setFocusedTimeout(late, 5000);
    vi.advanceTimersByTime(200);
    env.focus();
    expect(early).toHaveBeenCalledTimes(1);
    expect(late).not.toHaveBeenCalled();
    vi.advanceTimersByTime(5000);
    expect(late).toHaveBeenCalledTimes(1);
  });

  it('never runs a task that was cancelled while it waited', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const task = vi.fn();
    const id = t.setFocusedTimeout(task, 100);
    vi.advanceTimersByTime(200);
    t.clearFocusedTimeout(id);
    env.focus();
    expect(task).not.toHaveBeenCalled();
  });

  /** An effect re-runs, cancels its own previous timeout and schedules a new one: one is waiting. */
  it('keeps only the latest task of an effect that re-ran', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const first = vi.fn();
    const second = vi.fn();
    const a = t.setFocusedTimeout(first, 100);
    vi.advanceTimersByTime(200);
    t.clearFocusedTimeout(a);
    t.setFocusedTimeout(second, 100);
    vi.advanceTimersByTime(200);
    expect(t.pending()).toBe(1);
    env.focus();
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('flushes the waiting tasks in the order they were scheduled', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const order: string[] = [];
    t.setFocusedTimeout(() => order.push('first'), 200);
    t.setFocusedTimeout(() => order.push('second'), 100);
    vi.advanceTimersByTime(500);
    env.focus();
    expect(order).toEqual(['first', 'second']);
  });

  it('keeps flushing when one task throws', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const ok = vi.fn();
    t.setFocusedTimeout(() => {
      throw new Error('boom');
    }, 10);
    t.setFocusedTimeout(ok, 20);
    vi.advanceTimersByTime(100);
    env.focus();
    expect(ok).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it('stops listening for focus once nothing is waiting', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    t.setFocusedTimeout(() => {}, 10);
    vi.advanceTimersByTime(50);
    expect(env.listening()).toBe(true);
    env.focus();
    expect(env.listening()).toBe(false);
  });

  it('ignores a focus event that arrives while still inactive', () => {
    const env = fakeEnv(false);
    const t = createFocusedTimers(env);
    const task = vi.fn();
    t.setFocusedTimeout(task, 10);
    vi.advanceTimersByTime(50);
    // `visibilitychange` can fire with the window still unfocused.
    env.fire();
    expect(task).not.toHaveBeenCalled();
    expect(t.pending()).toBe(1);
    env.focus();
    expect(task).toHaveBeenCalledTimes(1);
  });
});

describe('clearFocusedTimeout', () => {
  it('accepts undefined and an unknown id, like clearTimeout', () => {
    const t = createFocusedTimers(fakeEnv(true));
    expect(() => {
      t.clearFocusedTimeout(undefined);
      t.clearFocusedTimeout(null);
      t.clearFocusedTimeout(-999);
    }).not.toThrow();
  });
});
