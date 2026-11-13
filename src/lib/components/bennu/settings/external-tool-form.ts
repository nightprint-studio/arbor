/**
 * The external-tool editor's fields as text, and back. Arguments are one per line — the only form in which an
 * argument with spaces cannot be misread, since nothing here is ever split like a shell line; variables are
 * `KEY=value` lines; the keys Imber may set are a list separated by commas or spaces.
 */
import type { ExternalTool, ExternalToolApproval } from '$lib/ipc/bennu/config';

export const APPROVAL_OPTIONS: { value: ExternalToolApproval; label: string; description: string }[] = [
  { value: 'conversation', label: 'Once per conversation', description: 'The first call asks; the yes covers the rest of the conversation, until you edit the tool.' },
  { value: 'every_call', label: 'Every call', description: 'Each call shows the command and waits for you.' },
  { value: 'never', label: 'Never', description: 'You trust it: calls run without asking (in manual mode a tool that changes things still asks).' },
];

export function blankTool(): ExternalTool {
  return {
    name: '', description: '', program: '', args: [], extra_args: true, env: {}, env_keys: [],
    writes: false, approval: 'conversation', timeout_s: 120, enabled: true,
  };
}

/** One argument per line; blank lines are dropped, the rest kept exactly (spaces included). */
export const argsFromLines = (text: string): string[] => text.split('\n').filter((l) => l.trim() !== '');
export const linesFromArgs = (args: string[]): string => args.join('\n');

/** `KEY=value` per line; a line without `=` sets the key to an empty value. */
export function envFromLines(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    const at = line.indexOf('=');
    const key = (at < 0 ? line : line.slice(0, at)).trim();
    if (key) out[key] = at < 0 ? '' : line.slice(at + 1);
  }
  return out;
}
export const linesFromEnv = (env: Record<string, string>): string =>
  Object.entries(env).map(([k, v]) => `${k}=${v}`).join('\n');

export const keysFromText = (text: string): string[] => [...new Set(text.split(/[\s,]+/).filter(Boolean))];

/** The file name of a program path, for a row that has no room for the whole path. */
export const programName = (path: string): string => path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;

/** A name for a new tool from its program: `/usr/local/bin/Lint.exe` → `lint`. */
export function nameFromProgram(path: string): string {
  return programName(path).replace(/\.(exe|cmd|bat|sh)$/i, '').toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^[^a-z]+/, '').slice(0, 40);
}
