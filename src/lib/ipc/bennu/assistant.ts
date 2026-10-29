/**
 * Assistant IPC — the provider key, which lives in its own credential vault.
 *
 * These are direct shell commands, not `bennu(...)` rpc calls, on purpose: the key goes from the
 * Settings page to the shell and into the OS store and never passes through a backend. The backend
 * asks for it only when it makes a request. The value is never read back — the page can only learn
 * whether one is stored.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { bennu } from '../rpc';
import type { AccessScope, AssistantAgent, AssistantEffort, AssistantMode, AssistantModel, Conversation, MessageBlock, ProposedFile } from '$lib/components/bennu/assistant/assistant-types';

/** The only provider so far; the vault keeps one key per provider id. */
export const DEFAULT_PROVIDER = 'anthropic';

/** Store or replace the key. A blank key clears it. */
export function storeAssistantKey(key: string, provider = DEFAULT_PROVIDER): Promise<void> {
  return invoke('assistant_store_key', { provider, key });
}

export function deleteAssistantKey(provider = DEFAULT_PROVIDER): Promise<void> {
  return invoke('assistant_delete_key', { provider });
}

export function hasAssistantKey(provider = DEFAULT_PROVIDER): Promise<boolean> {
  return invoke('assistant_has_key', { provider });
}

// ── The conversation — served by `bennu-be` over the rpc seam ────────────────────────────────────

/** What a send needs to know beyond the text — the choices made at the composer, and where the user is. */
export interface SendParams {
  conversation_id: string | null;
  text: string;
  /** Send the conversation again as it stands (a run that failed), with no new message. */
  retry?: boolean;
  root: string;
  file: string | null;
  selection: string | null;
  model: string;
  effort: AssistantEffort | null;
  max_tokens?: number;
  mode: AssistantMode;
  scope: AccessScope;
  allow_edits: boolean;
  allow_temp: boolean;
  extra_folders: string[];
  workspace_roots: string[];
  /** How many open projects one conversation may work across. */
  max_projects: number;
  max_context: number;
  /** Where today, this week and this month began — for the spending limits. */
  periods: { day: number; week: number; month: number };
  /** A chat with no tools. */
  chat: boolean;
  /** `shared` (builds queue with the others) or `separate` (own build target): first message only. */
  build_isolation?: 'shared' | 'separate';
  /** The isolated copy to work in (`copy` or `git`): honoured on a conversation's first message only. */
  worktree?: string | null;
  /** Read CLAUDE.md / AGENTS.md into the instructions. */
  instructions: boolean;
  /** Give the assistant the git tools. Off unless the user turned it on. */
  git: boolean;
  /** With git on: show the model made-up ids instead of authors. */
  hide_authors: boolean;
  /** Offer the step plan. */
  plan: boolean;
  /** Tool families the user has switched off. */
  families_off: string[];
  /** Let the assistant change a running Bevy game over BRP. Off unless the user turned it on. */
  bevy_write: boolean;
  debug_mutate?: boolean;
  /** Skills the user enabled (`<scope>/<name>`). */
  skills_on: string[];
  /** The default decision level and notes, and the conversation's own (empty: inherit). */
  decision_level?: string;
  decision_notes?: string;
  chat_decision_level?: string;
  chat_decision_notes?: string;
  /** Offer sub-agents: the default from the settings, and this conversation's own (`on`, `off`; empty follows the setting). */
  subagents_enabled?: boolean;
  chat_subagents?: string;
  /** Minutes an unused debug session of the assistant lives. */
  debug_idle_minutes?: number;
  dry_run: boolean;
}

/** The request a dry run builds: what would be sent, with the key masked. */
/** The provider's own account of one request: uncached input, output, and the cache reads and writes. */
export interface LastRequest { input: number; output: number; cacheRead: number; cacheWrite: number }

export interface RequestPreview {
  /** Estimated tokens by part, in the order the provider reads them. */
  parts?: { id: string; label: string; tokens: number }[];
  cache?: { longStanding: boolean; marks: string[] };
  provider: string;
  method: string;
  url: string;
  headers: [string, string][];
  body: string;
  breakdown: { system: number; tools: number; messages: number; total: number };
  key: string;
  /** Dollars: this request at full price, and a later one once the instructions and tools are cached.
   *  Absent for a model the price list does not know. */
  estimate: { first: number; next: number; uncached?: number } | null;
}

export type SendResult = { run_id: string; conversation_id: string } | { preview: RequestPreview };

export interface ConversationSummary {
  id: string;
  title: string;
  updatedAt: number;
  model: string;
  cost: number;
  context: Conversation['context'];
  chat?: boolean;
}

/** One window's spending against its limit, in euros. */
export interface SpendMeter {
  period: 'session' | 'day' | 'week' | 'month';
  spentEur: number;
  /** `null` when that limit is off. */
  limitEur: number | null;
  level: 'ok' | 'warn' | 'reached';
}

