import { describe, expect, it } from 'vitest';
import { formatBytes, formatDuration, statusTone } from './format';

describe('response bar words', () => {
  it('writes durations the way a person reads them', () => {
    expect(formatDuration(184)).toBe('184 ms');
    expect(formatDuration(1840)).toBe('1.84 s');
    expect(formatDuration(12_300)).toBe('12.3 s');
    expect(formatDuration(75_000)).toBe('1 min 15 s');
  });

  it('writes sizes with the right unit', () => {
    expect(formatBytes(144)).toBe('144 B');
    expect(formatBytes(2300)).toBe('2.2 KB');
    expect(formatBytes(150 * 1024)).toBe('150 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
  });

  it('colours a status by its class', () => {
    expect([200, 204, 301, 404, 409, 500, 503].map(statusTone)).toEqual(['success', 'success', 'info', 'warning', 'warning', 'error', 'error']);
  });
});
