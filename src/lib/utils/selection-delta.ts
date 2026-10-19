/**
 * Narrow a revision's diff-against-disk to a range of lines of the current file.
 *
 * Local History can only compare a revision to what is on disk, so "the history of these lines"
 * is built from those diffs: a revision *touches* the selection when its diff against disk has
 * changes inside it, and two revisions whose changes inside the selection are identical held the
 * same text there. Lines are the disk file's (the new side), which is what the user highlighted.
 *
 * A removed line has no new-side number; it is placed at the position of the line that follows it,
 * i.e. between the line before and the one after, and counts as inside the range when that gap
 * falls within it.
 */
import type { DiffHunk } from '$lib/ipc/bennu/history';

/** Where each line of a hunk sits on the disk side. A deletion sits at the next line's number. */
function positions(hunk: DiffHunk): number[] {
  let next = hunk.new_start;
  return hunk.lines.map((l) => {
    if (l.kind === 'del') return next;
    const at = l.new ?? next;
    next = at + 1;
    return at;
  });
}

/**
 * What changed inside `start..end` (1-based, inclusive), as a string: equal strings mean the
 * revisions held the same text in that range; the empty string means the range is identical to
 * disk. Context lines are left out — they are unchanged by definition.
 */
export function selectionSignature(hunks: readonly DiffHunk[], start: number, end: number): string {
  const parts: string[] = [];
  for (const hunk of hunks) {
    const at = positions(hunk);
    hunk.lines.forEach((l, i) => {
      if (l.kind === 'context') return;
      if (at[i] >= start && at[i] <= end) parts.push(`${l.kind}${at[i]}:${l.text}`);
    });
  }
  return parts.join('\n');
}

/** The hunks that change something inside `start..end`, whole — their context is what makes
 *  a change readable. */
export function hunksTouching(hunks: readonly DiffHunk[], start: number, end: number): DiffHunk[] {
  return hunks.filter((hunk) => {
    const at = positions(hunk);
    return hunk.lines.some((l, i) => l.kind !== 'context' && at[i] >= start && at[i] <= end);
  });
}

/**
 * Which of `revisions` (newest first) to list for a selection, given each one's signature.
 *
 * A row is a *state* of the lines; it is listed when it differs from the next newer state (the
 * newest is compared with the disk, whose signature is `''`) — i.e. when somebody changed the
 * lines between the two. States identical to their newer neighbour are the same version of the
 * selection and are listed once, at the newest of the run's end.
 */
export function revisionsWhereSelectionChanged<T>(
  revisions: readonly T[],
  signatures: readonly string[],
): T[] {
  return revisions.filter((_, i) => signatures[i] !== (i === 0 ? '' : signatures[i - 1]));
}
