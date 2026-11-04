/**
 * A line diff for the replace preview: the hunks that change, with a little context, instead of two whole files
 * side by side. Pure.
 */

export type DiffKind = 'same' | 'del' | 'add';

export interface DiffLine {
  kind: DiffKind;
  text: string;
  /** 1-based line in the old text (absent for an added line). */
  oldNo?: number;
  /** 1-based line in the new text (absent for a removed line). */
  newNo?: number;
}

export interface Hunk {
  lines: DiffLine[];
}

/** The middle of a diff is solved exactly up to this many cells; past it the whole middle is one replacement. */
const MAX_CELLS = 4_000_000;

/** Every line of the diff, in order. */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = before.split('\n');
  const b = after.split('\n');
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) { endA--; endB--; }

  const out: DiffLine[] = [];
  for (let i = 0; i < start; i++) out.push({ kind: 'same', text: a[i], oldNo: i + 1, newNo: i + 1 });
  out.push(...middle(a, b, start, endA, endB));
  for (let i = endA, j = endB; i < a.length; i++, j++) out.push({ kind: 'same', text: a[i], oldNo: i + 1, newNo: j + 1 });
  return out;
}

function middle(a: string[], b: string[], start: number, endA: number, endB: number): DiffLine[] {
  const n = endA - start;
  const m = endB - start;
  const out: DiffLine[] = [];
  const del = (i: number) => out.push({ kind: 'del', text: a[i], oldNo: i + 1 });
  const add = (j: number) => out.push({ kind: 'add', text: b[j], newNo: j + 1 });
  if (n === 0 || m === 0 || n * m > MAX_CELLS) {
    for (let i = start; i < endA; i++) del(i);
    for (let j = start; j < endB; j++) add(j);
    return out;
  }
  // Longest common subsequence of the changed middle, walked back to front.
  const width = m + 1;
  const table = new Uint32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i * width + j] = a[start + i] === b[start + j]
        ? table[(i + 1) * width + j + 1] + 1
        : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[start + i] === b[start + j]) {
      out.push({ kind: 'same', text: a[start + i], oldNo: start + i + 1, newNo: start + j + 1 });
      i++; j++;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      del(start + i++);
    } else {
      add(start + j++);
    }
  }
  while (i < n) del(start + i++);
  while (j < m) add(start + j++);
  return out;
}

/** The changed regions with `context` unchanged lines around each; regions that touch are one hunk. */
export function hunksOf(before: string, after: string, context = 2): Hunk[] {
  const lines = diffLines(before, after);
  const keep = new Array<boolean>(lines.length).fill(false);
  lines.forEach((line, idx) => {
    if (line.kind === 'same') return;
    for (let k = Math.max(0, idx - context); k <= Math.min(lines.length - 1, idx + context); k++) keep[k] = true;
  });
  const hunks: Hunk[] = [];
  let current: DiffLine[] | null = null;
  lines.forEach((line, idx) => {
    if (!keep[idx]) { current = null; return; }
    if (!current) { current = []; hunks.push({ lines: current }); }
    current.push(line);
  });
  return hunks;
}

/** Added and removed line counts — the `+3 −3` beside a file. */
export function changeCounts(before: string, after: string): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const l of diffLines(before, after)) {
    if (l.kind === 'add') added++;
    else if (l.kind === 'del') removed++;
  }
  return { added, removed };
}
