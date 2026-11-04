/** Pure helpers for the Schedules window: a recurrence in words, times relative to now, and the form ⇄ recurrence. */

import type { ImberJob, ImberJobInput, ImberRecurrence, ImberScheduleAction } from '$lib/types/corvus/imber';

export const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const;
const pad = (n: number) => String(n).padStart(2, '0');
export const clock = (hour: number, minute: number) => `${pad(hour)}:${pad(minute)}`;

/** `2026-11-05 09:00`, local. */
export function formatLocal(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${clock(d.getHours(), d.getMinutes())}`;
}

/** `2026-11-05 09:00` or `2026-11-05T09:00`, read as local time. `null` when it is not a real date. */
export function parseLocal(text: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!m) return null;
  const [y, mo, d, h, mi] = m.slice(1).map(Number);
  const date = new Date(y, mo - 1, d, h, mi);
  // `new Date` rolls 31 Feb into March: a date that moved was never real.
  const same = date.getFullYear() === y && date.getMonth() === mo - 1 && date.getDate() === d && date.getHours() === h && date.getMinutes() === mi;
  return same ? date.getTime() : null;
}

export function parseClock(text: string): { hour: number; minute: number } | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!m) return null;
  const [hour, minute] = [Number(m[1]), Number(m[2])];
  return hour <= 23 && minute <= 59 ? { hour, minute } : null;
}

export function describeRecurrence(r: ImberRecurrence): string {
  switch (r.kind) {
    case 'once': return `Once, ${formatLocal(r.at_ms)}`;
    case 'interval': {
      const m = Math.round(r.secs / 60);
      return m % 60 === 0 ? `Every ${m / 60 === 1 ? 'hour' : `${m / 60} hours`}` : `Every ${m === 1 ? 'minute' : `${m} minutes`}`;
    }
    case 'daily': return `Every day at ${clock(r.hour, r.minute)}`;
    case 'weekly': {
      const days = [...r.days].sort((a, b) => a - b);
      const label = days.length === 7 ? 'Every day' : days.join() === '0,1,2,3,4' ? 'Weekdays' : days.map((d) => WEEKDAYS[d]).join(', ');
      return `${label} at ${clock(r.hour, r.minute)}`;
    }
    case 'cron': return `Cron ${r.expr}`;
  }
}

/** `in 3 h`, `in 12 min`, `5 min ago`, `just now`. */
export function relativeTime(ms: number, now: number): string {
  const diff = ms - now;
  const abs = Math.abs(diff);
  if (abs < 45_000) return 'just now';
  const mins = Math.round(abs / 60_000);
  const text = mins < 60 ? `${mins} min` : mins < 48 * 60 ? `${Math.round(mins / 60)} h` : `${Math.round(mins / 1440)} d`;
  return diff > 0 ? `in ${text}` : `${text} ago`;
}

// ── The form ─────────────────────────────────────────────────────────────────────────────────────

export type WhenKind = ImberRecurrence['kind'];
export type ActionKind = ImberScheduleAction['kind'];

export interface ScheduleForm {
  name: string;
  action: ActionKind;
  prompt: string;
  remote: string;
  when: WhenKind;
  /** `once`: `YYYY-MM-DD HH:MM`. */
  at: string;
  /** `daily`, `weekly`: `HH:MM`. */
  time: string;
  days: boolean[];
  minutes: number;
  cron: string;
  missed: 'skip' | 'run_once';
  mode: 'manual' | 'auto';
  enabled: boolean;
}

/** A new schedule: weekdays at 09:00, read-only, a missed run skipped. */
export function blankForm(now: number): ScheduleForm {
  const soon = new Date(now + 3_600_000);
  return {
    name: '', action: 'awake', prompt: '', remote: '', when: 'weekly',
    at: formatLocal(new Date(soon.getFullYear(), soon.getMonth(), soon.getDate(), soon.getHours() + 1, 0).getTime()),
    time: '09:00', days: [true, true, true, true, true, false, false], minutes: 60, cron: '0 0 9 * * 1-5',
    missed: 'skip', mode: 'manual', enabled: true,
  };
}

export function formFromJob(job: ImberJob): ScheduleForm {
  const f = blankForm(Date.now());
  const r = job.recurrence;
  Object.assign(f, { name: job.name, missed: job.missed, mode: job.mode, enabled: job.enabled, when: r.kind, action: job.action.kind });
  if (job.action.kind !== 'fetch') f.prompt = job.action.prompt;
  else f.remote = job.action.remote ?? '';
  if (r.kind === 'once') f.at = formatLocal(r.at_ms);
  if (r.kind === 'daily') f.time = clock(r.hour, r.minute);
  if (r.kind === 'weekly') { f.time = clock(r.hour, r.minute); f.days = WEEKDAYS.map((_, i) => r.days.includes(i)); }
  if (r.kind === 'interval') f.minutes = Math.round(r.secs / 60);
  if (r.kind === 'cron') f.cron = r.expr;
  return f;
}

/** The recurrence the form describes, or why it does not. */
export function recurrenceFromForm(f: ScheduleForm): { ok: ImberRecurrence } | { error: string } {
  switch (f.when) {
    case 'once': {
      const at = parseLocal(f.at);
      return at === null ? { error: 'Write the date and time as 2026-11-05 09:00.' } : { ok: { kind: 'once', at_ms: at } };
    }
    case 'interval':
      return !Number.isFinite(f.minutes) || f.minutes < 1
        ? { error: 'The interval must be at least one minute.' }
        : { ok: { kind: 'interval', secs: Math.round(f.minutes) * 60, anchor_ms: 0 } };
    case 'daily': {
      const t = parseClock(f.time);
      return t ? { ok: { kind: 'daily', ...t } } : { error: 'Write the time as 09:30.' };
    }
    case 'weekly': {
      const t = parseClock(f.time);
      const days = f.days.flatMap((on, i) => (on ? [i] : []));
      if (!t) return { error: 'Write the time as 09:30.' };
      return days.length ? { ok: { kind: 'weekly', days, ...t } } : { error: 'Pick at least one weekday.' };
    }
    case 'cron':
      return f.cron.trim().split(/\s+/).length === 6
        ? { ok: { kind: 'cron', expr: f.cron.trim() } }
        : { error: 'A cron expression has 6 fields: sec min hour day month weekday.' };
  }
}

/**
 * The job to save, or the first thing wrong with the form. `conversationId` is the conversation a new *awake* wakes;
 * `keepConversation` is the one an edited job already wakes (it wins).
 */
export function inputFromForm(f: ScheduleForm, root: string, conversationId: string | null, id?: string, keepConversation?: string | null): ImberJobInput | { error: string } {
  if (!f.name.trim()) return { error: 'Give the schedule a name.' };
  if (f.action !== 'fetch' && !f.prompt.trim()) return { error: 'Write the prompt Imber will be given.' };
  const when = recurrenceFromForm(f);
  if ('error' in when) return when;
  const action: ImberScheduleAction =
    f.action === 'awake' ? { kind: 'awake', conversation_id: keepConversation ?? conversationId, prompt: f.prompt.trim() }
    : f.action === 'prompt' ? { kind: 'prompt', prompt: f.prompt.trim() }
    : { kind: 'fetch', remote: f.remote.trim() || null };
  return { id, name: f.name.trim(), enabled: f.enabled, recurrence: when.ok, missed: f.missed, action, root, mode: f.mode };
}

/** How the last run ended, in a few words. */
export function lastRunLine(job: ImberJob, now: number): string {
  if (!job.last_run) return 'Never run';
  return `${job.last_run.ok ? 'Done' : 'Failed'} ${relativeTime(job.last_run.at_ms, now)}`;
}

/** What the row says about the future. */
export function nextRunLine(job: ImberJob, now: number): string {
  if (job.next_run_ms !== null) return `Next ${formatLocal(job.next_run_ms)} (${relativeTime(job.next_run_ms, now)})`;
  if (job.recurrence.kind === 'once' && job.runs > 0) return 'Ran once';
  return job.enabled ? 'No next run' : 'Off';
}
