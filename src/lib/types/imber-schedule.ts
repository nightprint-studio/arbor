/**
 * Imber's schedules as the Schedules window sees them. Product-neutral: Corvus and Bennu serve the same
 * wire shape (Bennu without the `fetch` action, and always `manual`), so one window drives both.
 */

/** Mirrors `arbor_scheduler::calendar::Recurrence`. Weekdays are 0 = Monday … 6 = Sunday; times are local. */
export type ImberRecurrence =
  | { kind: 'once'; at_ms: number }
  | { kind: 'interval'; secs: number; anchor_ms: number }
  | { kind: 'daily'; hour: number; minute: number }
  | { kind: 'weekly'; days: number[]; hour: number; minute: number }
  | { kind: 'cron'; expr: string };

/** What an unattended run may do: `manual` is read-only; `auto` lets the product's writes run without asking. */
export type ImberScheduleMode = 'manual' | 'auto';

export type ImberScheduleAction =
  | { kind: 'awake'; conversation_id: string | null; prompt: string }
  | { kind: 'prompt'; prompt: string }
  | { kind: 'fetch'; remote: string | null };

export type ImberScheduleActionKind = ImberScheduleAction['kind'];

export interface ImberLastRun {
  at_ms: number;
  ok: boolean;
  detail: string;
  /** A run missed while the app was closed and dropped by the schedule's policy: neither a success nor a failure. */
  skipped?: boolean;
}

export interface ImberJob {
  id: string;
  name: string;
  enabled: boolean;
  recurrence: ImberRecurrence;
  /** What to do with a run missed while the app was closed. */
  missed: 'skip' | 'run_once';
  action: ImberScheduleAction;
  root: string;
  mode: ImberScheduleMode;
  created_ms: number;
  next_run_ms: number | null;
  last_run: ImberLastRun | null;
  runs: number;
}

/** What the Schedules window sends to save a job (`id` absent creates). */
export type ImberJobInput = Pick<ImberJob, 'name' | 'enabled' | 'recurrence' | 'missed' | 'action' | 'root' | 'mode'> & { id?: string };

/** One event of `arbor://<product>/imber-schedule`, fired outside any conversation. */
export interface ImberScheduleEvent {
  kind: 'started' | 'failed';
  id: string;
  name: string;
  root: string;
  action: ImberScheduleAction;
  detail?: string;
}
