/** Number formatting and the editor-hint text for the profiler. Pure, so it is tested. */

import type { FileMetrics, MethodMetric } from '$lib/ipc/bennu/profiler';

export function fmtMs(ms: number): string {
  if (ms >= 60_000) return `${(ms / 60_000).toFixed(1)} min`;
  if (ms >= 10_000) return `${(ms / 1000).toFixed(0)} s`;
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.round(ms)} ms`;
}

export function fmtBytes(b: number): string {
  if (b >= 1 << 30) return `${(b / (1 << 30)).toFixed(1)} GB`;
  if (b >= 1 << 20) return `${(b / (1 << 20)).toFixed(1)} MB`;
  if (b >= 1 << 10) return `${(b / (1 << 10)).toFixed(0)} KB`;
  return `${b} B`;
}

export function fmtPct(p: number): string {
  if (p >= 10) return `${Math.round(p)}%`;
  if (p >= 0.1) return `${p.toFixed(1)}%`;
  return '<0.1%';
}

/** "38% CPU · 120 ms self · 4.2 MB alloc" — only the parts that carry information. */
export function hintLabel(m: MethodMetric): string {
  const parts = [`${fmtPct(m.total_pct)} CPU`];
  if (m.self_ms >= 1 && m.self_pct >= 0.5) parts.push(`${fmtMs(m.self_ms)} self`);
  if (m.alloc_bytes > 0) parts.push(`${fmtBytes(m.alloc_bytes)} alloc`);
  return parts.join(' · ');
}

export interface HeatMark {
  line: number;
  heat: number;
  label?: string;
  tooltip?: string;
  stale?: boolean;
}

/** The editor's marks for one file: a labelled bar on each measured method's signature line, and a
 *  bar-only mark on every hot line inside it. A line that is both keeps the label. */
export function heatMarks(fm: FileMetrics | null | undefined, hintsOn: boolean): HeatMark[] {
  if (!hintsOn || !fm || !fm.recording_id) return [];
  const stale = fm.stale;
  const staleNote = stale ? ' (the file changed since this recording)' : '';
  const marks: HeatMark[] = fm.metrics.lines.map((l) => ({
    line: l.line,
    heat: l.heat,
    tooltip: `${fmtPct(l.pct)} of samples on this line · ${fm.label}${staleNote}`,
    stale,
  }));
  for (const m of fm.metrics.methods) {
    marks.push({
      line: m.line,
      heat: Math.max(m.heat, 0.15),
      label: hintLabel(m),
      tooltip: `${m.name}: ${fmtPct(m.total_pct)} of the run in total, ${fmtPct(m.self_pct)} in the method itself (${fmtMs(m.self_ms)})` +
        (m.alloc_count ? `; ${m.alloc_count} sampled allocations, ${fmtBytes(m.alloc_bytes)}` : '') +
        ` — ${fm.label}${staleNote}`,
      stale,
    });
  }
  return marks;
}
