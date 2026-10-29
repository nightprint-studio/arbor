/**
 * The shapes the assistant panel draws — conversations, what is in them, and the choices around them.
 *
 * Nothing here talks to a model. The panel is a design: these types are what a real backend would
 * have to fill, so the screen can be judged now and wired later without being redrawn.
 */

import type { DiffHunkModel } from '$lib/components/shared/ui/TextDiff.svelte';

/**
 * How much the agent decides for itself. `manual`: every edit is a proposal. `accept_edits`: edits are applied
 * at once inside what you allowed. `autonomous`: that, and it may open the files its own build reports an error in.
 */
export type AssistantMode = 'manual' | 'accept_edits' | 'autonomous';

export interface AssistantModel {
  id: string;
  name: string;
  /** One line on what it is for. */
  tagline: string;
  /** The largest context window it accepts, in tokens. */
  contextMax: number;
  /** The effort levels it accepts; empty when it has no such setting. */
  efforts: AssistantEffort[];
  /** Dollars per million tokens. */
  inputPrice: number;
  outputPrice: number;
}

/** What a tool call did, which is what picks its icon. `refactor` is one of Bennu's own — rename, add
 *  parameter, change case — planned through the refactoring engine rather than typed out as text. */
export type ToolKind = 'read' | 'search' | 'edit' | 'validate' | 'refactor';

export interface ToolCall {
  id: string;
  kind: ToolKind;
  /** `Read BandoService.java`. */
  label: string;
  /** What came back, in a few words: `412 lines`, `3 matches`. */
  detail: string;
  /** False when it was refused or failed; absent in conversations saved before this was recorded. */
  ok?: boolean;
}

export interface ProposedFile {
  path: string;
  added: number;
  removed: number;
  /** What changes, as the diff viewer draws it — shown when you review the proposal. */
  hunks: DiffHunkModel[];
  /** The file is no longer what this was proposed against — it changed since. */
  stale?: boolean;
  /** Another undecided proposal edits this file too; applying either makes the other outdated. */
  shared?: boolean;
}

/** `outdated`: refused because files changed since the proposal; it is kept, to be re-made or dropped. */
export type ProposalStatus = 'pending' | 'applied' | 'rejected' | 'outdated' | 'undone';

/** Changes the agent wants to make, shown before they are written. */
export interface EditProposal {
  id: string;
  files: ProposedFile[];
  status: ProposalStatus;
}

/**
 * How much effort the model puts into an answer — the lever between speed and thoroughness. Low is
 * quick and uses few tools; max thinks the longest and costs the most. Not every model has the lever:
 * `AssistantModel.efforts` lists the levels it accepts, and an empty list means it has none.
 */
export type AssistantEffort = 'low' | 'medium' | 'high' | 'xhigh' | 'max';

/** What the assistant is allowed to look at and change. Narrowest first. */
export type AccessScope = 'file' | 'project' | 'workspace' | 'disk';

export interface QuestionOption {
  label: string;
  /** What choosing it means, in a line. */
  description?: string;
}

/** One question: a short header, the question, and a few options to pick (one, or several). */
export interface AssistantQuestion {
  id: string;
  /** A word or two naming the decision: `Currency`, `Scope`. */
  header: string;
  text: string;
  options: QuestionOption[];
  multi: boolean;
}

/**
 * Questions the assistant puts to you instead of guessing — one or several, asked together. You move
 * between them, forwards and back, and answer them all at once; "Other" is always available, because
 * the options are the assistant's guess at what you might say and not the limit of it. Once answered
 * the set stays in the thread as what was decided.
 */
export interface QuestionSet {
  id: string;
  questions: AssistantQuestion[];
  /** The answer to each question, by its id — the labels chosen, or the text typed under "Other".
   *  Absent until the set is submitted. */
  answers?: Record<string, string[]>;
}

/** What you decided about a request to go beyond where the assistant may reach. */
export type AccessAnswer = 'denied' | 'read' | 'write';

/**
 * The assistant asking to reach somewhere it may not: one folder, why, and one prompt that decides both
 * whether and how far — read only, or read and write. Granted for the session, for that folder.
 */
export interface AccessRequest {
  id: string;
  /** The folder it wants. */
  folder: string;
  /** The file that made it ask, if one did. */
  file?: string;
  reason: string;
  /** Absent until you answer. */
  answer?: AccessAnswer;
}

/** Whether a file the agent opened for itself is still open. `expired`: the run ended, and the access with it. */
export type GrantState = 'active' | 'revoked' | 'expired';

/** A file Autonomous mode opened without asking, because the build reported an error in it. */
export interface AccessGrant {
  id: string;
  /** The file, as the thread names it. */
  file: string;
  /** `Access granted to A.java because the build reports an error there`. */
  reason: string;
  state: GrantState;
}

/** A yes/no the assistant needs before it runs something that changes things or runs project code. */
export interface ConfirmRequest {
  id: string;
  title: string;
  detail: string;
  danger: boolean;
  answer?: 'yes' | 'no';
}

export type PlanStatus = 'pending' | 'in_progress' | 'completed';
export interface PlanStep {
  text: string;
  status: PlanStatus;
}
/** The plan the assistant keeps while it works on something with several steps. */
export interface PlanBlock {
  id: string;
  steps: PlanStep[];
}

