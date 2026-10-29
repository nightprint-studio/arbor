/**
 * Where "today", "this week" and "this month" begin, in the user's own time zone.
 *
 * The spending limits are sums over these windows, and only the machine the user is sitting at knows
 * what day it is for them — the backend has no calendar, and a day that started at the wrong hour
 * would reset a daily limit in the middle of the afternoon. So the panel works the boundaries out and
 * sends them with each request.
 *
 * A week starts on Monday; that is a choice, made once here.
 */

export interface PeriodStarts {
  /** Epoch milliseconds. */
  day: number;
  week: number;
  month: number;
}

export function periodStarts(now: Date = new Date()): PeriodStarts {
  const day = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  // getDay(): Sunday 0 … Saturday 6 → days since Monday.
  const sinceMonday = (day.getDay() + 6) % 7;
  const week = new Date(day.getFullYear(), day.getMonth(), day.getDate() - sinceMonday);
  const month = new Date(now.getFullYear(), now.getMonth(), 1);
  return { day: day.getTime(), week: week.getTime(), month: month.getTime() };
}
