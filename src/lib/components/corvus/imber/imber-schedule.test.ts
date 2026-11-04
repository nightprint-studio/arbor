import { describe, expect, it } from 'vitest';
import type { ImberJob } from '$lib/types/corvus/imber';
import {
  blankForm, describeRecurrence, formFromJob, formatLocal, inputFromForm, lastRunLine, nextRunLine, parseClock, parseLocal, recurrenceFromForm, relativeTime,
} from './imber-schedule';

const MIN = 60_000;
const job = (over: Partial<ImberJob> = {}): ImberJob => ({
  id: 'j1', name: 'Check', enabled: true, recurrence: { kind: 'daily', hour: 9, minute: 0 }, missed: 'skip', action: { kind: 'prompt', prompt: 'status?' },
  root: '/r', mode: 'manual', created_ms: 0, next_run_ms: null, last_run: null, runs: 0, ...over,
});

describe('local date and time', () => {
  it('reads and writes the same text', () => {
    const ms = parseLocal('2026-11-05 09:00');
    expect(ms).not.toBeNull();
    expect(formatLocal(ms!)).toBe('2026-11-05 09:00');
    expect(parseLocal('2026-11-05T09:00')).toBe(ms);
  });

  it('refuses what is not a real date', () => {
    for (const bad of ['', 'tomorrow', '2026-02-31 09:00', '2026-13-01 09:00', '2026-11-05 25:00', '2026-11-05']) expect(parseLocal(bad), bad).toBeNull();
  });

  it('reads a time of day', () => {
    expect(parseClock('9:05')).toEqual({ hour: 9, minute: 5 });
    expect(parseClock('24:00')).toBeNull();
    expect(parseClock('nine')).toBeNull();
  });
});

describe('describeRecurrence', () => {
  it('says each kind in words', () => {
    expect(describeRecurrence({ kind: 'daily', hour: 9, minute: 5 })).toBe('Every day at 09:05');
    expect(describeRecurrence({ kind: 'weekly', days: [0, 1, 2, 3, 4], hour: 8, minute: 0 })).toBe('Weekdays at 08:00');
    expect(describeRecurrence({ kind: 'weekly', days: [4, 0], hour: 8, minute: 0 })).toBe('Mon, Fri at 08:00');
    expect(describeRecurrence({ kind: 'weekly', days: [0, 1, 2, 3, 4, 5, 6], hour: 8, minute: 0 })).toBe('Every day at 08:00');
    expect(describeRecurrence({ kind: 'interval', secs: 1800, anchor_ms: 0 })).toBe('Every 30 minutes');
    expect(describeRecurrence({ kind: 'interval', secs: 3600, anchor_ms: 0 })).toBe('Every hour');
    expect(describeRecurrence({ kind: 'interval', secs: 7200, anchor_ms: 0 })).toBe('Every 2 hours');
    expect(describeRecurrence({ kind: 'cron', expr: '0 0 9 * * 1-5' })).toBe('Cron 0 0 9 * * 1-5');
    expect(describeRecurrence({ kind: 'once', at_ms: parseLocal('2026-11-05 09:00')! })).toBe('Once, 2026-11-05 09:00');
  });
});

describe('relativeTime', () => {
  it('counts in the right unit and direction', () => {
    const now = 1_000_000_000;
    expect(relativeTime(now + 10_000, now)).toBe('just now');
    expect(relativeTime(now + 12 * MIN, now)).toBe('in 12 min');
    expect(relativeTime(now - 5 * MIN, now)).toBe('5 min ago');
    expect(relativeTime(now + 3 * 60 * MIN, now)).toBe('in 3 h');
    expect(relativeTime(now - 72 * 60 * MIN, now)).toBe('3 d ago');
  });
});

describe('the form', () => {
  it('starts as weekdays at 09:00, read-only, skipping missed runs', () => {
    const f = blankForm(Date.now());
    expect(recurrenceFromForm(f)).toEqual({ ok: { kind: 'weekly', days: [0, 1, 2, 3, 4], hour: 9, minute: 0 } });
    expect([f.mode, f.missed, f.enabled]).toEqual(['manual', 'skip', true]);
  });

  it('round-trips a job through the form', () => {
    const j = job({ name: 'Weekly', recurrence: { kind: 'weekly', days: [0, 4], hour: 7, minute: 30 }, mode: 'auto', missed: 'run_once' });
    const input = inputFromForm(formFromJob(j), '/r', null, j.id);
    expect(input).toMatchObject({ id: 'j1', name: 'Weekly', recurrence: j.recurrence, mode: 'auto', missed: 'run_once', action: { kind: 'prompt', prompt: 'status?' } });
  });

  it('says what is wrong, one thing at a time', () => {
    const f = blankForm(0);
    expect(inputFromForm(f, '/r', null)).toEqual({ error: 'Give the schedule a name.' });
    f.name = 'x';
    expect(inputFromForm(f, '/r', null)).toEqual({ error: 'Write the prompt Imber will be given.' });
    f.prompt = 'p';
    f.days = f.days.map(() => false);
    expect(inputFromForm(f, '/r', null)).toEqual({ error: 'Pick at least one weekday.' });
    f.when = 'once';
    f.at = 'soon';
    expect(inputFromForm(f, '/r', null)).toEqual({ error: 'Write the date and time as 2026-11-05 09:00.' });
    f.when = 'cron';
    f.cron = 'every day';
    expect(inputFromForm(f, '/r', null)).toMatchObject({ error: expect.stringContaining('6 fields') });
    f.when = 'interval';
    f.minutes = 0;
    expect(inputFromForm(f, '/r', null)).toEqual({ error: 'The interval must be at least one minute.' });
  });

  it('an awake wakes the conversation it was made from, and keeps its own when edited', () => {
    const f = blankForm(0);
    Object.assign(f, { name: 'n', prompt: 'p', action: 'awake' });
    expect(inputFromForm(f, '/r', 'c-new')).toMatchObject({ action: { kind: 'awake', conversation_id: 'c-new' } });
    expect(inputFromForm(f, '/r', 'c-new', 'j1', 'c-old')).toMatchObject({ action: { kind: 'awake', conversation_id: 'c-old' } });
  });

  it('a fetch needs no prompt', () => {
    const f = blankForm(0);
    Object.assign(f, { name: 'n', action: 'fetch', remote: ' upstream ' });
    expect(inputFromForm(f, '/r', null)).toMatchObject({ action: { kind: 'fetch', remote: 'upstream' } });
  });
});

describe('the lines a row shows', () => {
  it('says when it runs next and how it last ended', () => {
    const now = 10_000_000_000;
    expect(nextRunLine(job({ next_run_ms: now + 90 * MIN }), now)).toMatch(/^Next \d{4}-\d{2}-\d{2} \d{2}:\d{2} \(in 2 h\)$/);
    expect(nextRunLine(job({ enabled: false }), now)).toBe('Off');
    expect(nextRunLine(job({ recurrence: { kind: 'once', at_ms: 1 }, runs: 1, enabled: false }), now)).toBe('Ran once');
    expect(lastRunLine(job(), now)).toBe('Never run');
    expect(lastRunLine(job({ last_run: { at_ms: now - 5 * MIN, ok: true, detail: '' } }), now)).toBe('Done 5 min ago');
    expect(lastRunLine(job({ last_run: { at_ms: now - 5 * MIN, ok: false, detail: 'x' } }), now)).toBe('Failed 5 min ago');
  });
});
