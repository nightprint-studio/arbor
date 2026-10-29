/**
 * Pure helpers for the Live panel: how a duration, a clock time, a process state and a timeline event
 * are written, and how the arrow keys move through a list. No store, no IPC — so the rules are testable.
 */

import type { LiveEvent, LiveProcess } from '$lib/ipc/bennu/live';

/** `250 ms`, `1.5 s`, `2 min 5 s`. */
export function durationText(ms: number): string {
  if (ms < 1000) return `${Math.max(0, Math.round(ms))} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.floor(ms / 60_000)} min ${Math.floor((ms / 1000) % 60)} s`;
}

/** Local `hh:mm:ss.mmm` of an epoch-milliseconds time. */
export function clockText(ms: number): string {
  const d = new Date(ms);
  const p = (n: number, w = 2) => String(n).padStart(w, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

export type StateTone = 'success' | 'warning' | 'error' | 'neutral';

/** The word and tone of a process's state. A non-zero exit is an error; a stop we asked for is not. */
export function processState(p: Pick<LiveProcess, 'phase' | 'exit_code'>): { label: string; tone: StateTone } {
  switch (p.phase) {
    case 'ready': return { label: 'ready', tone: 'success' };
    case 'running': return { label: 'starting', tone: 'warning' };
    case 'stopped': return { label: 'stopped', tone: 'neutral' };
    case 'exited':
      return p.exit_code === 0
        ? { label: 'exited 0', tone: 'neutral' }
        : { label: `exited ${p.exit_code ?? '?'}`, tone: 'error' };
  }
}

/** How long a process has run (or ran), measured against `now`. */
export function uptimeText(p: Pick<LiveProcess, 'started_ms' | 'ended_ms'>, now: number): string {
  return durationText((p.ended_ms ?? now) - p.started_ms);
}

/** One timeline row as text: time, who, what. */
export function eventText(e: LiveEvent): string {
  const who = e.kind === 'http_request' ? 'http →' : e.kind === 'http_response' ? 'http ←' : e.source;
  const corr = e.corr && e.kind !== 'log' ? ` [${e.corr}]` : '';
  return `${clockText(e.ts_ms)}  ${who.padEnd(10)}${corr} ${e.text}`;
}

/** The CSS class a timeline row gets, by what it is. */
export function eventClass(e: LiveEvent): string {
  switch (e.kind) {
    case 'http_request': return 'line-http-req';
    case 'http_response': return /^(4|5)\d\d|failed/.test(e.text) ? 'line-error' : 'line-http-res';
    case 'process_started':
    case 'process_ready': return 'line-marker';
    case 'process_exited': return 'line-marker';
    case 'marker': return 'line-marker';
    default: return (e.data as { stream?: string } | undefined)?.stream === 'stderr' ? 'line-stderr' : '';
  }
}

/** The index the arrow keys move to; stays inside the list. */
export function stepIndex(current: number, count: number, key: string): number {
  if (count === 0) return -1;
  if (key === 'ArrowDown') return Math.min(count - 1, current < 0 ? 0 : current + 1);
  if (key === 'ArrowUp') return Math.max(0, current < 0 ? 0 : current - 1);
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  return current;
}
