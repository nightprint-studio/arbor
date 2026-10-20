/**
 * The environment of a run configuration as text — the `KEY=value` lines of a `.env` file.
 *
 * The editor offers the same variables two ways (a table for a few, a text box for a pasted block),
 * and these two functions are the whole conversion between them. Kept out of the component so the
 * round trip is one definition, not two that drift: a value the table writes must be one the text
 * view reads back unchanged.
 */

import type { EnvVar } from '$lib/stores/bennu/run-config.svelte';

/** Variables to text: one `KEY=value` per line. A value that would not survive being read back
 *  bare (leading/trailing space, a ` #`, a newline) is double-quoted. */
export function envToText(env: EnvVar[]): string {
  return env
    .filter((e) => e.key.trim())
    .map((e) => `${e.key.trim()}=${needsQuotes(e.value) ? quote(e.value) : e.value}`)
    .join('\n');
}

/** Text to variables. Tolerant, like the backend's parser: comments, blank lines and an `export `
 *  prefix are ignored, a line with no `=` is skipped (one stray line must not cost the rest). */
export function textToEnv(text: string): EnvVar[] {
  const out: EnvVar[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const body = line.replace(/^export\s+/, '');
    const eq = body.indexOf('=');
    if (eq <= 0) continue;
    const key = body.slice(0, eq).trim();
    if (!key || /\s/.test(key)) continue;
    out.push({ key, value: unquote(body.slice(eq + 1).trimStart()) });
  }
  return out;
}

/** Whether a variable's value should be hidden by default. A name containing one of these words is
 *  a credential often enough that showing it on a screen-share is the worse default. */
export function isSecretKey(key: string): boolean {
  return /(pass(word|wd)?|secret|token|api[_-]?key|private[_-]?key|credential)/i.test(key);
}

function needsQuotes(v: string): boolean {
  return v !== v.trim() || /\s#|\n|"/.test(v);
}

function quote(v: string): string {
  return `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n')}"`;
}

function unquote(v: string): string {
  if (v.startsWith('"')) {
    let out = '';
    for (let i = 1; i < v.length; i++) {
      const c = v[i];
      if (c === '"') break;
      if (c === '\\' && i + 1 < v.length) {
        const n = v[++i];
        out += n === 'n' ? '\n' : n === 't' ? '\t' : n;
      } else out += c;
    }
    return out;
  }
  if (v.startsWith("'")) return v.slice(1).split("'")[0];
  const hash = v.indexOf(' #');
  return (hash === -1 ? v : v.slice(0, hash)).trimEnd();
}
