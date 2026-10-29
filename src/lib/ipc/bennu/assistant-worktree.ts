/**
 * The assistant's isolated copy — served by `bennu-be` over the rpc seam.
 *
 * A conversation can work on a copy of the project so nothing it does touches the real files until the user
 * applies it. These are the calls the panel makes about that copy; making it happens on the conversation's
 * first message (`SendParams.worktree`), not here.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { bennu } from '../rpc';
import type { ProposedFile } from '$lib/components/bennu/assistant/assistant-types';

/** `copy`: text only, no code intelligence. `git`: a git worktree, indexed, every tool works. */
export type WorktreeKind = 'copy' | 'git';

export interface WorktreeOptions {
  git: { available: boolean; reason: string };
  copy: { available: boolean; reason: string; via: 'git' | 'copy' };
}

export interface WorktreeInfo {
  conversationId: string;
  mode: WorktreeKind;
  realRoot: string;
  copyRoot: string;
  viaGit: boolean;
  /** Short commit id the worktree was made at. */
  head: string | null;
  branch: string | null;
  createdMs: number;
}

export interface WorktreeFile extends Omit<ProposedFile, 'hunks'> {
  kind: 'added' | 'modified' | 'deleted';
  renamedFrom?: string | null;
  /** Null for a big file: counts only. */
  hunks: ProposedFile['hunks'] | null;
  /** Not text: no hunks. */
  binary?: boolean;
}

export interface WorktreeClash { rel: string; reason: string }

export interface WorktreeDiff { files: WorktreeFile[]; clashes: WorktreeClash[]; canApply: boolean }

export type WorktreeApplyResult =
  | { applied: true; count: number; leftover: string | null }
  | { applied: false; clashes?: WorktreeClash[]; held?: string };

/** What a copy could be for `root`, and why one is not offered. */
export const worktreeOptions = (root: string, git: boolean) => bennu<WorktreeOptions>('assistant_worktree_options', { root, git });

/** The copy a conversation works in, or null. */
export const worktreeInfo = (id: string) => bennu<WorktreeInfo | null>('assistant_worktree_info', { id });

/** The combined diff of the copy against the project. */
export const worktreeDiff = (id: string) => bennu<WorktreeDiff>('assistant_worktree_diff', { id });

/** Apply the copy to the project (all or nothing) and drop it. */
export const worktreeApply = (id: string) => bennu<WorktreeApplyResult>('assistant_worktree_apply', { id });

/** Drop the copy without applying it. */
export const worktreeDiscard = (id: string) => bennu<{ leftover: string | null }>('assistant_worktree_discard', { id });

/** How the making of a copy is going. `indexing` carries the index's own phase (`index_phase`, `state`). */
export interface WorktreeProgress {
  kind: 'worktree';
  conversation_id: string;
  phase: 'worktree' | 'copying' | 'snapshot' | 'indexing' | 'ready' | 'failed';
  done?: number;
  total?: number;
  index_phase?: string;
  state?: string;
  message?: string;
}

export function onWorktreeProgress(handler: (p: WorktreeProgress) => void): Promise<UnlistenFn> {
  return listen<{ kind: string }>('arbor://bennu/assistant', (e) => {
    if (e.payload.kind === 'worktree') handler(e.payload as WorktreeProgress);
  });
}
