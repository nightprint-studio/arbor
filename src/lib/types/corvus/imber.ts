/** What Imber streams to the Corvus panel: the message as it stands, in blocks, in the order a person reads them. */
import type { Plan } from './plan';

export type ImberMode = 'manual' | 'auto';

/** `operate` has the git tools; `chat` has none and only talks. */
export type ImberKind = 'operate' | 'chat';

export interface ImberState {
  hasKey: boolean;
  model: string;
  modelName: string;
  effort: string;
  mode: ImberMode;
  /** The per-conversation spending limit in euros; 0 when off. */
  sessionLimitEur: number;
}

export interface ImberToolCall {
  id: string;
  kind: 'read' | 'search' | 'edit' | 'validate' | 'refactor';
  label: string;
  detail: string;
  ok: boolean;
}

/** An action Imber wants to take, shown with the exact command until the user decides. */
export interface ImberApproval {
  id: string;
  tool: string;
  title: string;
  /** The git commands the action amounts to, as they would be typed. */
  commands: string[];
  effect: string;
  risk: 'write' | 'destructive';
  answer: 'approve' | 'reject' | null;
  /** What the action did once it ran (absent while undecided or declined). */
  result: { ok: boolean; detail: string } | null;
  /** For a plan tool: the plan it is about, in full (the run is proposed with its steps in view). */
  plan?: Plan | null;
}

export type ImberBlock =
  | { type: 'text'; text: string }
  | { type: 'tool'; call: ImberToolCall }
  | { type: 'approval'; approval: ImberApproval; promptId?: string }
  /** A plan Imber built or ran: a snapshot; the panel shows the live plan when it has it. */
  | { type: 'plan'; plan: Plan }
  | { type: 'notice'; level: 'info' | 'warning' | 'error'; text: string; detail?: string; canRetry?: boolean };

export interface ImberMessage {
  id: string;
  role: 'user' | 'assistant';
  at: number;
  model?: string;
  blocks: ImberBlock[];
}

/** One event of `arbor://corvus/imber`. */
export type ImberEvent = { run_id: string; conversation_id: string } & (
  | { kind: 'message'; message: ImberMessage }
  | { kind: 'status'; status: 'thinking' | 'tool'; tool?: string }
  | { kind: 'usage'; input: number; output: number; cache_read: number; cache_write: number }
  | { kind: 'retry'; attempt: number; max: number; wait_secs: number; detail: string }
  | { kind: 'retry_end' }
  /** A schedule woke this conversation: `text` is the saved prompt it was given. */
  | { kind: 'scheduled'; note: string; text: string }
  | { kind: 'done'; ended: 'done' | 'cancelled' | 'too_many_steps' | 'incomplete' | 'failed'; cost_usd: number; conversation_cost_usd: number; priced: boolean }
);

// ── Schedules ────────────────────────────────────────────────────────────────────────────────────

/** Mirrors `arbor_scheduler::calendar::Recurrence`. Weekdays are 0 = Monday … 6 = Sunday; times are local. */
export type ImberRecurrence =
  | { kind: 'once'; at_ms: number }
  | { kind: 'interval'; secs: number; anchor_ms: number }
  | { kind: 'daily'; hour: number; minute: number }
  | { kind: 'weekly'; days: number[]; hour: number; minute: number }
  | { kind: 'cron'; expr: string };

export type ImberScheduleAction =
  | { kind: 'awake'; conversation_id: string | null; prompt: string }
  | { kind: 'prompt'; prompt: string }
  | { kind: 'fetch'; remote: string | null };

export interface ImberLastRun { at_ms: number; ok: boolean; detail: string }

export interface ImberJob {
  id: string;
  name: string;
  enabled: boolean;
  recurrence: ImberRecurrence;
  /** What to do with a run missed while Corvus was closed. */
  missed: 'skip' | 'run_once';
  action: ImberScheduleAction;
  root: string;
  /** What it may do with nobody to approve: `manual` is read-only. Destructive actions never run unattended. */
  mode: ImberMode;
  created_ms: number;
  next_run_ms: number | null;
  last_run: ImberLastRun | null;
  runs: number;
}

/** What the Schedules window sends to save a job (`id` absent creates). */
export type ImberJobInput = Pick<ImberJob, 'name' | 'enabled' | 'recurrence' | 'missed' | 'action' | 'root' | 'mode'> & { id?: string };

// ── What Imber did, and undoing it ───────────────────────────────────────────────────────────────

/** How an action is taken back (mirrors `undo::UndoOp`; the window only needs to know *whether*). */
export type ImberUndoOp = { op: string; [k: string]: unknown };

export interface ImberAction {
  id: string;
  at_ms: number;
  root: string;
  tool: string;
  /** One line: `Committed “fix: totals”`. */
  summary: string;
  undo: ImberUndoOp | null;
  /** Why it cannot be undone (a push, a merge request, a plan run left the machine). */
  irreversible: string | null;
  undone_ms: number | null;
}
