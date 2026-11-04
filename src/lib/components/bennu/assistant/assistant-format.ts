/**
 * How the Bennu assistant panel writes numbers — the figures every Imber panel shares live in
 * `shared/internal/imber/imber-format`; what is here concerns Bennu's context window.
 */

import type { ContextBreakdown } from './assistant-types';

export { formatCost, formatEur, formatPrice, formatTokens, relativeTime } from '$lib/components/shared/internal/imber/imber-format';

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
