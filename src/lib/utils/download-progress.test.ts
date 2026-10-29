import { describe, expect, it } from 'vitest';
import { createDownloadMeter, describeProgress, formatEta, formatRate } from './download-progress';

const MB = 1024 * 1024;

describe('createDownloadMeter', () => {
  it('knows the percentage and what is left as soon as the total is known', () => {
    const m = createDownloadMeter();
    const s = m.push(100 * MB, 400 * MB, 0);
    expect(s.percent).toBe(25);
    expect(s.remaining).toBe(300 * MB);
  });

  it('has no speed or ETA from a single reading', () => {
    const s = createDownloadMeter().push(10 * MB, 100 * MB, 0);
    expect(s.speed).toBeNull();
    expect(s.etaSeconds).toBeNull();
  });

  it('measures the speed over the recent readings and derives the ETA from it', () => {
    const m = createDownloadMeter();
    m.push(0, 100 * MB, 0);
    const s = m.push(10 * MB, 100 * MB, 2000); // 5 MB/s
    expect(s.speed).toBeCloseTo(5 * MB, 0);
    expect(s.etaSeconds).toBeCloseTo(18, 0); // 90 MB left at 5 MB/s
  });

  it('forgets a slow start: only the last window counts', () => {
    const m = createDownloadMeter(6000);
    m.push(0, 1000 * MB, 0);
    m.push(1 * MB, 1000 * MB, 10_000); // crawled for ten seconds
    m.push(11 * MB, 1000 * MB, 12_000); // then 10 MB in 2 s
    const s = m.push(21 * MB, 1000 * MB, 14_000);
    expect(s.speed).toBeCloseTo(5 * MB, 0);
  });

  it('starts over when the reading goes backwards (a retry)', () => {
    const m = createDownloadMeter();
    m.push(50 * MB, 100 * MB, 0);
    const s = m.push(1 * MB, 100 * MB, 1000);
    expect(s.speed).toBeNull();
  });

  it('works without a total: bytes so far, no percentage, no ETA', () => {
    const m = createDownloadMeter();
    m.push(0, null, 0);
    const s = m.push(5 * MB, null, 2000);
    expect(s.percent).toBeNull();
    expect(s.remaining).toBeNull();
    expect(s.etaSeconds).toBeNull();
    expect(describeProgress(s)).toBe('5.0 MB downloaded');
  });
});

describe('formatEta / formatRate / describeProgress', () => {
  it('rounds the time to what a person can read', () => {
    expect(formatEta(2)).toBe('a few seconds left');
    expect(formatEta(42)).toBe('40 s left');
    expect(formatEta(95)).toBe('about 2 min left');
    expect(formatEta(4320)).toBe('1 h 12 min left');
    expect(formatEta(7200)).toBe('2 h left');
    expect(formatEta(Number.NaN)).toBe('');
  });

  it('writes a rate', () => {
    expect(formatRate(4.2 * MB)).toBe('4.2 MB/s');
  });

  it('describes a download on one line', () => {
    const line = describeProgress({ percent: 26, done: 121 * MB, total: 465 * MB, remaining: 344 * MB, speed: 4.2 * MB, etaSeconds: 82 });
    expect(line).toBe('26% · 121 MB of 465 MB · 344 MB left · 4.2 MB/s · about 1 min left');
  });
});
