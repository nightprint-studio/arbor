/**
 * How the Imber chat writes numbers and times — pure, shared by every product's panel.
 */

import type { WorkItem } from './imber-types';

/** `37500` → `37.5K`, `1000000` → `1M`; below a thousand, the number itself. */
export function formatTokens(tokens: number): string {
  if (tokens < 1000) return String(Math.round(tokens));
  if (tokens < 1_000_000) {
    const k = tokens / 1000;
    return `${k >= 100 ? Math.round(k) : trim(k.toFixed(1))}K`;
  }
  return `${trim((tokens / 1_000_000).toFixed(1))}M`;
}

/** `12.0` → `12`, `37.5` → `37.5`. */
function trim(text: string): string {
  return text.replace(/\.0$/, '');
}

/** `$0.42`, and `<$0.01` for the fractions of a cent a short conversation costs. */
export function formatCost(dollars: number): string {
  if (dollars <= 0) return '$0.00';
  if (dollars < 0.01) return '<$0.01';
  return `$${dollars.toFixed(2)}`;
}

/** `€0.42`, `<€0.01` — an amount in euros. */
export function formatEur(euros: number): string {
  if (euros <= 0) return '€0.00';
  if (euros < 0.01) return '<€0.01';
  return `€${euros.toFixed(2)}`;
}

/** `$4 / $20` — input and output, per million tokens. */
export function formatPrice(input: number, output: number): string {
  // Whole dollars bare (`$4`), fractions to the cent (`$0.10`, `$2.50`).
  const p = (n: number) => (Number.isInteger(n) ? String(n) : n.toFixed(2));
  return `$${p(input)} / $${p(output)}`;
}

/** `420` → `420 ms`, `8200` → `8.2s`, `192000` → `3m 12s`, `3_900_000` → `1h 5m`. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${trim((ms / 1000).toFixed(1))}s`;
  if (ms < 3_600_000) {
    const m = Math.floor(ms / 60_000);
    const s = Math.round((ms % 60_000) / 1000);
    return s ? `${m}m ${s}s` : `${m}m`;
  }
  const h = Math.floor(ms / 3_600_000);
  const m = Math.round((ms % 3_600_000) / 60_000);
  return m ? `${h}h ${m}m` : `${h}h`;
}

/** Where a run's time went: the model's answers, the tools running, and the user answering cards. */
export interface TimeSplit {
  model: number;
  tools: number;
  waiting: number;
  /** Whether any of it was recorded — older conversations have no times. */
  known: boolean;
}

/** The time split of a run's rows. */
export function splitTime(items: WorkItem[]): TimeSplit {
  const split: TimeSplit = { model: 0, tools: 0, waiting: 0, known: false };
  for (const item of items) {
    if (item.kind === 'step') {
      split.model += item.step.ms;
      split.known = true;
    } else if (item.call.ms !== undefined) {
      split.tools += item.call.ms;
      split.waiting += item.call.waitMs ?? 0;
      split.known = true;
    }
  }
  return split;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** `just now`, `5 min ago`, `3 h ago`, `yesterday`, `4 days ago`, then the date. */
export function relativeTime(at: number, now: number): string {
  const age = Math.max(0, now - at);
  if (age < MINUTE) return 'just now';
  if (age < HOUR) return `${Math.floor(age / MINUTE)} min ago`;
  if (age < DAY) return `${Math.floor(age / HOUR)} h ago`;
  if (age < 2 * DAY) return 'yesterday';
  if (age < 7 * DAY) return `${Math.floor(age / DAY)} days ago`;
  return new Date(at).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}
