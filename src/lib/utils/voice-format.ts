/**
 * Plain-language wording for voice models and test runs. Pure, so the sentences a person reads
 * ("about 2 s for a 10 s sentence") are tested rather than eyeballed.
 */

import type { Comfort } from '$lib/ipc/voice';

/** Accuracy 1–5 as a word. */
export function accuracyLabel(level: number): string {
  return ['', 'Basic', 'Fair', 'Good', 'Very good', 'Excellent'][Math.max(1, Math.min(5, Math.round(level)))];
}

/** How a model feels on this machine, from its real-time factor (seconds of work per second of speech). */
export function speedLabel(rtf: number): string {
  if (rtf <= 0.15) return 'Instant';
  if (rtf <= 0.4) return 'Fast';
  if (rtf <= 0.65) return 'Moderate';
  if (rtf <= 1) return 'Slow';
  return 'Too slow for this PC';
}

/** What a "Test it" run says, for a 10-second sentence at the measured speed. */
export function testVerdict(audioSecs: number, tookSecs: number, comfort: Comfort | null): string {
  const per10 = (tookSecs / Math.max(0.1, audioSecs)) * 10;
  const about = per10 < 1 ? 'under 1 s' : `about ${Math.round(per10)} s`;
  const feel: Record<Comfort, string> = {
    instant: 'Instant.',
    comfortable: 'Comfortable.',
    slow: 'A bit slow - a smaller model will feel snappier.',
    too_slow: 'Too slow for dictation on this PC - pick a smaller model.',
  };
  return `${about} for a 10 s sentence. ${comfort ? feel[comfort] : ''}`.trim();
}

/** 0.0-1.0 progress of a download, or `null` while the total is unknown. */
export function progressOf(done: number, total: number | null): number | null {
  return total && total > 0 ? Math.min(1, done / total) : null;
}

/**
 * How long the page waits for a transcription of `audioSecs` before it gives up and resets: the
 * backend's own deadline (20 s + 8x the audio, at least 30 s) plus a margin for the hand-off.
 * Mirrors `voice_core::plan::transcription_timeout`.
 */
export function transcribeDeadlineMs(audioSecs: number): number {
  const backend = Math.max(30, 20 + Math.max(0, audioSecs) * 8);
  return Math.round((backend + 15) * 1000);
}

/** "0.6 s of work per second of speech", as a short phrase for a model row. */
export function rtfLabel(rtf: number, measured: boolean): string {
  const text = rtf < 0.1 ? 'under 0.1 s' : `${rtf.toFixed(1)} s`;
  return `${text} per second of speech${measured ? ' (measured)' : ' (estimated)'}`;
}
