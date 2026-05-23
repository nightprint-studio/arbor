// ── LCS-based conflict diff ──────────────────────────────────────────────────
//
// Used by stash blocking mode where there are no `<<<<<<<` markers: we have
// the user's current workdir content and the stash version, and need to
// surface the differing line groups so the user can compose a merge per-line.
//
// Algorithm: classic O(n·m) LCS DP + backtracking that emits a stream of
// keep/remove/add ops. Adjacent removes are bucketed as "ours" lines, adjacent
// adds as "theirs" lines, and any equality run flushes the current bucket to
// emit a `conflict` region.
//
// Performance guard: above 250k cells (n·m) we don't bother with DP — the
// editor would freeze on the resulting render anyway. We fall back to a single
// whole-file region so the user can still see both sides and choose.

import type { Region } from './region-types';

const MAX_DP_CELLS = 250_000;

export function computeDiff(a: string | null, b: string | null): Region[] {
  const aLines = (a ?? '').split('\n');
  const bLines = (b ?? '').split('\n');
  // Trim trailing empty line from the .split('\n') artefact.
  if (aLines.at(-1) === '') aLines.pop();
  if (bLines.at(-1) === '') bLines.pop();

  if (aLines.length === 0 && bLines.length === 0) return [];
  if (aLines.length === 0) {
    return [{ kind: 'conflict', id: 0, oursLines: [], theirsLines: bLines, oursLabel: 'Corrente', theirsLabel: 'Stash' }];
  }
  if (bLines.length === 0) {
    return [{ kind: 'conflict', id: 0, oursLines: aLines, theirsLines: [], oursLabel: 'Corrente', theirsLabel: 'Stash' }];
  }

  // Bail out on pathologically large files: degrade to a single full-file
  // region rather than freeze the renderer trying to DP it.
  if (aLines.length * bLines.length > MAX_DP_CELLS) {
    return [{ kind: 'conflict', id: 0, oursLines: aLines, theirsLines: bLines, oursLabel: 'Corrente', theirsLabel: 'Stash' }];
  }

  const n = aLines.length, m = bLines.length;
  const dp = Array.from({ length: n + 1 }, () => new Int32Array(m + 1));
  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      dp[i][j] = aLines[i-1] === bLines[j-1]
        ? dp[i-1][j-1] + 1
        : Math.max(dp[i-1][j], dp[i][j-1]);
    }
  }

  type Op = { k: 'eq' | 'rm' | 'add'; l: string };
  const ops: Op[] = [];
  let i = n, j = m;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && aLines[i-1] === bLines[j-1]) {
      ops.push({ k: 'eq',  l: aLines[i-1] }); i--; j--;
    } else if (j > 0 && (i === 0 || dp[i][j-1] >= dp[i-1][j])) {
      ops.push({ k: 'add', l: bLines[j-1] }); j--;
    } else {
      ops.push({ k: 'rm',  l: aLines[i-1] }); i--;
    }
  }
  ops.reverse();

  const result: Region[] = [];
  let ctx: string[] = [];
  let rmLines: string[] = [];
  let addLines: string[] = [];
  let id = 0;

  function flush() {
    if (rmLines.length === 0 && addLines.length === 0) return;
    if (ctx.length) { result.push({ kind: 'context', lines: [...ctx] }); ctx = []; }
    result.push({ kind: 'conflict', id: id++, oursLines: [...rmLines], theirsLines: [...addLines], oursLabel: 'Corrente', theirsLabel: 'Stash' });
    rmLines = []; addLines = [];
  }

  for (const op of ops) {
    if (op.k === 'eq')       { flush(); ctx.push(op.l); }
    else if (op.k === 'rm')  rmLines.push(op.l);
    else                     addLines.push(op.l);
  }
  flush();
  if (ctx.length) result.push({ kind: 'context', lines: [...ctx] });
  return result;
}
