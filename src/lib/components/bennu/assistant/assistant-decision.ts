/**
 * How much the assistant decides for itself, in one place: the three levels, what each means in a line, and
 * how a saved word is read. The settings page, the chat's menu and the command palette all draw from here.
 */

export type DecisionLevel = 'ask_forks' | 'ask_big' | 'decide';

export interface DecisionInfo {
  id: DecisionLevel;
  label: string;
  description: string;
}

export const DECISION_LEVELS: readonly DecisionInfo[] = [
  { id: 'ask_forks', label: 'Ask at forks', description: 'Stops to ask when a choice is yours: a name, a scope, a trade-off, two reasonable ways.' },
  { id: 'ask_big', label: 'Only big decisions', description: 'Decides names and small trade-offs itself; asks about architecture, new dependencies and anything hard to undo.' },
  { id: 'decide', label: 'Decide itself', description: 'Picks the best option and goes on; asks only when truly blocked, and lists its decisions at the end.' },
];

export const DEFAULT_DECISION_LEVEL: DecisionLevel = 'ask_forks';

/** Longest note kept; mirrors the backend's cap. */
export const MAX_DECISION_NOTES = 2000;

export function parseDecisionLevel(word: string | null | undefined): DecisionLevel {
  return DECISION_LEVELS.find((l) => l.id === word?.trim().toLowerCase())?.id ?? DEFAULT_DECISION_LEVEL;
}

export function decisionInfo(level: DecisionLevel): DecisionInfo {
  return DECISION_LEVELS.find((l) => l.id === level) ?? DECISION_LEVELS[0];
}

/** The level a conversation follows: its own when it has one, else the setting. */
export function effectiveDecisionLevel(own: string | undefined, global: DecisionLevel): DecisionLevel {
  return own?.trim() ? parseDecisionLevel(own) : global;
}