export interface SpendStatus {
  meters: SpendMeter[];
  /** The warning worth saying, if a limit is close. */
  warning: string | null;
  /** Euros per dollar, as configured. */
  rate: number;
}

/** An instruction file that would be read for a project. */
export interface InstructionFile {
  name: string;
  bytes: number;
  truncated: boolean;
}
export const getInstructions = (root: string) => bennu<InstructionFile[]>('assistant_instructions', { root });

/** A skill found on disk: a folder with a `SKILL.md`. */
export interface SkillInfo {
  /** `<scope>/<name>` — what settings store. */
  id: string;
  name: string;
  description: string;
  scope: 'user' | 'project';
  /** The folder it came from: `.bennu`, `.claude` or `bennu` (the user's own config folder). */
  source: string;
  /** The project a project skill belongs to. */
  project: string | null;
  path: string;
  /** The SKILL.md text after the frontmatter, cut at the limit. */
  body: string;
  truncated: boolean;
  bytes: number;
  /** A skill of the same name in the project hides this one from the model. */
  shadowed: boolean;
}

/** A skill folder that could not be used, and why. */
export interface InvalidSkill {
  path: string;
  reason: string;
}

export interface SkillListing {
  skills: SkillInfo[];
  invalid: InvalidSkill[];
  /** The most skills kept. */
  max: number;
}

/** The skills under these project roots and the user's folders. */
export const listSkills = (roots: string[]) => bennu<SkillListing>('assistant_skills_list', { roots });

export const getSpend = (periods: { day: number; week: number; month: number }, conversationId: string | null) =>
  bennu<SpendStatus>('assistant_spend', { periods, conversation_id: conversationId });

export const sendAssistantMessage = (p: SendParams) => bennu<SendResult>('assistant_send', { args: p });
export const cancelAssistantRun = (runId: string) => bennu<boolean>('assistant_cancel', { run_id: runId });
/** Skip the wait before a failed request is sent again. False when the run is not going. */
export const retryAssistantRunNow = (runId: string) => bennu<boolean>('assistant_retry_now', { run_id: runId });
/** Take back a file the agent opened for itself (Autonomous). False when the run is over. */
export const revokeAssistantGrant = (runId: string, grantId: string) =>
  bennu<boolean>('assistant_revoke_grant', { run_id: runId, grant_id: grantId });
export const replyToAssistant = (promptId: string, answer: unknown) =>
  bennu<boolean>('assistant_reply', { prompt_id: promptId, answer });
/** How deciding a proposal came out; `stale` names (by card label) the files that changed since it was made. */
export interface DecideResult {
  status: 'applied' | 'rejected' | 'outdated';
  files: string[];
  stale: string[];
}
export const decideProposal = (conversationId: string, proposalId: string, apply: boolean) =>
  bennu<DecideResult>('assistant_decide', { conversation_id: conversationId, proposal_id: proposalId, apply });

/** An outdated proposal made again on the current files: `pending` to approve, or `rejected` when nothing is left to apply. */
export interface RemakeResult {
  status: 'pending' | 'rejected';
  files: ProposedFile[];
  already_there: string[];
}
export const remakeProposal = (conversationId: string, proposalId: string) =>
  bennu<RemakeResult>('assistant_remake_proposal', { conversation_id: conversationId, proposal_id: proposalId });
/** The models on offer (the built-in price list overlaid with the user's own file) and where that file goes. */
export interface ModelList {
  models: AssistantModel[];
  file: string;
  error: string | null;
}
/** A saved screenshot (by the relative path the thread carries): its media type and its bytes in base64. */
export const getAssistantImage = (path: string) => bennu<{ media_type: string; data: string }>('assistant_image', { path });
export const listModels = () => bennu<ModelList>('assistant_models');
/** Create the user's price file from the built-in one, if there is none; answers with its path. */
export const createModelsTemplate = () => bennu<string>('assistant_models_template');
export const listConversations = () => bennu<ConversationSummary[]>('assistant_list');
export const loadConversation = (id: string) => bennu<Conversation>('assistant_load', { id });
export const deleteConversation = (id: string) => bennu<void>('assistant_delete', { id });
export const renameConversation = (id: string, title: string) => bennu<void>('assistant_rename', { id, title });

