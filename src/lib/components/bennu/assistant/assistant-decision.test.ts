import { describe, expect, it } from 'vitest';
import { DECISION_LEVELS, effectiveDecisionLevel, parseDecisionLevel } from './assistant-decision';

describe('decision level', () => {
  it('reads every saved word and falls back to asking at forks', () => {
    for (const l of DECISION_LEVELS) expect(parseDecisionLevel(l.id)).toBe(l.id);
    expect(parseDecisionLevel('nonsense')).toBe('ask_forks');
    expect(parseDecisionLevel(undefined)).toBe('ask_forks');
  });

  it('follows the conversation when it has its own level, else the setting', () => {
    expect(effectiveDecisionLevel('decide', 'ask_big')).toBe('decide');
    expect(effectiveDecisionLevel('', 'ask_big')).toBe('ask_big');
    expect(effectiveDecisionLevel(undefined, 'decide')).toBe('decide');
  });
});
