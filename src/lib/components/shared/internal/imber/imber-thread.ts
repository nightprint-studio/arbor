/** Pure helpers for the shared Imber thread and its conversation switcher. */

import type { ConversationEntry } from './imber-types';

/** The reader is close enough to the end of the thread for it to keep following new text. */
export function isNearBottom(scrollHeight: number, scrollTop: number, clientHeight: number, slack = 80): boolean {
  return scrollHeight - scrollTop - clientHeight < slack;
}

export interface ConversationGroups { recent: ConversationEntry[]; earlier: ConversationEntry[] }

const DAY = 24 * 60 * 60 * 1000;

/** Today's conversations first, the rest under "Earlier" — each in the order given (newest first). */
export function groupConversations(list: ConversationEntry[], now: number): ConversationGroups {
  return {
    recent: list.filter((c) => now - c.updatedAt < DAY),
    earlier: list.filter((c) => now - c.updatedAt >= DAY),
  };
}
