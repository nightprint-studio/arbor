/**
 * What a download should tell the person waiting for it: how far along it is, how much is left, how fast it is
 * going and how long that means. Pure — no store, no clock of its own — so the rules are tested without a window
 * and every download (a speech model, a tool, a sound bank) can say the same things the same way.
 */

import { formatBytes } from '$lib/utils/format';

export interface DownloadStats {
  /** 0–100, or `null` while the total is not known. */
  percent: number | null;
  done: number;
  total: number | null;
  /** Bytes still to come, or `null` while the total is not known. */
  remaining: number | null;
  /** Bytes per second over the last few seconds, or `null` before there is enough to say. */
  speed: number | null;
  /** Seconds to go at that speed, or `null` when it cannot be told yet. */
  etaSeconds: number | null;
}

interface Sample { t: number; done: number }

/** Below this span of time a speed would be noise (two samples a few milliseconds apart). */
const MIN_SPAN_MS = 800;

/**
 * Feeds on `(done, total)` readings and answers with the stats. The speed is the average over the last
 * `windowMs`, not over the whole download: a model that stalled and then caught up should not keep promising
 * the ETA of its slow start, and one that began slowly should not stay pessimistic for the rest.
 */
export function createDownloadMeter(windowMs = 6000) {
  let samples: Sample[] = [];

  return {
    push(done: number, total: number | null, now: number = Date.now()): DownloadStats {
      // A reading that goes backwards is a restart (a retry): forget what came before.
      if (samples.length && done < samples[samples.length - 1].done) samples = [];
      samples.push({ t: now, done });
      samples = samples.filter((s) => now - s.t <= windowMs);

      const first = samples[0];
      const last = samples[samples.length - 1];
      const span = last.t - first.t;
      const speed = span >= MIN_SPAN_MS && last.done > first.done ? ((last.done - first.done) * 1000) / span : null;

      const known = total != null && total > 0;
      const remaining = known ? Math.max(0, total - done) : null;
      return {
        percent: known ? Math.min(100, (done / total) * 100) : null,
        done,
        total: known ? total : null,
        remaining,
        speed,
        etaSeconds: speed && remaining != null ? remaining / speed : null,
      };
    },
    reset() { samples = []; },
  };
}

/** `4.2 MB/s`. */
export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** `a few seconds left`, `42 s left`, `about 3 min left`, `1 h 12 min left`. */
export function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '';
  if (seconds < 5) return 'a few seconds left';
  if (seconds < 60) return `${Math.round(seconds / 5) * 5} s left`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `about ${minutes} min left`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m ? `${h} h ${m} min left` : `${h} h left`;
}

/** The one line under a progress bar: `26% · 121 MB of 465 MB · 344 MB left · 4.2 MB/s · about 1 min left`. */
export function describeProgress(s: DownloadStats): string {
  if (s.total == null) return `${formatBytes(s.done)} downloaded`;
  const parts = [
    `${Math.floor(s.percent ?? 0)}%`,
    `${formatBytes(s.done)} of ${formatBytes(s.total)}`,
    `${formatBytes(s.remaining ?? 0)} left`,
  ];
  if (s.speed) parts.push(formatRate(s.speed));
  if (s.etaSeconds != null) parts.push(formatEta(s.etaSeconds));
  return parts.join(' · ');
}
