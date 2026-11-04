/**
 * Structural search — the pure half of the modal: scope, grouping of hits by file, the query's problems as
 * editor diagnostics, and the small arithmetic of walking results. No Svelte, no IPC, so it is tested alone.
 */

import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor';
import type { SsrExplained } from '$lib/ipc/bennu/ssr';

// ── scope ────────────────────────────────────────────────────────────────────

export type SsrScopeKind = 'project' | 'module' | 'directory' | 'file';

export interface SsrScope {
  kind: SsrScopeKind;
  /** Project-relative, forward-slashed. Empty for the whole project. */
  path: string;
}

export const WHOLE_PROJECT: SsrScope = { kind: 'project', path: '' };

/** Whether the query already carries its own `in <path>` clause. */
export function hasInClause(query: string): boolean {
  return /^\s*in\s+\S/m.test(query);
}

/**
 * The query the backend is sent: the text the user wrote plus the scope picked beside it, as the `in` clause
 * the language already has — so the scope is not a second mechanism but the same one, said for you.
 *
 * A query that writes its own `in` is left alone: two clauses would be a union, and the one typed wins.
 */
export function withScope(query: string, scope: SsrScope): string {
  if (scope.kind === 'project' || !scope.path || hasInClause(query)) return query;
  const path = scope.path.replace(/\\/g, '/').replace(/^\/+/, '');
  return `${query.replace(/\s+$/, '')}\nin ${path}`;
}

/** `abs` relative to `root`, forward-slashed; `abs` itself when it is not under it. */
export function relativeTo(root: string, abs: string): string {
  const r = root.replace(/\\/g, '/').replace(/\/+$/, '');
  const a = abs.replace(/\\/g, '/');
  if (a.toLowerCase() === r.toLowerCase()) return '';
  return a.toLowerCase().startsWith(`${r.toLowerCase()}/`) ? a.slice(r.length + 1) : a;
}

/** The directory part of a project-relative path. */
export function parentOf(rel: string): string {
  const i = rel.lastIndexOf('/');
  return i < 0 ? '' : rel.slice(0, i);
}

// ── results ───────────────────────────────────────────────────────────────────

export interface FileGroup<H> {
  file: string;
  rel: string;
  hits: H[];
}

/** Hits grouped by file, files in the order they first appear (the walk's own order). */
export function groupByFile<H extends { file: string; rel: string }>(hits: readonly H[]): FileGroup<H>[] {
  const byFile = new Map<string, FileGroup<H>>();
  for (const hit of hits) {
    let group = byFile.get(hit.file);
    if (!group) {
      group = { file: hit.file, rel: hit.rel, hits: [] };
      byFile.set(hit.file, group);
    }
    group.hits.push(hit);
  }
  return [...byFile.values()];
}

export const plural = (n: number, word: string, many = `${word}s`) => `${n} ${n === 1 ? word : many}`;

/** "12 matches in 3 files". */
export function summary(matches: number, files: number): string {
  return `${plural(matches, 'match', 'matches')} in ${plural(files, 'file')}`;
}

/** `i` moved by `delta` inside `len` items, wrapping round. -1 for an empty list. */
export function stepIndex(i: number, delta: number, len: number): number {
  if (len <= 0) return -1;
  return (((i + delta) % len) + len) % len;
}

/** A duration the way it reads: `840ms`, `4.2s`, `1m 05s`. */
export function formatElapsed(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, '0')}s`;
}

// ── the query's problems, as diagnostics ─────────────────────────────────────────

const encoder = new TextEncoder();
const bytes = (s: string) => encoder.encode(s).length;

/** The byte range of 1-based `line` of `text` (the whole first line when `line` is 0 or past the end). */
function lineBytes(text: string, line: number): { from: number; to: number } {
  const lines = text.split('\n');
  const idx = line >= 1 && line <= lines.length ? line - 1 : 0;
  let from = 0;
  for (let i = 0; i < idx; i++) from += bytes(lines[i]) + 1;
  return { from, to: from + bytes(lines[idx]) };
}

/** `$name$` / `$name...$` / `$name: …$` holes of `text` with their UTF-16 offsets. */
export function holesIn(text: string): { name: string; from: number; to: number }[] {
  const out: { name: string; from: number; to: number }[] = [];
  for (const m of text.matchAll(/\$([A-Za-z0-9_]+)(?:\.\.\.)?(?::[^$\n]*)?\$/g)) {
    out.push({ name: m[1], from: m.index, to: m.index + m[0].length });
  }
  return out;
}

/** The query's syntax error as an editor diagnostic, or none when it reads. */
export function queryDiagnostics(query: string, explained: SsrExplained | null): EditorDiagnostic[] {
  if (!explained?.error) return [];
  const { from, to } = lineBytes(query, explained.errorLine);
  return [{ from, to, severity: 'error', message: explained.error }];
}

/** Captures a replacement names that the query does not bind, with their byte ranges in the replacement. */
export function replacementDiagnostics(replacement: string, captures: readonly string[]): EditorDiagnostic[] {
  if (!replacement.trim()) return [];
  const known = new Set(captures);
  const out: EditorDiagnostic[] = [];
  for (const hole of holesIn(replacement)) {
    if (known.has(hole.name)) continue;
    out.push({
      from: bytes(replacement.slice(0, hole.from)),
      to: bytes(replacement.slice(0, hole.to)),
      severity: 'error',
      message: `$${hole.name}$ is not bound by the query`,
    });
  }
  return out;
}

/** The first of `a`…`z` the query does not bind yet — the name a new hole gets. */
export function nextHoleName(used: readonly string[]): string {
  for (const n of 'abcdefghijklmnopqrstuvwxyz') if (!used.includes(n)) return n;
  return `v${used.length + 1}`;
}

/** Insert `$name$` into `text` at UTF-16 `at`, with a space before when it would glue to a word. */
export function insertVariable(text: string, at: number, name: string): { text: string; caret: number } {
  const hole = `$${name}$`;
  const before = text.slice(0, at);
  const glue = before.length > 0 && /[A-Za-z0-9_$]$/.test(before) ? ' ' : '';
  const next = `${before}${glue}${hole}${text.slice(at)}`;
  return { text: next, caret: at + glue.length + hole.length };
}
