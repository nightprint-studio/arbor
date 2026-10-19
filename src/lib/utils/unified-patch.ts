/**
 * Parse the text of a unified diff into the hunk model `TextDiff` draws.
 *
 * Git hands history back as patch text (`git show`, `git log -L`); the shared diff widget wants
 * hunks of numbered lines. This is the bridge, and it is deliberately tolerant: file headers
 * (`diff --git`, `index`, `---`, `+++`), rename notes and `\ No newline at end of file` are
 * skipped, and a patch with several files simply yields all their hunks in order.
 *
 * A hunk ends when the line counts in its `@@` header are used up — not when a line *looks* like
 * a header — because a deleted line whose text begins `-- ` is `--- ` on the wire, and a parser
 * that took that for the next file header would drop real lines.
 */
import type { DiffHunkModel, DiffLineModel } from '$lib/components/shared/ui/TextDiff.svelte';

const HUNK_HEADER = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

export function parseUnifiedPatch(patch: string): DiffHunkModel[] {
  const hunks: DiffHunkModel[] = [];
  const lines = patch.split('\n');
  let i = 0;
  while (i < lines.length) {
    const m = HUNK_HEADER.exec(lines[i]);
    i++;
    if (!m) continue;
    const oldStart = Number(m[1]);
    const newStart = Number(m[3]);
    let oldLeft = m[2] === undefined ? 1 : Number(m[2]);
    let newLeft = m[4] === undefined ? 1 : Number(m[4]);
    let oldNo = oldStart;
    let newNo = newStart;
    const body: DiffLineModel[] = [];
    while (i < lines.length && (oldLeft > 0 || newLeft > 0)) {
      const raw = lines[i];
      const tag = raw[0];
      if (tag === '\\') { i++; continue; } // "\ No newline at end of file"
      if (tag === '+') {
        body.push({ kind: 'add', new: newNo++, text: raw.slice(1) });
        newLeft--;
      } else if (tag === '-') {
        body.push({ kind: 'del', old: oldNo++, text: raw.slice(1) });
        oldLeft--;
      } else {
        // A context line; an empty line inside a hunk is a context line whose text is empty.
        body.push({ kind: 'context', old: oldNo++, new: newNo++, text: raw.slice(1) });
        oldLeft--;
        newLeft--;
      }
      i++;
    }
    hunks.push({ old_start: oldStart, new_start: newStart, lines: body });
  }
  return hunks;
}
