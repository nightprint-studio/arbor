// Normalize the plugin-facing diff shape (FormDiffHunk[]) into the app's
// DiffHunk[] the way DiffHunk/VirtualHunk expect: filled line numbers, a
// synthesised `@@ … @@` header, and the per-hunk old/new line counts.
//
// The plugin supplies only `{ kind, content }` lines (plus optional explicit
// line numbers and start offsets); everything else is derived here so a
// Lua-side diff stays terse.

import type { FormDiffHunk } from '$lib/types/plugin';
import type { DiffHunk, DiffLine } from '$lib/types/git';

export function normalizeDiffHunks(hunks: FormDiffHunk[] | undefined): DiffHunk[] {
  if (!Array.isArray(hunks)) return [];
  return hunks.map(normalizeHunk);
}

function normalizeHunk(h: FormDiffHunk): DiffHunk {
  const oldStart = h.old_start ?? 1;
  const newStart = h.new_start ?? 1;

  let oldNo = oldStart;
  let newNo = newStart;
  let oldCount = 0;
  let newCount = 0;

  const lines: DiffLine[] = (Array.isArray(h.lines) ? h.lines : []).map((l) => {
    const out: DiffLine = { kind: l.kind, content: l.content ?? '' };
    if (l.kind !== 'added') {
      out.old_lineno = l.old_lineno ?? oldNo++;
      oldCount++;
    }
    if (l.kind !== 'removed') {
      out.new_lineno = l.new_lineno ?? newNo++;
      newCount++;
    }
    return out;
  });

  const header = h.header ?? `@@ -${oldStart},${oldCount} +${newStart},${newCount} @@`;

  return {
    header,
    old_start: oldStart,
    old_lines: oldCount,
    new_start: newStart,
    new_lines: newCount,
    lines,
  };
}

/** Additions/deletions across all hunks (drives the header stats). */
export function diffStats(hunks: DiffHunk[]): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const h of hunks) {
    for (const l of h.lines) {
      if (l.kind === 'added') additions++;
      else if (l.kind === 'removed') deletions++;
    }
  }
  return { additions, deletions };
}

/** Total rendered line count — drives the virtualization fallback. */
export function totalLineCount(hunks: DiffHunk[]): number {
  let n = 0;
  for (const h of hunks) n += h.lines.length;
  return n;
}