/** What a run reports while it goes. `message` carries the whole assistant message as it stands. */
export type AssistantEvent =
  | { kind: 'open_ui'; run_id: string; conversation_id: string; target: string; query?: string }
  | { kind: 'status'; run_id: string; conversation_id: string; phase: 'thinking' | 'writing' | 'tool' | 'compacting'; tool: string | null; iteration: number }
  | { kind: 'message'; run_id: string; conversation_id: string; message: AssistantWireMessage }
  | { kind: 'usage'; run_id: string; conversation_id: string; input: number; output: number; cache_read: number; cache_write: number }
  /** A request failed for a reason that can pass; it is sent again at `until` (ms since the epoch). */
  | { kind: 'retry'; run_id: string; conversation_id: string; attempt: number; max: number; wait_s: number; until: number; error_kind: string; message: string; detail: string }
  /** Data came in again after a wait: the status line goes. */
  | { kind: 'retry_end'; run_id: string; conversation_id: string }
  /** A sub-agent started, spent or ended: the list entry as it stands. */
  | { kind: 'agent'; run_id: string; conversation_id: string; agent: AssistantAgent }
  | {
      kind: 'done'; run_id: string; conversation_id: string;
      ended: 'done' | 'cancelled' | 'too_many_steps' | 'incomplete' | 'failed';
      error: string | null; error_kind?: string | null; save_error: string | null; cost: number; title: string; updated_at: number;
      context: Conversation['context'];
      /** What the provider billed for the run's last request. */
      request?: LastRequest | null;
      spend: SpendStatus;
    };

export interface AssistantWireMessage {
  id: string;
  role: 'assistant';
  at: number;
  model: string;
  blocks: MessageBlock[];
}

export function onAssistantEvent(handler: (e: AssistantEvent) => void): Promise<UnlistenFn> {
  return listen<AssistantEvent>('arbor://bennu/assistant', (e) => handler(e.payload));
}

// ── Sub-agents ───────────────────────────────────────────────────────────────────────────────────

/** An agent and its thread so far (`message` is one assistant message, absent when nothing was recorded). */
export interface AgentTranscript {
  agent: AssistantAgent;
  message: AssistantWireMessage | null;
}

/** The sub-agents of a conversation, oldest first. */
export const listAgents = (conversationId: string) => bennu<AssistantAgent[]>('assistant_agents', { conversation_id: conversationId });
/** One agent's thread, read-only: live while it runs. */
export const getAgentTranscript = (id: string) => bennu<AgentTranscript>('assistant_agent_transcript', { id });
/** Stop one agent; false when it was not running. */
export const stopAgent = (id: string) => bennu<boolean>('assistant_agent_stop', { id });
/** Stop every running agent of a conversation; answers how many. */
export const stopAllAgents = (conversationId: string) => bennu<number>('assistant_agents_stop_all', { conversation_id: conversationId });

/** Fold the early part of a conversation into a note the model writes. `instructions` says what it must keep. */
export const compactConversation = (args: { id: string; instructions?: string; model: string }) =>
  bennu<{ freed: number; summarized: boolean; cost: number; context: Conversation['context']; messages: Conversation['messages'] }>(
    'assistant_compact', { args },
  );

/** A debug session the assistant left open for its conversation (`arbor://bennu/assistant-debug`). */
export interface KeptDebugSession {
  conversation: string;
  title: string;
  root: string;
  label: string;
  sessionId: string;
  launched: boolean;
  state: 'kept' | 'ended';
  reason: string;
  /** Whole minutes until it is released if no debug tool is used. */
  endsInMin: number;
}

export const listKeptDebugSessions = () => bennu<KeptDebugSession[]>('assistant_debug_kept');
export const stopKeptDebugSession = (conversation: string) => bennu<string>('assistant_debug_stop', { conversation });

/** How an undo came out; `stale` names (by card label) the files changed since the apply, which stops the whole undo. */
export interface UndoResult {
  status: 'undone' | 'applied';
  files: string[];
  stale: string[];
}
/** Put back exactly the files an applied proposal wrote, as they were — all of them or none. */
export const undoProposal = (conversationId: string, proposalId: string) =>
  bennu<UndoResult>('assistant_undo_proposal', { conversation_id: conversationId, proposal_id: proposalId });
/** Whether an applied proposal can still be undone from memory (a restart or enough newer ones forget it). */
export const canUndoProposal = (proposalId: string) => bennu<boolean>('assistant_can_undo', { proposal_id: proposalId });

/** The debugged program of an idle conversation stopped (`arbor://bennu/assistant-stop`). */
export const ASSISTANT_STOP_EVENT = 'arbor://bennu/assistant-stop';
/** The assistant changed the debugged program (`arbor://bennu/assistant-debug-action`). */
export const ASSISTANT_DEBUG_ACTION_EVENT = 'arbor://bennu/assistant-debug-action';
/** May a stop that arrived while the conversation was idle start a run by itself? Counts one of the hour's automatic runs when yes. */
export const assistantStopAutoOk = (conversation: string, global: boolean, chatOverride: boolean | null) =>
  bennu<boolean>('assistant_stop_auto_ok', { conversation, global, chat_override: chatOverride });
