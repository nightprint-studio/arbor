/**
 * How many requests one run may make before it stops and asks to be told to continue. The same rule as
 * `imber-core::steps` (which is what the backend applies); kept here so the setting shows the value that runs.
 */
export const DEFAULT_MAX_STEPS = 40;
export const MIN_MAX_STEPS = 5;
export const MAX_MAX_STEPS = 300;

/** The configured steps per run, kept to a sane range; `0` (unset) or a non-number is the default. */
export function clampMaxSteps(steps: number): number {
  if (!Number.isFinite(steps) || steps <= 0) return DEFAULT_MAX_STEPS;
  return Math.min(MAX_MAX_STEPS, Math.max(MIN_MAX_STEPS, Math.round(steps)));
}
