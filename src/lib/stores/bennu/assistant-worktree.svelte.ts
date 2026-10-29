/**
 * The assistant's isolated copy, from the panel's side.
 *
 * Whether a conversation works on a copy is chosen once, from the composer's menu, and is fixed when the
 * conversation starts — like chat-only. Off by default, and a choice for the *next* conversation: it is
 * not remembered across restarts (a setting that silently applied to every future chat would be a surprise;
 * the menu shows what is selected). This store holds that choice, what the backend says about the open
 * conversation's copy, how making it is going, and the review / apply / discard flow.
 *
 * It does not import the assistant store (that one imports this): the panel, which has both, reloads the
 * conversation after an apply or a discard.
 */

import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  onWorktreeProgress, worktreeApply, worktreeDiff, worktreeDiscard, worktreeInfo, worktreeOptions,
  type WorktreeApplyResult, type WorktreeClash, type WorktreeDiff, type WorktreeInfo, type WorktreeKind, type WorktreeOptions,
  type WorktreeProgress,
} from '$lib/ipc/bennu/assistant-worktree';

function createWorktreeStore() {
  /** The kind chosen for the next new conversation; null = off. */
  let kind = $state<WorktreeKind | null>(null);
  let options = $state<WorktreeOptions | null>(null);
  let info = $state<WorktreeInfo | null>(null);
  let progress = $state<WorktreeProgress | null>(null);

  let diff = $state<WorktreeDiff | null>(null);
  let diffOpen = $state(false);
  let diffLoading = $state(false);
  let applying = $state(false);
  let discardAsked = $state(false);
  /** The files an apply was refused over, kept to be read in the review window. */
  let clashes = $state<WorktreeClash[]>([]);
  let held = $state<string | null>(null);
  let started = false;

  return {
    get kind() { return kind; },
    get options() { return options; },
    get info() { return info; },
    get progress() { return progress; },
    get diff() { return diff; },
    get diffOpen() { return diffOpen; },
    get diffLoading() { return diffLoading; },
    get applying() { return applying; },
    get discardAsked() { return discardAsked; },
    get clashes() { return clashes; },
    get held() { return held; },
    /** The open conversation works in a copy. */
    get active() { return info !== null; },

    /** Listen to the progress of making a copy. Once. */
    async start() {
      if (started) return;
      started = true;
      await onWorktreeProgress((p) => {
        progress = p.phase === 'ready' || p.phase === 'failed' ? null : p;
        if (p.phase === 'failed') toastStore.show(p.message ?? 'The isolated copy could not be opened.', 'error');
      }).catch(() => {});
    },

    /** Which copies can be offered for this project; asked when the menu is about to be shown. */
    async refreshOptions(root: string, git: boolean) {
      try { options = await worktreeOptions(root, git); } catch { options = null; }
    },

    setKind(next: WorktreeKind | null) { kind = next; },

    /** The kind to ask for with a send: only on a conversation's first message. */
    forSend(conv: { messages: unknown[]; worktree?: string }): WorktreeKind | null {
      if (conv.messages.length !== 0 || conv.worktree) return null;
      // Spent with the conversation it was chosen for: the next new one starts with the flag off again.
      const chosen = kind;
      kind = null;
      return chosen;
    },

    /** Read the copy of the open conversation (null when it has none). */
    async refreshInfo(conversationId: string | null) {
      if (!conversationId) { info = null; return; }
      try { info = await worktreeInfo(conversationId); } catch { info = null; }
    },

    async openDiff() {
      if (!info) return;
      diffOpen = true;
      diffLoading = true;
      clashes = [];
      held = null;
      try { diff = await worktreeDiff(info.conversationId); }
      catch (e) { toastStore.show(String(e), 'error'); diffOpen = false; }
      finally { diffLoading = false; }
    },
    closeDiff() { diffOpen = false; },

    /** Apply the copy. Returns whether it went in (the panel then reloads the conversation). */
    async apply(): Promise<boolean> {
      if (!info || applying) return false;
      applying = true;
      try {
        const r: WorktreeApplyResult = await worktreeApply(info.conversationId);
        if (r.applied) {
          toastStore.show(`Applied ${r.count} file${r.count === 1 ? '' : 's'} to the project.`, 'success');
          if (r.leftover) toastStore.show(`The copy could not be fully removed: ${r.leftover}`, 'warning');
          info = null;
          diffOpen = false;
          return true;
        }
        clashes = r.clashes ?? [];
        held = r.held ?? null;
        return false;
      } catch (e) {
        toastStore.show(String(e), 'error');
        return false;
      } finally {
        applying = false;
      }
    },

    askDiscard() { discardAsked = true; },
    cancelDiscard() { discardAsked = false; },

    /** Drop the copy without applying it. Returns whether it went. */
    async discard(): Promise<boolean> {
      discardAsked = false;
      if (!info) return false;
      try {
        const r = await worktreeDiscard(info.conversationId);
        if (r.leftover) toastStore.show(`The copy could not be fully removed: ${r.leftover}`, 'warning');
        info = null;
        diffOpen = false;
        return true;
      } catch (e) {
        toastStore.show(String(e), 'error');
        return false;
      }
    },
  };
}

export const bennuWorktreeStore = createWorktreeStore();
