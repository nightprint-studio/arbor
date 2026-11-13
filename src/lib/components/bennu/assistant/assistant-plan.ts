/**
 * Counting a plan: how many steps are done, how many are being worked on, and the percentage.
 *
 * The percentage is steps done over steps planned, and says nothing about how long the rest will take —
 * which is the honest limit of what a list of steps can tell. Pure, so it is tested.
 */

import type { AssistantMessage, PlanBlock, PlanStep } from './assistant-types';

export interface PlanProgress {
  total: number;
  done: number;
  inProgress: number;
  /** Whole percent of steps done, 0–100. */
  percent: number;
  finished: boolean;
  /** The step being worked on (the first in progress), by index; `null` when none is. */
  current: number | null;
}

/** The card's headline: where the work is, in words — "Step 2 of 6", "Done", or the count when nothing is running. */
export function planHeadline(p: PlanProgress): string {
  if (p.finished) return `Done · ${p.total} of ${p.total}`;
  if (p.current !== null) return `Step ${p.current + 1} of ${p.total}`;
  return `${p.done} of ${p.total} done`;
}

export function planProgress(steps: readonly PlanStep[]): PlanProgress {
  const total = steps.length;
  const done = steps.filter((s) => s.status === 'completed').length;
  const inProgress = steps.filter((s) => s.status === 'in_progress').length;
  return {
    total,
    done,
    inProgress,
    percent: total === 0 ? 0 : Math.round((done / total) * 100),
    finished: total > 0 && done === total,
    current: inProgress ? steps.findIndex((s) => s.status === 'in_progress') : null,
  };
}

/** The progress bar's two parts: steps done and the one in progress. */
export function planSegments(p: PlanProgress) {
  return [
    { id: 'done', label: 'Done', value: p.done, tone: 'success' as const },
    { id: 'doing', label: 'In progress', value: p.inProgress, tone: 'accent' as const },
  ];
}

/** The newest plan in a conversation — the one the work follows now — or `null` when it has none. */
export function latestPlan(messages: readonly AssistantMessage[]): PlanBlock | null {
  for (let m = messages.length - 1; m >= 0; m--) {
    const blocks = messages[m].blocks;
    for (let b = blocks.length - 1; b >= 0; b--) {
      const block = blocks[b];
      if (block.type === 'plan') return block.plan;
    }
  }
  return null;
}
