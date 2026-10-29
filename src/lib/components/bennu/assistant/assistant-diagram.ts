/**
 * The diagrams the assistant has drawn and is waiting to hear about.
 *
 * `show_diagram` cannot be checked by the backend — only Mermaid itself can say whether a source parses —
 * so the run parks on a prompt and the panel answers it. The answer is given from the store, as the
 * message arrives, not from the component that draws it: a diagram that lands in a conversation you are
 * not looking at still has to be answered, or the run would wait for it.
 */

import type { AssistantMessage } from './assistant-types';

export interface PendingDiagram {
  promptId: string;
  source: string;
}

/** The diagrams of a message that are waiting for a verdict and have not been taken up yet. */
export function pendingDiagrams(message: AssistantMessage, taken: ReadonlySet<string>): PendingDiagram[] {
  const out: PendingDiagram[] = [];
  for (const b of message.blocks) {
    if (b.type !== 'diagram' || b.diagram.status !== 'checking' || !b.promptId || taken.has(b.promptId)) continue;
    out.push({ promptId: b.promptId, source: b.diagram.source });
  }
  return out;
}

/** What the run is told: it is either fine, or here is why not. */
export function verdictAnswer(verdict: { ok: true } | { ok: false; error: string }): { ok: true } | { error: string } {
  return verdict.ok ? { ok: true } : { error: verdict.error };
}