/** What a run did with live processes — the card at the end of its message. */
export interface LiveSummary {
  started: string[];
  stopped: string[];
  still_running: string[];
  request_count: number;
  requests_ok: number;
  requests: { method: string; url: string; status: number | null; corr: string; ms: number | null }[];
  scenarios: { name: string; passed: boolean; steps: number; failed: number }[];
  assertions: { passed: number; failed: number };
  files: string[];
  /** Changes made to a running Bevy game. It has no undo, so every one is listed; absent in older conversations. */
  bevy_change_count?: number;
  bevy_changes?: string[];
  /** What the run cost, in dollars; absent when the model's prices are unknown. */
  cost: number | null;
}

/**
 * A diagram the assistant drew with `show_diagram`: Mermaid source, checked by the panel before the model
 * is told it worked. `checking` is the moment the panel parses it; once accepted it stays `ok`.
 */
export interface DiagramBlock {
  id: string;
  title?: string;
  source: string;
  status?: 'checking' | 'ok';
}

/** Where a sub-agent is. `stopped`: by you, or because the run that started it ended. `failed`: an error, or its steps or money ran out. */
export type AgentStatus = 'running' | 'done' | 'stopped' | 'failed';

/** The card a sub-agent leaves in the thread of the conversation that started it. */
export interface AgentCard {
  id: string;
  title: string;
  status: AgentStatus;
  /** `read` or `read+edit`. */
  tools: 'read' | 'read+edit';
  model: string;
  /** Dollars. */
  cost: number;
  startedAt: number;
  endedAt: number | null;
  /** What it was asked, cut to a line or two. Absent in cards saved before the card carried it. */
  task?: string;
  /** The first part of what it answered. */
  summary: string;
  /** Why it failed, when it did (a lost connection, a rejected key…). */
  error?: string | null;
  /** Set while it waits to send a failed request again. */
  waiting?: string | null;
}

/** A sub-agent as the Agents list shows it. */
export interface AssistantAgent extends Omit<AgentCard, 'summary'> {
  conversationId: string;
  runId: string;
  task: string;
  summary: string | null;
  error: string | null;
}

export type MessageBlock =
  | { type: 'text'; text: string }
  | { type: 'diagram'; diagram: DiagramBlock; promptId?: string }
  | { type: 'plan'; plan: PlanBlock }
  | { type: 'access'; request: AccessRequest; promptId?: string }
  | { type: 'grant'; grant: AccessGrant }
  | { type: 'confirm'; confirm: ConfirmRequest; promptId?: string }
  | { type: 'notice'; level: 'error' | 'warning' | 'info'; text: string; detail?: string; kind?: string; retryable?: boolean; canRetry?: boolean }
  | { type: 'questions'; set: QuestionSet; promptId?: string }
  | { type: 'code'; language: string; code: string }
  | { type: 'tool'; call: ToolCall }
  | { type: 'proposal'; proposal: EditProposal }
  | { type: 'live_summary'; summary: LiveSummary }
  | { type: 'screenshot'; shot: ScreenshotShot }
  | { type: 'agent'; agent: AgentCard };

/** A screenshot the assistant took of a page on this machine. The thread carries a path, never the bytes. */
export interface ScreenshotShot {
  id: string;
  /** `<conversation>/<file>` under the assistant's data folder; fetched with `getAssistantImage`. */
  path: string;
  media_type: string;
  width: number;
  height: number;
  /** `http://localhost:3000/ · 1280×800`. */
  caption: string;
}

export interface AssistantMessage {
  id: string;
  role: 'user' | 'assistant';
  blocks: MessageBlock[];
  /** Epoch milliseconds. */
  at: number;
  /** The model that wrote it, for an assistant message. */
  model?: string;
}

/** Where the tokens of the open conversation go. An estimate — see `assistant-format.ts`. */
export interface ContextBreakdown {
  /** Instructions and tool definitions: the part every request carries. */
  system: number;
  /** Files and search results the agent read into the conversation. */
  project: number;
  /** What you and the assistant said. */
  conversation: number;
  /** Parts of `system`, told apart (absent in a conversation saved before they were counted). */
  tools?: number;
  instructions?: number;
  skills?: number;
}

export interface Conversation {
  id: string;
  title: string;
  /** Epoch milliseconds of the last message. */
  updatedAt: number;
  model: string;
  messages: AssistantMessage[];
  context: ContextBreakdown;
  /** What this conversation has cost so far, in dollars. */
  cost: number;
  /** A chat: no tools, no access to files. Absent in conversations saved before chats existed. */
  chat?: boolean;
  /** The kind of isolated copy this conversation works in, until it is applied or discarded. */
  worktree?: string;
  /** `separate` when this conversation compiles in a target of its own; absent means the shared, queued one. */
  buildIsolation?: string;
  /** This conversation's own decision level; absent inherits the setting. */
  decisionLevel?: string;
  /** This conversation's own words about decisions, added to the global ones. */
  decisionNotes?: string;
  /** This conversation's own answer to "start a run when the debugged program stops": `on`, `off`; absent follows the setting. Kept while the panel is open. */
  autoContinue?: string;
  /** This conversation's own sub-agents switch: `on`, `off`; absent follows the setting. */
  subagents?: string;
}
