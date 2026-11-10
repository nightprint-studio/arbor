/**
 * Imber in Corvus — the conversation and the commit-message generator, served by `corvus-be`.
 *
 * The API key is **not** here: it is the one Bennu uses, in the shell's credential vault. Setting or
 * clearing it goes through the same direct shell commands (re-exported below), never through a backend.
 */

import { corvus } from '../rpc';
import type { ImberMode, ImberState } from '$lib/types/corvus/imber';

export { hasAssistantKey, storeAssistantKey, deleteAssistantKey } from '../bennu/assistant';

export const IMBER_TOPIC = 'arbor://corvus/imber';

export interface ImberSendParams {
  /** `null` starts a conversation. */
  conversation_id: string | null;
  text: string;
  /** The repository the conversation is about. */
  root: string;
  /** A conversation with no tools. */
  chat: boolean;
  /** Send the conversation again as it stands (a run that failed), with no new message. */
  retry?: boolean;
  mode: ImberMode;
}

export const imberState = () => corvus<ImberState>('imber_state');

export const imberSetMode = (mode: ImberMode) => corvus<ImberMode>('imber_set_mode', { mode });

export const imberSend = (args: ImberSendParams) =>
  corvus<{ run_id: string; conversation_id: string }>('imber_send', { args });

export const imberCancel = (runId: string) => corvus<boolean>('imber_cancel', { run_id: runId });

/** Answer an approval card. */
export const imberReply = (promptId: string, answer: 'approve' | 'reject') =>
  corvus<boolean>('imber_reply', { prompt_id: promptId, answer: { answer } });

export const imberForget = (conversationId: string) => corvus<void>('imber_forget', { conversation_id: conversationId });

/** A commit message for what is staged in `root`. Rejects with `cancelled` when `imberCancel(requestId)` stopped it. */
export const imberCommitMessage = (root: string, hint: string, requestId: string) =>
  corvus<string>('imber_commit_message', { args: { root, hint, request_id: requestId } });

// ── What Imber did, and undoing it ───────────────────────────────────────────────────────────────

import type { ImberAction } from '$lib/types/corvus/imber';

/** Imber's actions in a repository, newest first, each reversible or with the reason it is not. */
export const imberActions = (root: string) => corvus<ImberAction[]>('imber_actions', { root });
/** Undo the newest reversible action (or the one named). Resolves with what was done; rejects with why not. */
export const imberUndo = (root: string, actionId?: string) => corvus<string>('imber_undo', { root, action_id: actionId ?? null });
