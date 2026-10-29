/**
 * The assistant's sub-agents, from the panel's side.
 *
 * The backend keeps the agents in memory and, for the life of the conversation, on disk (so they survive a restart);
 * it reports each one as an `agent` event when it starts, spends or ends, and serves the list and a
 * read-only transcript on request. This store holds the list per conversation, the transcript open in the
 * viewer, and the verbs: open, stop one, stop all, delete one.
 *
 * It does not import the assistant store (the panel, which has both, passes the open conversation in).
 */

import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  deleteAgent, getAgentTranscript, listAgents, onAssistantEvent, stopAgent, stopAllAgents, type AgentTranscript,
} from '$lib/ipc/bennu/assistant';
import { runningCount, upsertAgent } from '$lib/components/bennu/assistant/assistant-agents';
import type { AssistantAgent } from '$lib/components/bennu/assistant/assistant-types';

function createAgentsStore() {
  let byConversation = $state<Record<string, AssistantAgent[]>>({});
  /** The agent whose thread is open in the viewer. */
  let openId = $state<string | null>(null);
  let transcript = $state<AgentTranscript | null>(null);
  let transcriptError = $state<string | null>(null);
  let started = false;

  async function refresh(conversationId: string | null) {
    if (!conversationId) return;
    try {
      const list = await listAgents(conversationId);
      byConversation = { ...byConversation, [conversationId]: list };
    } catch {
      // The backend is not up yet, or the conversation is new: there are no agents to list.
    }
  }

  async function refreshOpen() {
    if (!openId) return;
    try {
      transcript = await getAgentTranscript(openId);
      transcriptError = null;
    } catch (e) {
      transcriptError = String(e);
    }
  }

  return {
    /** Listen for agent events. Once. */
    async start() {
      if (started) return;
      started = true;
      await onAssistantEvent((e) => {
        if (e.kind === 'agent') {
          byConversation = { ...byConversation, [e.conversation_id]: upsertAgent(byConversation[e.conversation_id] ?? [], e.agent) };
          if (openId === e.agent.id) void refreshOpen();
        } else if (e.kind === 'done') {
          // A run that ended has settled its agents: take the backend's word for the list.
          void refresh(e.conversation_id);
        }
      });
    },

    list(conversationId: string | null): AssistantAgent[] {
      return conversationId ? byConversation[conversationId] ?? [] : [];
    },
    running(conversationId: string | null): number {
      return runningCount(this.list(conversationId));
    },
    refresh,

    get openId() { return openId; },
    get transcript() { return transcript; },
    get transcriptError() { return transcriptError; },
    async open(id: string) {
      openId = id;
      transcript = null;
      transcriptError = null;
      await refreshOpen();
    },
    refreshOpen,
    /** Open the thread of the agent a conversation started last (a running one first). */
    async openLatest(conversationId: string | null) {
      const list = this.list(conversationId);
      const target = [...list].reverse().find((a) => a.status === 'running') ?? list.at(-1);
      if (target) await this.open(target.id);
    },
    close() {
      openId = null;
      transcript = null;
    },

    async stop(id: string) {
      const ok = await stopAgent(id).catch(() => false);
      if (!ok) toastStore.show('That agent is no longer running.', 'info');
    },
    /** Delete a finished agent: it leaves the list and what was kept of its thread is removed. Its card in the
     *  conversation stays, and says its thread is no longer kept. */
    async remove(id: string, conversationId: string | null) {
      try {
        await deleteAgent(id);
        if (openId === id) this.close();
        await refresh(conversationId);
      } catch (e) {
        toastStore.show(String(e), 'error');
      }
    },
    async stopAll(conversationId: string | null) {
      if (!conversationId) return;
      const n = await stopAllAgents(conversationId).catch(() => 0);
      if (n === 0) toastStore.show('No agent is running.', 'info');
    },
  };
}

export const bennuAgentsStore = createAgentsStore();
