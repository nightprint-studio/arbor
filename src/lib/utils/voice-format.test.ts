import { describe, expect, it } from 'vitest';
import { accuracyLabel, progressOf, rtfLabel, speedLabel, testVerdict, transcribeDeadlineMs } from './voice-format';

describe('voice wording', () => {
  it('labels accuracy and clamps', () => {
    expect(accuracyLabel(1)).toBe('Basic');
    expect(accuracyLabel(5)).toBe('Excellent');
    expect(accuracyLabel(99)).toBe('Excellent');
  });

  it('scales speed with the machine', () => {
    expect(speedLabel(0.1)).toBe('Instant');
    expect(speedLabel(0.6)).toBe('Moderate');
    expect(speedLabel(2.3)).toBe('Too slow for this PC');
  });

  it('turns a measurement into a sentence', () => {
    expect(testVerdict(5, 1, 'comfortable')).toBe('about 2 s for a 10 s sentence. Comfortable.');
    expect(testVerdict(10, 0.5, 'instant')).toContain('under 1 s');
    expect(testVerdict(5, 10, 'too_slow')).toContain('Too slow');
  });

  it('knows when progress is unknowable', () => {
    expect(progressOf(5, null)).toBeNull();
    expect(progressOf(50, 100)).toBe(0.5);
    expect(progressOf(500, 100)).toBe(1);
  });

  it('bounds how long the page waits for a transcription', () => {
    expect(transcribeDeadlineMs(0)).toBe(45_000);
    expect(transcribeDeadlineMs(5)).toBe(75_000);
    expect(transcribeDeadlineMs(60)).toBeGreaterThan(400_000);
  });

  it('words the real-time factor', () => {
    expect(rtfLabel(0.6, true)).toBe('0.6 s per second of speech (measured)');
    expect(rtfLabel(0.05, false)).toContain('under 0.1 s');
  });
});
