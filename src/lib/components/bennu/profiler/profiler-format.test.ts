import { describe, expect, it } from 'vitest';
import { fmtBytes, fmtMs, fmtPct, heatMarks, hintLabel } from './profiler-format';
import type { FileMetrics } from '$lib/ipc/bennu/profiler';

const fm = (over: Partial<FileMetrics> = {}): FileMetrics => ({
  recording_id: 'java-1',
  label: 'App',
  created_ms: 1,
  stale: false,
  metrics: {
    methods: [{ line: 10, name: 'spin', self_pct: 25, total_pct: 38, self_ms: 120, total_ms: 180, alloc_bytes: 0, alloc_count: 0, heat: 1 }],
    lines: [{ line: 12, pct: 9, heat: 0.4 }, { line: 10, pct: 1, heat: 0.1 }],
  },
  ...over,
});

describe('profiler formatting', () => {
  it('formats durations, sizes and shares', () => {
    expect(fmtMs(120)).toBe('120 ms');
    expect(fmtMs(1500)).toBe('1.5 s');
    expect(fmtBytes(1 << 20)).toBe('1.0 MB');
    expect(fmtPct(0.01)).toBe('<0.1%');
    expect(fmtPct(38.4)).toBe('38%');
  });

  it('writes the hint the way the editor shows it', () => {
    expect(hintLabel(fm().metrics.methods[0])).toBe('38% CPU · 120 ms self');
    const withAlloc = { ...fm().metrics.methods[0], alloc_bytes: 4 << 20 };
    expect(hintLabel(withAlloc)).toBe('38% CPU · 120 ms self · 4.0 MB alloc');
  });

  it('labels the signature line and keeps the label over a bar-only mark', () => {
    const marks = heatMarks(fm(), true);
    const at10 = marks.filter((m) => m.line === 10);
    expect(at10.some((m) => m.label)).toBe(true);
    expect(marks.find((m) => m.line === 12)?.label).toBeUndefined();
  });

  it('draws nothing when off, without a recording, and dims when stale', () => {
    expect(heatMarks(fm(), false)).toEqual([]);
    expect(heatMarks(fm({ recording_id: null }), true)).toEqual([]);
    expect(heatMarks(fm({ stale: true }), true).every((m) => m.stale)).toBe(true);
  });
});
