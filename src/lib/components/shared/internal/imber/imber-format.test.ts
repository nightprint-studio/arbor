import { describe, expect, it } from 'vitest';
import { formatDuration, splitTime } from './imber-format';

describe('formatDuration', () => {
  it('writes each range in the unit that reads best', () => {
    expect(formatDuration(420)).toBe('420 ms');
    expect(formatDuration(8_200)).toBe('8.2s');
    expect(formatDuration(12_000)).toBe('12s');
    expect(formatDuration(192_000)).toBe('3m 12s');
    expect(formatDuration(120_000)).toBe('2m');
    expect(formatDuration(3_900_000)).toBe('1h 5m');
  });
});

describe('splitTime', () => {
  it('adds the model, the tools and the waits apart', () => {
    const split = splitTime([
      { kind: 'step', step: { ms: 8_000, outputTokens: 300 } },
      { kind: 'call', call: { id: 'a', kind: 'read', label: 'Read', detail: '', ms: 40 } },
      { kind: 'call', call: { id: 'b', kind: 'edit', label: 'Delete', detail: '', ms: 10, waitMs: 5_000 } },
    ]);
    expect(split).toEqual({ model: 8_000, tools: 50, waiting: 5_000, known: true });
  });

  it('knows nothing of a conversation saved before times were recorded', () => {
    expect(splitTime([{ kind: 'call', call: { id: 'a', kind: 'read', label: 'Read', detail: '' } }]).known).toBe(false);
  });
});
