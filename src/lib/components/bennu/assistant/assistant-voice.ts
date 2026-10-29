/**
 * What Bennu tells the speech recogniser it is likely to hear: the project, the class being
 * edited, the files open next to it and the words in the current selection. Pure, so the
 * ordering and the filtering are tested; the caller reads the stores.
 */

import type { VoiceVocabulary } from '$lib/ipc/voice';

const IDENT = /[A-Za-z_$][A-Za-z0-9_$]{2,}/g;
const STOP = new Set(['the', 'and', 'for', 'new', 'null', 'true', 'false', 'return', 'this', 'class', 'public', 'private', 'static', 'void', 'int', 'string', 'import', 'package', 'final']);

/** `src/main/java/com/acme/OrderService.java` -> `OrderService`. */
export function stemOf(path: string | null | undefined): string | null {
  const name = path?.split(/[\\/]/).pop();
  if (!name) return null;
  const stem = name.replace(/\.[^.]+$/, '');
  return stem.length >= 3 ? stem : null;
}

/** Distinct identifier-looking words from `text`, longest-first among equals, capped. */
export function identifiersOf(text: string, limit = 12): string[] {
  const seen = new Map<string, number>();
  for (const m of text.match(IDENT) ?? []) {
    if (STOP.has(m.toLowerCase())) continue;
    seen.set(m, (seen.get(m) ?? 0) + 1);
  }
  return [...seen.entries()]
    .sort((a, b) => b[1] - a[1] || b[0].length - a[0].length)
    .slice(0, limit)
    .map(([w]) => w);
}

/** A word of the live transcript; `fresh` words arrived with the latest piece and are not settled yet. */
export interface TranscriptWord { text: string; fresh: boolean }

/**
 * The live text again, marked: words that were already there in the previous piece are settled, anything
 * after the first difference is fresh (new, or revised by the recogniser). Pure, so the overlay can dim and
 * fade in exactly the tail that just changed.
 */
export function markFresh(previous: string, next: string): TranscriptWord[] {
  const before = previous.split(/\s+/).filter(Boolean);
  const after = next.split(/\s+/).filter(Boolean);
  let settled = 0;
  while (settled < before.length && settled < after.length && before[settled].toLowerCase() === after[settled].toLowerCase()) settled++;
  return after.map((text, i) => ({ text, fresh: i >= settled }));
}

export type VoiceProblemKind = 'silent' | 'setup' | 'other';

export interface VoiceProblem { kind: VoiceProblemKind; title: string; detail: string }

/**
 * What to tell the user when voice did not work. "Silent" can simply be retried; "setup" (no engine, no
 * model, no microphone, access denied) needs the Voice settings; anything else shows the message as it is.
 */
export function classifyProblem(message: string): VoiceProblem {
  if (/no speech was heard/i.test(message)) {
    return { kind: 'silent', title: 'I did not catch that', detail: 'Nothing loud enough was said. Check that the right microphone is selected, then try again.' };
  }
  if (/\b(model|engine|microphone|mic|permission|access|denied|input device|audio)\b/i.test(message)) {
    return { kind: 'setup', title: 'Voice is not ready', detail: message };
  }
  return { kind: 'other', title: 'Voice did not work', detail: message };
}

export interface VoiceContext {
  projectName: string | null;
  activePath: string | null;
  openPaths: readonly string[];
  selection: string;
}

export function vocabularyFor(ctx: VoiceContext): VoiceVocabulary {
  const active = stemOf(ctx.activePath);
  const neighbours = ctx.openPaths.filter((p) => p !== ctx.activePath).map(stemOf).filter((s): s is string => !!s).slice(0, 12);
  return {
    project: ctx.projectName ?? '',
    active: [...(active ? [active] : []), ...identifiersOf(ctx.selection)],
    neighbours,
    recent: [],
  };
}
