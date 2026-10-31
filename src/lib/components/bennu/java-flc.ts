/**
 * The ghost text for Java that comes from the full-line completion: the rest of the line the caret is in,
 * from the project's log profile or from the small local model (see `bennu-flc`).
 *
 * Unlike the other inline sources, which return only what is *certain*, this one can return a guess — which is
 * why it is a setting, off by default for the model, and why it never answers while both halves are off.
 */

import type { EditorView } from '@codemirror/view';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import type { InlineCompletion, InlineCompletionOutcomeHandler, SettledHandler } from '$lib/components/shared/ui/code-editor';
import { flcComplete, type FlcReply } from '$lib/ipc/bennu/flc';
import { flcFeedbackQueue, flcLearnQueue } from './flc-feedback';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';

/** What the editor hands back about a suggestion: where it came from and how sure it was — never its text. */
export type FlcMeta = Pick<FlcReply, 'origin' | 'confidence' | 'context'>;

export const javaFlcSource = async (view: EditorView, pos: number): Promise<InlineCompletion | null> => {
  const flc = bennuSettingsStore.flc;
  if (!flc.enabled && !flc.logs) return null;
  const path = projectStore.activeFilePath;
  if (!path || !path.toLowerCase().endsWith('.java')) return null;
  const src = view.state.doc.toString();
  const reply = await flcComplete(path, src, makeU16ToByte(src)(pos)).catch(() => null);
  if (!reply || !reply.text) return null;
  const meta: FlcMeta = { origin: reply.origin, confidence: reply.confidence, context: reply.context };
  return { text: reply.text, meta };
};

/** Counts what became of each suggestion (shown, taken, dismissed) when *Count how often suggestions are taken* is on. */
export const javaFlcOutcome: InlineCompletionOutcomeHandler = (kind, meta) => {
  const m = meta as FlcMeta | undefined;
  if (!m) return;
  if (bennuSettingsStore.flc.feedback) flcFeedbackQueue.push({ origin: m.origin, confidence: m.confidence, outcome: kind });
  // a suggestion that was never taken has no text to compare: the learner only needs to know it was passed over
  if (kind === 'dismissed' && bennuSettingsStore.flc.learn) flcLearnQueue.push({ origin: m.origin, context: m.context, ghost: '', after: null });
};

/** Tells the learner what stands where an accepted suggestion was written once the user is done with it, when *Learn from corrections* is on. */
export const javaFlcSettled: SettledHandler = ({ ghost, after, meta }) => {
  const m = meta as FlcMeta | undefined;
  if (!m || !bennuSettingsStore.flc.learn) return;
  flcLearnQueue.push({ origin: m.origin, context: m.context, ghost, after });
};
