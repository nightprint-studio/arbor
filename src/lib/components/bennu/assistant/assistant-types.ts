/**
 * The shapes the assistant panel draws — conversations, what is in them, and the choices around them.
 *
 * Nothing here talks to a model. The panel is a design: these types are what a real backend would
 * have to fill, so the screen can be judged now and wired later without being redrawn.
 */

import type { DiffHunkModel } from '$lib/components/shared/ui/TextDiff.svelte';

/** Who runs the edits: you approve each one, or the agent applies them as it goes. */
export type AssistantMode = 'manual' | 'auto';

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
}

export type ProposalStatus = 'pending' | 'applied' | 'rejected';

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

export type MessageBlock =
  | { type: 'text'; text: string }
  | { type: 'plan'; plan: PlanBlock }
  | { type: 'access'; request: AccessRequest; promptId?: string }
  | { type: 'confirm'; confirm: ConfirmRequest; promptId?: string }
  | { type: 'notice'; level: 'error' | 'warning' | 'info'; text: string }
  | { type: 'questions'; set: QuestionSet; promptId?: string }
  | { type: 'code'; language: string; code: string }
  | { type: 'tool'; call: ToolCall }
  | { type: 'proposal'; proposal: EditProposal };

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
}
