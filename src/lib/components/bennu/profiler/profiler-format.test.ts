import { describe, expect, it } from 'vitest';
import { fmtBytes, fmtMs, fmtPct, heatMarks, hintLabel, lineLabel, NOT_SAMPLED } from './profiler-format';
import type { FileMetrics, LineMetric, MethodMetric } from '$lib/ipc/bennu/profiler';
import { DEFAULT_THRESHOLDS, severityColor } from './profiler-filter';

const method = (over: Partial<MethodMetric> = {}): MethodMetric => ({
  line: 10,
  name: 'spin',
  self_pct: 25,
  total_pct: 38,
  self_ms: 120,
  total_ms: 180,
  alloc_bytes: 0,
  alloc_count: 0,
  alloc_pct: 0,
  alloc_top_type: '',
  wait_ms: 0,
  wait_on: '',
  sampled: true,
  heat: 1,
  ...over,
});

const line = (over: Partial<LineMetric> = {}): LineMetric => ({ line: 12, pct: 9, heat: 0.4, alloc_bytes: 0, alloc_pct: 0, wait_ms: 0, ...over });

const fm = (over: Partial<FileMetrics> = {}): FileMetrics => ({
  recording_id: 'java-1',
  label: 'App',
  created_ms: 1,
  stale: false,
  metrics: {
    methods: [method()],
    lines: [line(), line({ line: 10, pct: 1, heat: 0.1 })],
  },
  ...over,
});

const GREY = 'hsl(215 12% 50%)';

describe('profiler formatting', () => {
  it('formats durations, sizes and shares', () => {
    expect(fmtMs(120)).toBe('120 ms');
    expect(fmtMs(1500)).toBe('1.5 s');
    expect(fmtBytes(1 << 20)).toBe('1.0 MB');
    expect(fmtPct(0.01)).toBe('<0.1%');
    expect(fmtPct(38.4)).toBe('38%');
  });

  it('writes the hint the way the editor shows it', () => {
    expect(hintLabel(method())).toBe('38% CPU · 120 ms self');
    expect(hintLabel(method({ alloc_bytes: 4 << 20 }))).toBe('38% CPU · 120 ms self · 4.0 MB alloc');
  });

  it('names the type allocated and what was waited on, dropping the CPU of a method that only waits', () => {
    const waiter = method({ self_pct: 0, total_pct: 0, self_ms: 0, total_ms: 0, alloc_bytes: 3 << 20, alloc_top_type: 'byte[]', wait_ms: 1200, wait_on: 'socket db.internal:5432' });
    expect(hintLabel(waiter)).toBe('3.0 MB alloc (byte[]) · 1.2 s waiting on db.internal:5432');
  });

  it('labels a line with only the dimensions that carry something', () => {
    expect(lineLabel(line({ pct: 12 }))).toBe('12% CPU');
    expect(lineLabel(line({ pct: 0, alloc_bytes: 2 << 20, wait_ms: 250 }))).toBe('2.0 MB alloc · 250 ms wait');
  });
});

describe('heatMarks', () => {
  it('labels the signature line, and a line that stands out, but not a quiet one', () => {
    const marks = heatMarks(fm(), true);
    expect(marks.some((m) => m.line === 10 && m.label?.includes('CPU'))).toBe(true);
    // 9% on a line is above the yellow threshold: it is labelled in place.
    expect(marks.find((m) => m.line === 12)?.label).toBe('9.0% CPU');
    // A line of 1% carries the gutter bar alone.
    expect(marks.find((m) => m.line === 10 && !m.label)?.label).toBeUndefined();
  });

  it('marks every method: one the recording never caught says so, in grey', () => {
    const quiet = method({ name: 'cold', line: 30, sampled: false, self_pct: 0, total_pct: 0, self_ms: 0, total_ms: 0, heat: 0 });
    const marks = heatMarks(fm({ metrics: { methods: [method(), quiet], lines: [] } }), true);
    const cold = marks.find((m) => m.line === 30)!;
    expect(cold.label).toBe(NOT_SAMPLED);
    expect(cold.color).toBe(GREY);
    expect(cold.tooltip).toContain('Not the same as fast');
  });

  it('colours each mark on the project’s severity scale, grey while unremarkable', () => {
    const marks = heatMarks(fm(), true);
    // 25% self on the method is orange on the default 5/15/30; a line at 9% is yellow; at 1% nothing to see.
    expect(marks.find((m) => m.label?.includes('CPU') && m.line === 10)?.color).toBe(severityColor(2));
    expect(marks.find((m) => m.line === 12)?.color).toBe(severityColor(1));
    expect(marks.find((m) => m.line === 10 && !m.label)?.color).toBe(GREY);
    // Raising the thresholds quiets the method; the colour follows them, not a fixed number.
    const lax = { ...DEFAULT_THRESHOLDS, cpu_self_pct: { yellow: 50, orange: 60, red: 70 } };
    expect(heatMarks(fm(), true, lax).find((m) => m.line === 10 && m.label)?.color).toBe(GREY);
  });

  it('colours a method by its worst dimension: a long wait is red whatever its CPU', () => {
    const waiter = method({ self_pct: 0.1, total_pct: 0.1, wait_ms: 4000, wait_on: 'socket db:5432' });
    const marks = heatMarks(fm({ metrics: { methods: [waiter], lines: [] } }), true);
    expect(marks[0].color).toBe(severityColor(3));
  });

  it('draws nothing when off, without a recording, and dims when stale', () => {
    expect(heatMarks(fm(), false)).toEqual([]);
    expect(heatMarks(fm({ recording_id: null }), true)).toEqual([]);
    expect(heatMarks(fm({ stale: true }), true).every((m) => m.stale)).toBe(true);
  });
});
