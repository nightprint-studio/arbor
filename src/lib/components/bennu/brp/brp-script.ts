/**
 * Input scripts: what the Input panel records and replays — a list of calls with the time at which each
 * was made, so a replay keeps the rhythm of the recording.
 */

export type ScriptCall = 'key' | 'text' | 'mouse' | 'gamepad';

export interface ScriptStep {
  /** Milliseconds since the recording started. */
  at: number;
  call: ScriptCall;
  args: Record<string, unknown>;
}

export const MAX_STEPS = 2000;
const CALLS: readonly string[] = ['key', 'text', 'mouse', 'gamepad'];

/** A recording in progress: `add` stamps each step with the time since `start`. */
export function recorder(start: number) {
  const steps: ScriptStep[] = [];
  return {
    steps,
    add(call: ScriptCall, args: Record<string, unknown>, now: number): boolean {
      if (steps.length >= MAX_STEPS) return false;
      steps.push({ at: Math.max(0, Math.round(now - start)), call, args });
      return true;
    },
  };
}

export function scriptToJson(steps: readonly ScriptStep[]): string {
  return JSON.stringify({ version: 1, steps }, null, 2);
}

/** Read a script back, refusing anything that would make a replay do something other than input. */
export function parseScript(text: string): { ok: true; steps: ScriptStep[] } | { ok: false; error: string } {
  let doc: unknown;
  try {
    doc = JSON.parse(text);
  } catch (e) {
    return { ok: false, error: `Not valid JSON: ${e instanceof Error ? e.message : String(e)}` };
  }
  const raw = Array.isArray(doc) ? doc : (doc as { steps?: unknown } | null)?.steps;
  if (!Array.isArray(raw)) return { ok: false, error: 'Expected { "steps": [ … ] }.' };
  if (raw.length > MAX_STEPS) return { ok: false, error: `A script has at most ${MAX_STEPS} steps.` };
  const steps: ScriptStep[] = [];
  for (const [i, s] of raw.entries()) {
    const step = s as Partial<ScriptStep> | null;
    if (!step || typeof step.at !== 'number' || !Number.isFinite(step.at) || step.at < 0) {
      return { ok: false, error: `Step ${i + 1}: "at" must be a number of milliseconds.` };
    }
    if (typeof step.call !== 'string' || !CALLS.includes(step.call)) {
      return { ok: false, error: `Step ${i + 1}: "call" must be one of ${CALLS.join(', ')}.` };
    }
    if (!step.args || typeof step.args !== 'object' || Array.isArray(step.args)) {
      return { ok: false, error: `Step ${i + 1}: "args" must be an object.` };
    }
    steps.push({ at: step.at, call: step.call as ScriptCall, args: step.args as Record<string, unknown> });
  }
  steps.sort((a, b) => a.at - b.at);
  return { ok: true, steps };
}

/** The wait before each step: the first immediately, the rest by the gap to the one before. */
export function gaps(steps: readonly ScriptStep[]): number[] {
  return steps.map((s, i) => (i === 0 ? 0 : Math.max(0, s.at - steps[i - 1].at)));
}

/** One line for a step, for the list: `key Enter`, `mouse click 120,80`. */
export function describeStep(s: ScriptStep): string {
  const a = s.args as Record<string, unknown>;
  switch (s.call) {
    case 'key': return `key ${[...(Array.isArray(a.modifiers) ? a.modifiers : []), a.key].join('+')}${a.action && a.action !== 'press' ? ` (${a.action})` : ''}`;
    case 'text': return `type “${String(a.text ?? '')}”`;
    case 'mouse': return `mouse ${a.action}${a.x !== undefined ? ` ${a.x},${a.y}` : ''}${a.to_x !== undefined ? ` → ${a.to_x},${a.to_y}` : ''}`;
    case 'gamepad': return `gamepad ${a.button ?? a.axis}${a.value !== undefined ? ` ${a.value}` : ''}`;
  }
}
