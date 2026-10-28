/**
 * The smart-suggestions settings group: defaults, clamping, and the round trip to the config.
 *
 * What is worth pinning is the compatibility story — a config written before these settings
 * existed has none of the keys and must read as "everything on", never as a field of `undefined`s
 * that switches the feature off — and that a hand-edited strength cannot escape its range.
 */

import { describe, expect, it } from 'vitest';
import {
  clampStrength,
  MAX_STRENGTH,
  SMART_DEFAULTS,
  smartFromConfig,
  smartToConfig,
  type SmartCompletionSettings,
} from './smart-completion';

describe('smartFromConfig', () => {
  it('reads a config written before the settings existed as everything on', () => {
    expect(smartFromConfig({})).toEqual(SMART_DEFAULTS);
  });

  it('keeps an explicit off, which is falsy but is not missing', () => {
    const s = smartFromConfig({ completion_smart_chain: false, completion_smart_strength: 0 });
    expect(s.chain).toBe(false);
    expect(s.strength).toBe(0);
    expect(s.nameAffinity).toBe(true);
  });

  it('clamps a hand-edited strength', () => {
    expect(smartFromConfig({ completion_smart_strength: 9000 }).strength).toBe(MAX_STRENGTH);
    expect(smartFromConfig({ completion_smart_strength: -5 }).strength).toBe(0);
  });
});

describe('clampStrength', () => {
  it('rounds to a whole number and refuses what is not a number', () => {
    expect(clampStrength(99.6)).toBe(100);
    expect(clampStrength(Number.NaN)).toBe(SMART_DEFAULTS.strength);
    expect(clampStrength(Number.POSITIVE_INFINITY)).toBe(SMART_DEFAULTS.strength);
  });
});

describe('smartToConfig', () => {
  const custom: SmartCompletionSettings = {
    nameAffinity: false,
    projectUsage: true,
    sequence: false,
    chain: true,
    context: false,
    statements: true,
    ghostText: false,
    strength: 175,
  };

  it('round-trips every field', () => {
    expect(smartFromConfig(smartToConfig(custom))).toEqual(custom);
  });

  it('writes the snake_case keys the backend config declares', () => {
    expect(smartToConfig(custom)).toEqual({
      completion_smart_name_affinity: false,
      completion_smart_project_usage: true,
      completion_smart_sequence: false,
      completion_smart_chain: true,
      completion_smart_context: false,
      completion_smart_statements: true,
      completion_smart_ghost_text: false,
      completion_smart_strength: 175,
    });
  });

  it('never writes a strength outside the range', () => {
    expect(smartToConfig({ ...custom, strength: 10_000 }).completion_smart_strength).toBe(MAX_STRENGTH);
  });
});
