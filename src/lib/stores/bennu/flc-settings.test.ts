import { describe, expect, it } from 'vitest';
import {
  FLC_DEFAULTS,
  MAX_CONFIDENCE,
  MIN_CONFIDENCE,
  clampConfidence,
  flcFromConfig,
  flcToConfig,
  statsLines,
} from './flc-settings';

describe('flc settings', () => {
  it('reads an old config as the defaults', () => {
    expect(flcFromConfig({})).toEqual(FLC_DEFAULTS);
  });

  it('round-trips through the config', () => {
    const s = { enabled: true, modelDir: '/models/flc', minConfidence: 70, logs: false, alternatives: true, check: false, feedback: false, learn: true, ignoreCommentedCode: true, learnKeepText: true, learnNotify: true, layerNames: false, layerNgram: false, layerCreation: true, layerEcho: true, layerHabits: true, profileProject: false, profileGlobal: false };
    expect(flcFromConfig(flcToConfig(s))).toEqual(s);
  });

  it('keeps the threshold inside what the stepper offers', () => {
    expect(clampConfidence(0)).toBe(MIN_CONFIDENCE);
    expect(clampConfidence(500)).toBe(MAX_CONFIDENCE);
    expect(clampConfidence(Number.NaN)).toBe(FLC_DEFAULTS.minConfidence);
  });

  it('trims the folder it writes', () => {
    expect(flcToConfig({ ...FLC_DEFAULTS, modelDir: '  /m  ' }).flc_model_dir).toBe('/m');
  });
});

describe('flc stats lines', () => {
  it('writes one line per kind that was shown, in a fixed order, with the share taken', () => {
    const lines = statsLines([
      { origin: 'model', shown: 41, accepted: 12, dismissed: 20 },
      { origin: 'log', shown: 8, accepted: 6, dismissed: 1 },
      { origin: 'name', shown: 0, accepted: 0, dismissed: 0 },
    ]);
    expect(lines).toEqual(['log statements: 8 shown, 6 taken (75%)', 'model: 41 shown, 12 taken (29%)']);
  });

  it('is empty when nothing was shown', () => {
    expect(statsLines([])).toEqual([]);
  });
});
