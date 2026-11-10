/**
 * Imber in Corvus: the panel's state, one conversation per repository, and the commit-message generator.
 *
 * A run streams `arbor://corvus/imber` events — the message as it stands (each push is the whole message,
 * so it replaces the last), usage, a retry notice, and `done`. An action Imber wants to take in manual
 * mode arrives as an `approval` block; `approve` / `reject` answer it and the run goes on.
 *
 * Conversations live in memory (the backend keeps the transcript for the session); persisting them is
 * not done yet. Everything the user chooses persistently — model, effort, limits, the key — is the shared
 * Imber setup, read through `imberState()`; only the mode is Corvus's own and is saved by the backend.
 */

import {
  hasAssistantKey, imberActions, imberCancel, imberCommitMessage, imberForget, imberReply, imberSend, imberSetMode, imberState, imberUndo, IMBER_TOPIC,
} from '$lib/ipc/corvus/imber';
import type { ImberAction, ImberEvent, ImberKind, ImberMessage, ImberMode, ImberState } from '$lib/types/corvus/imber';
import { pendingApproval, scheduledText, upsertMessage } from '$lib/components/corvus/imber/imber-messages';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { uiStore } from '../ui.svelte';
import { tabsStore } from './tabs.svelte';
import { corvusImberSchedules } from './imber-schedules.svelte';
import { graphStore } from './graph.svelte';
import { plansStore } from './plans.svelte';

interface Conversation {
  id: string | null;
  messages: ImberMessage[];
  kind: ImberKind;
  running: boolean;
  runId: string | null;
  /** `thinking`, or the tool being run. */
  status: string;
  retry: { attempt: number; max: number; waitSecs: number; detail: string } | null;
  costUsd: number;
  /** The last run failed and can be sent again. */
  canRetry: boolean;
}

const fresh = (): Conversation => ({ id: null, messages: [], kind: 'operate', running: false, runId: null, status: '', retry: null, costUsd: 0, canRetry: false });

