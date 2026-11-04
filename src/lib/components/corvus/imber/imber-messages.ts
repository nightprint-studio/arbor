/** Pure helpers over the Imber thread: how a streamed message joins the list, what is waiting on the user. */

import type { ImberApproval, ImberBlock, ImberMessage, ImberToolCall } from '$lib/types/corvus/imber';

/** A streamed message replaces the one with its id (it arrives whole, each time, as it stands). */
export function upsertMessage(list: ImberMessage[], message: ImberMessage): ImberMessage[] {
  const at = list.findIndex((m) => m.id === message.id);
  if (at < 0) return [...list, message];
  const next = list.slice();
  next[at] = message;
  return next;
}

export type Pending = { promptId: string; approval: ImberApproval };

/** The approval the run is parked on, if any: undecided, with a prompt to answer. */
export function pendingApproval(list: ImberMessage[]): Pending | null {
  for (let i = list.length - 1; i >= 0; i--) {
    for (const b of list[i].blocks) {
      if (b.type === 'approval' && b.promptId && b.approval.answer === null) return { promptId: b.promptId, approval: b.approval };
    }
  }
  return null;
}

type Shown = Exclude<ImberBlock, { type: 'tool' }> | { type: 'tools'; calls: ImberToolCall[] };

/** Runs of tool receipts fold into one entry: what it did on the way is one line, opened if you care. */
export function foldTools(blocks: ImberBlock[]): Shown[] {
  const out: Shown[] = [];
  for (const b of blocks) {
    if (b.type !== 'tool') { out.push(b); continue; }
    const last = out.at(-1);
    if (last?.type === 'tools') last.calls.push(b.call);
    else out.push({ type: 'tools', calls: [b.call] });
  }
  return out;
}

const VERB: Record<ImberToolCall['kind'], string> = { read: 'Looked', search: 'Searched', edit: 'Changed', validate: 'Checked', refactor: 'Planned' };

/** `Looked 3 times · Changed 2 times · 1 refused` — the receipts in one breath. */
export function summarizeTools(calls: ImberToolCall[]): string {
  const counts = new Map<ImberToolCall['kind'], number>();
  for (const c of calls.filter((x) => x.ok)) counts.set(c.kind, (counts.get(c.kind) ?? 0) + 1);
  const failed = calls.filter((x) => !x.ok).length;
  const parts = [...counts].map(([kind, n]) => `${VERB[kind]} ${n === 1 ? 'once' : `${n} times`}`);
  if (failed) parts.push(`${failed} failed`);
  return parts.join(' · ');
}

/** What a schedule's wake-up looks like in the thread: who woke it, and the saved prompt without the `[Scheduled: …]` tag the model got. */
export function scheduledText(note: string, prompt: string): string {
  return `⏰ ${note}\n\n${prompt.replace(/^\[Scheduled: [^\]]*\]\s*/, '')}`;
}

/** The text of an assistant message, for copying. */
export function plainText(message: ImberMessage): string {
  return message.blocks.flatMap((b) => (b.type === 'text' ? [b.text] : [])).join('\n\n');
}
