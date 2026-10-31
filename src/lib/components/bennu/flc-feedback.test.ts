import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createFeedbackQueue } from './flc-feedback';
import type { FlcFeedbackEvent } from '$lib/ipc/bennu/flc';

const event = (outcome: FlcFeedbackEvent['outcome']): FlcFeedbackEvent => ({ origin: 'model', confidence: 0.8, outcome });

describe('feedback queue', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('sends what piled up in one call after the interval, not one call per event', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const q = createFeedbackQueue(send, 1000);
    q.push(event('shown'));
    q.push(event('accepted'));
    expect(send).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1000);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0][0]).toHaveLength(2);
    expect(send.mock.calls[0][1]).toBe(false);
  });

  it('flushes at once and marks the last call when asked to', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const q = createFeedbackQueue(send, 1000);
    q.push(event('shown'));
    q.flush(true);
    expect(send).toHaveBeenCalledWith([event('shown')], true);
    vi.advanceTimersByTime(5000);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('sends nothing when nothing is waiting, unless it is the final write', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const q = createFeedbackQueue(send, 1000);
    q.flush();
    expect(send).not.toHaveBeenCalled();
    q.flush(true);
    expect(send).toHaveBeenCalledWith([], true);
  });

  it('survives a send that fails', async () => {
    const send = vi.fn().mockRejectedValue(new Error('backend down'));
    const q = createFeedbackQueue(send, 1000);
    q.push(event('shown'));
    expect(() => vi.advanceTimersByTime(1000)).not.toThrow();
    await Promise.resolve();
  });

  it('keeps a bounded backlog', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const q = createFeedbackQueue(send, 1000);
    for (let i = 0; i < 700; i++) q.push(event('shown'));
    q.flush();
    expect(send.mock.calls[0][0]).toHaveLength(500);
  });
});