function createImberStore() {
  let open = $state(false);
  let info = $state<ImberState | null>(null);
  let mode = $state<ImberMode>('manual');
  /** One conversation per repository path. */
  let byRoot = $state<Record<string, Conversation>>({});
  /** What is typed in the composer (kept here so the palette can schedule it as a prompt). */
  let draft = $state('');
  let commitRequest = $state<string | null>(null);
  /** What Imber did in the active repository, newest first (the undo list). */
  let actions = $state<ImberAction[]>([]);
  let actionsOpen = $state(false);
  /** Approvals whose plan has already been put in front of the user. */
  const shown = new Set<string>();

  const root = () => tabsStore.activeTab?.path ?? null;
  const conv = (r: string | null): Conversation | null => (r ? byRoot[r] ?? null : null);

  function ensure(r: string): Conversation {
    if (!byRoot[r]) byRoot[r] = fresh();
    return byRoot[r];
  }

  async function loadActions() {
    const r = root();
    if (!r) { actions = []; return; }
    try { actions = await imberActions(r); } catch { /* the backend is not up yet */ }
  }

  /** In manual mode Imber proposes running a plan: put the plan in front of the user, once, in the Plans window. */
  function reviewProposedPlan(c: Conversation) {
    if (mode !== 'manual') return;
    const pending = pendingApproval(c.messages);
    const plan = pending?.approval.tool === 'plan_run' ? pending.approval.plan : null;
    if (!pending || !plan || shown.has(pending.promptId)) return;
    shown.add(pending.promptId);
    plansStore.openModal(plan.id);
  }

  const ofEvent = (e: ImberEvent): Conversation | null => Object.values(byRoot).find((c) => c.id === e.conversation_id) ?? null;

  function onEvent(e: ImberEvent) {
    // A scheduled run may belong to a conversation this window never saw; the schedule list still wants to know it ended.
    if (e.kind === 'done') corvusImberSchedules.runEnded();
    const c = ofEvent(e);
    if (!c) return;
    switch (e.kind) {
      case 'message':
        c.messages = upsertMessage(c.messages, e.message);
        reviewProposedPlan(c);
        break;
      case 'status':
        c.status = e.status === 'tool' ? (e.tool ?? 'working') : 'thinking';
        break;
      case 'retry':
        c.retry = { attempt: e.attempt, max: e.max, waitSecs: e.wait_secs, detail: e.detail };
        break;
      case 'scheduled':
        // A schedule woke this conversation: show the saved prompt as what was said, and the run as going.
        c.messages = [...c.messages, { id: `s${Date.now()}`, role: 'user', at: Date.now(), blocks: [{ type: 'text', text: scheduledText(e.note, e.text) }] }];
        c.running = true;
        c.runId = e.run_id;
        c.status = 'thinking';
        break;
      case 'retry_end':
        c.retry = null;
        break;
      case 'done':
        c.running = false;
        c.runId = null;
        c.status = '';
        c.retry = null;
        c.costUsd = e.conversation_cost_usd;
        c.canRetry = e.ended === 'failed';
        // Imber may have changed the repository: the graph and the undo list catch up.
        graphStore.refresh();
        void loadActions();
        break;
    }
  }

  return {
    get open() { return open; },
    get info() { return info; },
    get mode() { return mode; },
    get current(): Conversation | null { return conv(root()); },
    get draft() { return draft; },
    set draft(v: string) { draft = v; },
    get messages(): ImberMessage[] { return conv(root())?.messages ?? []; },
    get running() { return conv(root())?.running ?? false; },
    /** What the run is parked on, if the user has to decide. */
    get pending() { return pendingApproval(conv(root())?.messages ?? []); },
    get generatingCommit() { return commitRequest !== null; },
    get actions() { return actions; },
    get actionsOpen() { return actionsOpen; },
    /** Something Imber did that can still be taken back. */
    get canUndo() { return actions.some((a) => a.undo !== null && a.undone_ms === null); },

    loadActions,
    /** For the gallery and tests: put actions in the list without a backend. */
    seedActions(list: ImberAction[]) { actions = list; },
    showActions() { actionsOpen = true; void loadActions(); },
    hideActions() { actionsOpen = false; },

    /** Undo Imber's last reversible action (or the one named). The result is toasted; a refusal says why. */
    async undo(id?: string): Promise<boolean> {
      const r = root();
      if (!r) return false;
      try {
        uiStore.showToast(await imberUndo(r, id), 'success');
        graphStore.refresh();
        return true;
      } catch (err) {
        uiStore.showToast(`Cannot undo: ${err}`, 'warning');
        return false;
      } finally {
        await loadActions();
      }
    },

    setupListeners(): () => void {
      return setupTauriListeners([{ event: IMBER_TOPIC, handler: (e: { payload: ImberEvent }) => onEvent(e.payload) }]);
    },

    /** The shared setup (model, effort, whether a key is stored) and Corvus's saved mode. */
    async loadState() {
      try {
        info = await imberState();
        mode = info.mode;
        // The vault is the truth about the key; the backend only reports what it could read.
        if (!info.hasKey) info = { ...info, hasKey: await hasAssistantKey() };
      } catch (err) {
        uiStore.showToast(`Imber: ${err}`, 'error');
      }
    },

    show() {
      // The panel lives beside a repository; say so rather than do nothing.
      if (!tabsStore.activeTab) { uiStore.showToast('Open a repository to use Imber', 'warning'); return; }
      open = true;
      void this.loadState();
      void loadActions();
    },
    hide() { open = false; },
    toggle() { if (open) this.hide(); else this.show(); },

    async setMode(next: ImberMode) {
      mode = next;
      try { mode = await imberSetMode(next); } catch (err) { uiStore.showToast(`Imber: ${err}`, 'error'); }
    },
    toggleMode() { return this.setMode(mode === 'manual' ? 'auto' : 'manual'); },

    setKind(kind: ImberKind) {
      const r = root();
      if (r) ensure(r).kind = kind;
    },

    async send(text: string): Promise<void> {
      const r = root();
      const body = text.trim();
      if (!r || !body) return;
      const c = ensure(r);
      if (c.running) return;
      c.messages = [...c.messages, { id: `u${Date.now()}`, role: 'user', at: Date.now(), blocks: [{ type: 'text', text: body }] }];
      await this.run(c, r, body, false);
    },

    /** Send the conversation again as it stands after a failed run. */
    async retry(): Promise<void> {
      const r = root();
      const c = conv(r);
      if (!r || !c || c.running || !c.canRetry) return;
      await this.run(c, r, '', true);
    },

    async run(c: Conversation, r: string, text: string, retry: boolean) {
      // The id is ours, minted before the send: the run's first events can beat the reply, and they find their conversation by it.
      c.id ??= `c-${Date.now().toString(16)}`;
      c.running = true;
      c.canRetry = false;
      c.status = 'thinking';
      try {
        const started = await imberSend({ conversation_id: c.id, text, root: r, chat: c.kind === 'chat', retry, mode });
        c.id = started.conversation_id;
        c.runId = started.run_id;
      } catch (err) {
        c.running = false;
        c.status = '';
        c.messages = [...c.messages, { id: `e${Date.now()}`, role: 'assistant', at: Date.now(), blocks: [{ type: 'notice', level: 'error', text: String(err) }] }];
      }
    },

    async cancel() {
      const c = conv(root());
      if (c?.runId) await imberCancel(c.runId).catch(() => {});
    },

    async answer(promptId: string, decision: 'approve' | 'reject') {
      await imberReply(promptId, decision).catch((err) => uiStore.showToast(`Imber: ${err}`, 'error'));
    },

    /** Start over on the active repository: the transcript is dropped on both sides. */
    async newConversation() {
      const r = root();
      const c = conv(r);
      if (!r || !c) return;
      if (c.running) await this.cancel();
      if (c.id) void imberForget(c.id).catch(() => {});
      const kind = c.kind;
      byRoot[r] = { ...fresh(), kind };
    },

    /**
     * A commit message for what is staged in the active repository. `null` when it was cancelled or failed
     * (a failure is toasted). Fills nothing and commits nothing: the caller puts the text in the box.
     */
    async generateCommitMessage(hint: string): Promise<string | null> {
      const r = root();
      if (!r || commitRequest) return null;
      const requestId = `cm${Date.now()}`;
      commitRequest = requestId;
      try {
        return await imberCommitMessage(r, hint, requestId);
      } catch (err) {
        if (String(err) !== 'cancelled') uiStore.showToast(`Imber: ${err}`, 'error');
        return null;
      } finally {
        commitRequest = null;
      }
    },

    cancelCommitMessage() {
      if (commitRequest) void imberCancel(commitRequest).catch(() => {});
    },
  };
}

export const imberStore = createImberStore();
