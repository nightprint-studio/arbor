/**
 * How the assistant panel writes numbers and times — pure, so it can be tested and reused.
 */

import type { ContextBreakdown } from './assistant-types';

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

/** The tokens in some text, estimated at four characters each — close enough to steer by, and honest
 *  about being a guess: the real count comes from the API once a request has been made. */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

export function totalTokens(c: ContextBreakdown): number {
  return c.system + c.project + c.conversation;
}

/** How full the window is, as a whole percentage (0 for a window of nothing). */
export function percentUsed(used: number, max: number): number {
  return max > 0 ? Math.min(100, Math.round((used / max) * 100)) : 0;
}

export type ContextTone = 'ok' | 'warn' | 'full';

/** Calm until it matters: past 75% the window is worth watching, past 90% the next answer may not fit. */
export function contextTone(percent: number): ContextTone {
  if (percent >= 90) return 'full';
  if (percent >= 75) return 'warn';
  return 'ok';
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
