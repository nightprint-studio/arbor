/**
 * One synchronous regex run, whatever the flavour: parse, match, substitute, split.
 *
 * Java and Rust/PCRE go through this module's own engine ({@link executeRegex}) so Java patterns
 * keep Java semantics; JavaScript goes through the host's `RegExp`, which is its reference. This is
 * the function the worker calls — it blocks for as long as the pattern takes, which is exactly why
 * it never runs on the UI thread (see `run.ts`).
 */

import { executeRegex } from './engine';
import { parseRegex } from './parse';
import { javaReplaceAll, javaSplit, rustReplaceAll } from './replace';
import type { GroupMatch, RegexMatch, RunRequest, RunResult } from './types';

const EMPTY: RunResult = {
  ok: false, error: null, matches: [], groupNames: [null], replaced: null, replaceError: null,
  split: null, aborted: false, truncated: false, ms: 0,
};

export function runRegex(req: RunRequest): RunResult {
  const t0 = performance.now();
  const result = req.flavour === 'js' ? runJs(req) : runOwn(req);
  return { ...result, ms: Math.round((performance.now() - t0) * 10) / 10 };
}

function runOwn(req: RunRequest): RunResult {
  const parsed = parseRegex(req.pattern, req.flavour, req.flags);
  if (parsed.errors.length > 0) {
    return { ...EMPTY, error: parsed.errors[0].message, groupNames: parsed.groupNames };
  }
  const { matches, aborted, truncated } = executeRegex(parsed, req.text, req.mode, { maxMatches: req.maxMatches });
  let replaced: string | null = null;
  let replaceError: string | null = null;
  if (req.replacement !== null) {
    const r = req.flavour === 'java'
      ? javaReplaceAll(req.text, matches, req.replacement, parsed.groupNames)
      : rustReplaceAll(req.text, matches, req.replacement, parsed.groupNames);
    replaced = r.error ? null : r.out;
    replaceError = r.error;
  }
  return {
    ok: true, error: null, matches, groupNames: parsed.groupNames, replaced, replaceError,
    split: req.flavour === 'java' && req.mode === 'find' ? javaSplit(req.text, matches) : null,
    aborted, truncated, ms: 0,
  };
}

// ── JavaScript ────────────────────────────────────────────────────────────────────

/** The JS source for a mode: `matches` / `lookingAt` are anchored with look-arounds that mean
 *  "start / end of input" whatever the `m` flag says. */
function jsSource(pattern: string, mode: RunRequest['mode']): string {
  if (mode === 'find') return pattern;
  return `(?<![^])(?:${pattern})${mode === 'matches' ? '(?![^])' : ''}`;
}

function runJs(req: RunRequest): RunResult {
  const userFlags = [...new Set(req.flags)].join('');
  const global = userFlags.includes('g');
  const base = userFlags.replace(/[gyd]/g, '');
  let re: RegExp;
  try {
    re = new RegExp(jsSource(req.pattern, req.mode), `${base}dg`);
  } catch (e) {
    return { ...EMPTY, error: e instanceof Error ? e.message : String(e) };
  }
  const groupNames = namesOf(req.pattern);
  const matches: RegexMatch[] = [];
  let truncated = false;
  const limit = global || req.mode !== 'find' ? req.maxMatches : 1;
  for (let m = re.exec(req.text); m; m = re.exec(req.text)) {
    matches.push(fromExec(m, req.text));
    if (matches.length >= limit) { truncated = global && matches.length >= req.maxMatches; break; }
    if (m[0] === '') re.lastIndex += 1;
  }

  let replaced: string | null = null;
  let replaceError: string | null = null;
  if (req.replacement !== null) {
    try {
      replaced = req.text.replace(new RegExp(req.pattern, `${base}${global ? 'g' : ''}`), req.replacement);
    } catch (e) {
      replaceError = e instanceof Error ? e.message : String(e);
    }
  }
  let split: string[] | null = null;
  if (req.mode === 'find') {
    try { split = req.text.split(new RegExp(req.pattern, base)); } catch { split = null; }
  }
  return { ok: true, error: null, matches, groupNames, replaced, replaceError, split, aborted: false, truncated, ms: 0 };
}

function fromExec(m: RegExpExecArray, text: string): RegexMatch {
  const indices = (m as RegExpExecArray & { indices: ([number, number] | undefined)[] }).indices;
  const groups: (GroupMatch | null)[] = [];
  for (let g = 0; g < m.length; g++) {
    const span = indices[g];
    groups.push(span ? { start: span[0], end: span[1], text: text.slice(span[0], span[1]) } : null);
  }
  return { start: m.index, end: m.index + m[0].length, groups };
}

/** Group names by index for a JS pattern, read by the shared parser so numbering matches the engine's. */
function namesOf(pattern: string): (string | null)[] {
  return parseRegex(pattern, 'js').groupNames;
}
