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
import type { AccessScope, AssistantEffort, AssistantModel, Conversation, MessageBlock } from '$lib/components/bennu/assistant/assistant-types';

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
  root: string;
  file: string | null;
  selection: string | null;
  model: string;
  effort: AssistantEffort | null;
  max_tokens?: number;
  auto: boolean;
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
  dry_run: boolean;
}

/** The request a dry run builds: what would be sent, with the key masked. */
export interface RequestPreview {
  provider: string;
  method: string;
  url: string;
  headers: [string, string][];
  body: string;
  breakdown: { system: number; tools: number; messages: number; total: number };
  key: string;
  /** Dollars: this request at full price, and a later one once the instructions and tools are cached.
   *  Absent for a model the price list does not know. */
  estimate: { first: number; next: number } | null;
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

export const getSpend = (periods: { day: number; week: number; month: number }, conversationId: string | null) =>
  bennu<SpendStatus>('assistant_spend', { periods, conversation_id: conversationId });

export const sendAssistantMessage = (p: SendParams) => bennu<SendResult>('assistant_send', { args: p });
export const cancelAssistantRun = (runId: string) => bennu<boolean>('assistant_cancel', { run_id: runId });
export const replyToAssistant = (promptId: string, answer: unknown) =>
  bennu<boolean>('assistant_reply', { prompt_id: promptId, answer });
export const decideProposal = (conversationId: string, proposalId: string, apply: boolean) =>
  bennu<string[]>('assistant_decide', { conversation_id: conversationId, proposal_id: proposalId, apply });
/** The models on offer (the built-in price list overlaid with the user's own file) and where that file goes. */
export interface ModelList {
  models: AssistantModel[];
  file: string;
  error: string | null;
}
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
  | {
      kind: 'done'; run_id: string; conversation_id: string;
      ended: 'done' | 'cancelled' | 'too_many_steps' | 'incomplete';
      error: string | null; save_error: string | null; cost: number; title: string; updated_at: number;
      context: Conversation['context'];
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

/** Fold the early part of a conversation into a note the model writes. `instructions` says what it must keep. */
export const compactConversation = (args: { id: string; instructions?: string; model: string }) =>
  bennu<{ freed: number; summarized: boolean; cost: number; context: Conversation['context']; messages: Conversation['messages'] }>(
    'assistant_compact', { args },
  );
