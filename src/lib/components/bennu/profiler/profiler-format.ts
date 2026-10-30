/** Number formatting and the editor-hint text for the profiler. Pure, so it is tested. */

import type { FileMetrics, LineMetric, MethodMetric, ProfilerThresholds } from '$lib/ipc/bennu/profiler';
import { DEFAULT_THRESHOLDS, SEVERITY_NAMES, severityColor, severityOf, type Severity } from './profiler-filter';

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

/** What a method is labelled with when no sample landed in it. */
export const NOT_SAMPLED = 'not sampled';

/**
 * "38% CPU · 120 ms self · 4.2 MB alloc · 1.2 s waiting on db.internal:5432" — only the parts that carry
 * information. A method that only waits has no CPU worth saying, and one that allocates says what it allocates.
 */
export function hintLabel(m: MethodMetric): string {
  const parts: string[] = [];
  if (m.total_pct >= 0.1 || (m.wait_ms ?? 0) <= 0) parts.push(`${fmtPct(m.total_pct)} CPU`);
  if (m.self_ms >= 1 && m.self_pct >= 0.5) parts.push(`${fmtMs(m.self_ms)} self`);
  if (m.alloc_bytes > 0) parts.push(`${fmtBytes(m.alloc_bytes)} alloc${m.alloc_top_type ? ` (${m.alloc_top_type})` : ''}`);
  if ((m.wait_ms ?? 0) >= 1) parts.push(`${fmtMs(m.wait_ms)} waiting${m.wait_on ? ` on ${m.wait_on.replace(/^\w+ /, '')}` : ''}`);
  return parts.join(' · ');
}

/** What a line says: only the dimensions that stand out, so a hot line reads "12% · 4.2 MB alloc". */
export function lineLabel(l: LineMetric): string {
  const parts: string[] = [];
  if (l.pct >= 0.1) parts.push(`${fmtPct(l.pct)} CPU`);
  if ((l.alloc_bytes ?? 0) > 0) parts.push(`${fmtBytes(l.alloc_bytes)} alloc`);
  if ((l.wait_ms ?? 0) >= 1) parts.push(`${fmtMs(l.wait_ms)} wait`);
  return parts.join(' · ');
}

export interface HeatMark {
  line: number;
  heat: number;
  label?: string;
  tooltip?: string;
  stale?: boolean;
  /** The mark's colour on the severity scale — grey while unremarkable. */
  color?: string;
}

/** What an unremarkable reading is drawn in: a mark that is not a problem must not look like one. */
const UNREMARKABLE = 'hsl(215 12% 50%)';

/** The worst of a reading's levels on the project's scales: CPU share, allocation share, time blocked. */
function worst(cpuPct: number, allocPct: number, waitMs: number, t: ProfilerThresholds): Severity {
  return Math.max(
    severityOf(cpuPct, t.cpu_self_pct),
    severityOf(allocPct, t.alloc_share_pct),
    severityOf(waitMs, t.lock_total_ms),
  ) as Severity;
}

/**
 * The editor's marks for one file.
 *
 * * **Every method** gets one on its signature line: the numbers it earned, or `not sampled` in grey when the
 *   recording never caught it. That is not the same as fast — a sampler taking a stack every ~10 ms does not see
 *   a method shorter than that, nor one whose whole cost is waiting — and saying nothing would read as "no data".
 * * **A line that stands out** (on any of CPU, allocation or waiting) is labelled in place, so the cost sits on
 *   the line that causes it; the others carry the gutter bar alone.
 *
 * A line that is both a signature and hot keeps the method's label.
 */
export function heatMarks(
  fm: FileMetrics | null | undefined,
  hintsOn: boolean,
  thresholds: ProfilerThresholds = DEFAULT_THRESHOLDS,
): HeatMark[] {
  if (!hintsOn || !fm || !fm.recording_id) return [];
  const stale = fm.stale;
  const staleNote = stale ? ' (the file changed since this recording)' : '';
  const marks: HeatMark[] = fm.metrics.lines.map((l) => {
    const level = worst(l.pct, l.alloc_pct ?? 0, l.wait_ms ?? 0, thresholds);
    const label = level > 0 ? lineLabel(l) : '';
    return {
      line: l.line,
      heat: l.heat,
      color: severityColor(level) ?? UNREMARKABLE,
      label: label || undefined,
      tooltip:
        `${fmtPct(l.pct)} of samples on this line` +
        ((l.alloc_bytes ?? 0) > 0 ? `; ${fmtBytes(l.alloc_bytes)} allocated from it (including what it calls)` : '') +
        ((l.wait_ms ?? 0) >= 1 ? `; ${fmtMs(l.wait_ms)} blocked on it (including what it calls)` : '') +
        ` · ${fm.label}${staleNote}`,
      stale,
    };
  });
  for (const m of fm.metrics.methods) {
    if (m.sampled === false) {
      marks.push({
        line: m.line,
        heat: 0.1,
        color: UNREMARKABLE,
        label: NOT_SAMPLED,
        tooltip:
          `${m.name}: no sample landed in this method — it ran for less than the sampling interval, did not run in this ` +
          `recording, or only waited (see the Waits tab). Not the same as fast. · ${fm.label}${staleNote}`,
        stale,
      });
      continue;
    }
    const level = worst(m.self_pct, m.alloc_pct ?? 0, m.wait_ms ?? 0, thresholds);
    marks.push({
      color: severityColor(level) ?? UNREMARKABLE,
      line: m.line,
      heat: Math.max(m.heat, 0.15),
      label: hintLabel(m),
      tooltip:
        `${m.name}: ${fmtPct(m.total_pct)} of the run in total, ${fmtPct(m.self_pct)} in the method itself (${fmtMs(m.self_ms)})` +
        (m.alloc_bytes > 0 ? `; ${fmtBytes(m.alloc_bytes)} allocated from its own code${m.alloc_top_type ? `, mostly ${m.alloc_top_type}` : ''}` : '') +
        ((m.wait_ms ?? 0) >= 1 ? `; blocked ${fmtMs(m.wait_ms)} off the CPU${m.wait_on ? ` on ${m.wait_on}` : ''}` : '') +
        (level > 0 ? ` — ${SEVERITY_NAMES[level]}` : '') +
        ` — ${fm.label}${staleNote}`,
      stale,
    });
  }
  return marks;
}
