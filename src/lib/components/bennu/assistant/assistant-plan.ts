/**
 * Counting a plan: how many steps are done, how many are being worked on, and the percentage.
 *
 * The percentage is steps done over steps planned, and says nothing about how long the rest will take —
 * which is the honest limit of what a list of steps can tell. Pure, so it is tested.
 */

import type { PlanStep } from './assistant-types';

export interface PlanProgress {
  total: number;
  done: number;
  inProgress: number;
  /** Whole percent of steps done, 0–100. */
  percent: number;
  finished: boolean;
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
  };
}
