/**
 * Full-line completion IPC — the rest of the line at the caret, from the project's log profile or the small
 * local model. Kept in its own file so concurrent edits to the main bennu IPC surface don't race.
 *
 * Every offset is a **UTF-8 byte offset**, like the rest of the bennu contract.
 */

import { bennu } from '../rpc';

/** What the backend would write after the caret. */
/** What a suggestion was for, as the backend counts it; handed back untouched with what became of the suggestion. */
export type FlcContext = string;

export interface FlcReply {
  context?: FlcContext;
  /** The rest of the line; only a `derived` block (a constructor written from the fields) runs over several lines. */
  text: string;
  /** 0..1. For `log`, the share of the place's log calls the pattern holds. */
  confidence: number;
  /** Where it came from. */
  origin: 'log' | 'project' | 'model' | 'name' | 'mapping' | 'arguments' | 'habit' | 'annotation' | 'similar' | 'echo' | 'sequence' | 'derived' | 'ngram' | 'catch' | 'lambda' | 'assert' | 'next';
}

/** What is running, for the settings page. */
export interface FlcStatus {
  enabled: boolean;
  model_dir: string;
  /** `off`, `no folder`, `idle`, `loading`, `ready`, or `failed: …`. */
  model: string;
  /** Log calls counted, and the distinct patterns they reduce to. */
  log_calls: number;
  log_patterns: number;
  log_scanning: boolean;
}

/** The rest of the line at the caret, or `null` when nothing is sure enough. Wire: `bennu_flc_complete`. */
export function flcComplete(file: string, source: string, offset: number): Promise<FlcReply | null> {
  return bennu('bennu_flc_complete', { args: { file, source, offset } });
}

/** Start the background work for a project (count its log calls, load the model). Idempotent.
 *  Wire: `bennu_flc_prepare`. */
export function flcPrepare(root: string): Promise<void> {
  return bennu('bennu_flc_prepare', { args: { root } });
}

/** Wire: `bennu_flc_status`. */
export function flcStatus(): Promise<FlcStatus> {
  return bennu('bennu_flc_status', {});
}

/** What the editor did with a suggestion it drew. */
export type FlcOutcome = 'shown' | 'accepted' | 'dismissed';

/** One suggestion's outcome: where it came from and how sure it was — numbers, never the text. */
export interface FlcFeedbackEvent {
  origin: FlcReply['origin'];
  confidence: number;
  outcome: FlcOutcome;
}

/** Counts a batch of outcomes. `flush` asks the backend to write them to disk now. Wire: `bennu_flc_feedback`. */
export function flcFeedback(events: FlcFeedbackEvent[], flush = false): Promise<void> {
  return bennu('bennu_flc_feedback', { args: { events, flush } });
}

/**
 * What stood where a ghost text was drawn once the user was done with it. `after` is `null` when the text was never
 * taken. The text travels to the local backend only to be classified; it is stored only when the user asked for the
 * raw log.
 */
export interface FlcSettledEvent {
  origin: FlcReply['origin'];
  context?: FlcContext;
  ghost: string;
  after: string | null;
}

/** Tells the learner what became of ghost texts. A no-op in the backend while *Learn from corrections* is off.
 *  Answers with a line per correction counted while the debugging switch is on, and with none otherwise. Wire: `bennu_learn_settled`. */
export function flcLearnSettled(events: FlcSettledEvent[], flush = false): Promise<string[]> {
  return bennu('bennu_learn_settled', { args: { events, flush } });
}

/** Forgets everything the learner kept, counters and raw log. Wire: `bennu_learn_reset`. */
export function flcLearnReset(): Promise<void> {
  return bennu('bennu_learn_reset', {});
}

/** The counters of one kind of suggestion. */
export interface FlcOriginStats {
  origin: FlcReply['origin'];
  shown: number;
  accepted: number;
  dismissed: number;
}

/** What the settings page shows about how often suggestions are taken. */
export interface FlcStatsReport {
  origins: FlcOriginStats[];
  /** A confidence threshold in percent the model's suggestions are taken at least half the time above; offered, never applied. */
  suggested_threshold: number | null;
}

/** Wire: `bennu_flc_stats`. */
export function flcStats(): Promise<FlcStatsReport> {
  return bennu('bennu_flc_stats', {});
}

/** Forget what Bennu learned about how your projects are written — the saved profiles and what is in memory.
 *  Wire: `bennu_flc_forget`. */
export function flcForget(): Promise<void> {
  return bennu('bennu_flc_forget', {});
}

/** Forget every counter. Wire: `bennu_flc_stats_reset`. */
export function flcStatsReset(): Promise<void> {
  return bennu('bennu_flc_stats_reset', {});
}
