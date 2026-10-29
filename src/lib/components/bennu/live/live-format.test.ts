import { describe, expect, it } from 'vitest';
import type { LiveEvent } from '$lib/ipc/bennu/live';
import { durationText, eventClass, processState, stepIndex, uptimeText } from './live-format';

const event = (over: Partial<LiveEvent>): LiveEvent => ({
  seq: 1, ts_ms: 0, kind: 'log', source: 'api', process_id: 'p1', corr: null, text: 'x', ...over,
});

describe('durationText', () => {
  it('writes milliseconds, seconds and minutes', () => {
    expect(durationText(250)).toBe('250 ms');
    expect(durationText(1500)).toBe('1.5 s');
    expect(durationText(125_000)).toBe('2 min 5 s');
  });
});

describe('processState', () => {
  it('tones the states', () => {
    expect(processState({ phase: 'ready', exit_code: null })).toEqual({ label: 'ready', tone: 'success' });
    expect(processState({ phase: 'exited', exit_code: 0 }).tone).toBe('neutral');
    expect(processState({ phase: 'exited', exit_code: 3 })).toEqual({ label: 'exited 3', tone: 'error' });
    expect(processState({ phase: 'stopped', exit_code: 1 }).tone).toBe('neutral');
  });
});

describe('uptimeText', () => {
  it('measures a finished process to its end and a live one to now', () => {
    expect(uptimeText({ started_ms: 1000, ended_ms: 3000 }, 99_000)).toBe('2.0 s');
    expect(uptimeText({ started_ms: 1000, ended_ms: null }, 1500)).toBe('500 ms');
  });
});

describe('eventClass', () => {
  it('marks errors, requests and stderr', () => {
    expect(eventClass(event({ kind: 'http_response', text: '500 Internal Server Error in 8 ms' }))).toBe('line-error');
    expect(eventClass(event({ kind: 'http_response', text: '200 OK in 8 ms' }))).toBe('line-http-res');
    expect(eventClass(event({ kind: 'http_request', text: 'GET /' }))).toBe('line-http-req');
    expect(eventClass(event({ data: { stream: 'stderr' } }))).toBe('line-stderr');
    expect(eventClass(event({}))).toBe('');
  });
});

describe('stepIndex', () => {
  it('moves inside the list', () => {
    expect(stepIndex(-1, 3, 'ArrowDown')).toBe(0);
    expect(stepIndex(2, 3, 'ArrowDown')).toBe(2);
    expect(stepIndex(0, 3, 'ArrowUp')).toBe(0);
    expect(stepIndex(1, 3, 'End')).toBe(2);
    expect(stepIndex(1, 0, 'ArrowDown')).toBe(-1);
    expect(stepIndex(1, 3, 'a')).toBe(1);
  });
});
