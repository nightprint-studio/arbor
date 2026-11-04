/** Schedules in words, and the form's working shape. Pure; the calendar rules themselves live in `arbor-scheduler`. */
import type { Recurrence, Schedule } from '$lib/types/hirundo';

export const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

const two = (n: number) => String(n).padStart(2, '0');

export function describe(r: Recurrence): string {
  switch (r.kind) {
    case 'once': return `once, ${new Date(r.at_ms).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}`;
    case 'interval': return r.secs % 3600 === 0 ? `every ${r.secs / 3600} h` : `every ${Math.round(r.secs / 60)} min`;
    case 'daily': return `every day at ${two(r.hour)}:${two(r.minute)}`;
    case 'weekly': {
      const days = [...r.days].sort().map((d) => WEEKDAYS[d]).join(', ');
      const weekdays = r.days.length === 5 && [0, 1, 2, 3, 4].every((d) => r.days.includes(d));
      return `${weekdays ? 'Mon–Fri' : days} at ${two(r.hour)}:${two(r.minute)}`;
    }
    case 'cron': return `cron ${r.expr}`;
  }
}

export function whenText(ms: number | null): string {
  return ms === null ? '—' : new Date(ms).toLocaleString([], { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
}

/** Local `datetime-local` text (`2026-11-04T09:30`) of an instant, and back. */
export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}T${two(d.getHours())}:${two(d.getMinutes())}`;
}

export function fromLocalInput(text: string): number | null {
  const t = new Date(text).getTime();
  return Number.isFinite(t) ? t : null;
}

export type Kind = Recurrence['kind'];

export function blankSchedule(target: string): Schedule {
  return {
    id: '', name: '', target, env_id: '', recurrence: { kind: 'daily', hour: 2, minute: 0 }, missed: 'skip', enabled: true,
    created_ms: 0, last_run_ms: null, last_result: '', last_summary: '',
  };
}

/** The recurrence the form builds when the user switches kind, starting from sensible values. */
export function recurrenceOf(kind: Kind): Recurrence {
  const now = Date.now();
  switch (kind) {
    case 'once': return { kind, at_ms: now + 3600_000 };
    case 'interval': return { kind, secs: 3600, anchor_ms: now };
    case 'daily': return { kind, hour: 2, minute: 0 };
    case 'weekly': return { kind, days: [0, 1, 2, 3, 4], hour: 8, minute: 0 };
    case 'cron': return { kind, expr: '0 0 2 * * *' };
  }
}
