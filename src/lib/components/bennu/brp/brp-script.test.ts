import { describe, expect, it } from 'vitest';
import { MAX_STEPS, describeStep, gaps, parseScript, recorder, scriptToJson } from './brp-script';

describe('recorder', () => {
  it('stamps steps with the time since the start', () => {
    const r = recorder(1000);
    r.add('key', { key: 'a' }, 1250);
    r.add('text', { text: 'hi' }, 900);
    expect(r.steps.map((s) => s.at)).toEqual([250, 0]);
  });

  it('refuses past the limit', () => {
    const r = recorder(0);
    for (let i = 0; i < MAX_STEPS; i++) r.add('key', { key: 'a' }, i);
    expect(r.add('key', { key: 'a' }, 1)).toBe(false);
  });
});

describe('parseScript', () => {
  it('round-trips what scriptToJson wrote', () => {
    const steps = [{ at: 0, call: 'key' as const, args: { key: 'a' } }, { at: 120, call: 'mouse' as const, args: { action: 'click', x: 1, y: 2 } }];
    expect(parseScript(scriptToJson(steps))).toEqual({ ok: true, steps });
  });

  it('accepts a bare array and sorts by time', () => {
    const r = parseScript('[{"at":50,"call":"key","args":{}},{"at":10,"call":"text","args":{"text":"x"}}]');
    expect(r.ok && r.steps.map((s) => s.at)).toEqual([10, 50]);
  });

  it('says which step is wrong', () => {
    expect(parseScript('{"steps":[{"at":0,"call":"rm","args":{}}]}')).toEqual({ ok: false, error: 'Step 1: "call" must be one of key, text, mouse, gamepad.' });
    expect(parseScript('{"steps":[{"at":-1,"call":"key","args":{}}]}').ok).toBe(false);
    expect(parseScript('{"steps":[{"at":0,"call":"key"}]}').ok).toBe(false);
    expect(parseScript('nope').ok).toBe(false);
    expect(parseScript('{}').ok).toBe(false);
  });
});

describe('gaps / describeStep', () => {
  it('waits by the difference between neighbours', () => {
    expect(gaps([{ at: 100, call: 'key', args: {} }, { at: 350, call: 'key', args: {} }, { at: 400, call: 'key', args: {} }])).toEqual([0, 250, 50]);
  });

  it('describes each kind of step', () => {
    expect(describeStep({ at: 0, call: 'key', args: { key: 's', modifiers: ['ctrl'] } })).toBe('key ctrl+s');
    expect(describeStep({ at: 0, call: 'mouse', args: { action: 'drag', x: 1, y: 2, to_x: 3, to_y: 4 } })).toBe('mouse drag 1,2 → 3,4');
    expect(describeStep({ at: 0, call: 'text', args: { text: 'hi' } })).toBe('type “hi”');
    expect(describeStep({ at: 0, call: 'gamepad', args: { axis: 'LeftStickX', value: 1 } })).toBe('gamepad LeftStickX 1');
  });
});
