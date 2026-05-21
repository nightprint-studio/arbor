import type { DiffFile, DiffHunk } from '$lib/types/git';

// Key format used throughout: `${hunkIdx}:${lineIdx}`
export type LineKey = string;

export function lineKey(hunkIdx: number, lineIdx: number): LineKey {
  return `${hunkIdx}:${lineIdx}`;
}

/** All changeable (added/removed) line keys for a given hunk. */
export function hunkLineKeys(hunkIdx: number, hunk: DiffHunk): LineKey[] {
  return hunk.lines
    .map((l, li) => (l.kind !== 'context' ? lineKey(hunkIdx, li) : null))
    .filter((k): k is LineKey => k !== null);
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function lineContent(raw: string): string {
  return raw.endsWith('\n') ? raw : raw + '\n';
}

function buildDiffHeader(path: string, oldPath?: string): string {
  const a = oldPath ?? path;
  const b = path;
  return `diff --git a/${a} b/${b}\n--- a/${a}\n+++ b/${b}\n`;
}

// ---------------------------------------------------------------------------
// Patch builder
//
// direction = 'stage'   — workdir → index
//   selected '+' → include as '+' (add to index)
//   selected '-' → include as '-' (remove from index)
//   non-selected '+' → skip (stays only in workdir)
//   non-selected '-' → convert to ' ' context (keep in index)
//
// direction = 'unstage' — partial revert index → HEAD
//   selected '+' (staged addition)  → '-' (remove from index)
//   selected '-' (staged deletion)  → '+' (restore in index)
//   non-selected '+' → ' ' context (keep staged)
//   non-selected '-' → skip (stay deleted)
//
// In "Show full file" mode, file.hunks contains a single giant hunk with
// every file line as context. A naive emission of that as a single hunk
// produces a patch with thousands of context lines that libgit2 chokes
// on (or whose context simply doesn't match the index byte-for-byte for
// every line). To stay robust we always compress: scan for runs of
// non-context emitted lines, surround each run with up to CONTEXT_RADIUS
// real context lines, and merge adjacent runs whose context overlaps.
// ---------------------------------------------------------------------------

const CONTEXT_RADIUS = 3;

type EmittedAction = ' ' | '+' | '-';
interface Emitted {
  action: EmittedAction;
  content: string;
  /** old-side line number (1-based) for this row, or null when '+' added. */
  oldLn: number | null;
  /** new-side line number (1-based) for this row, or null when '-' removed. */
  newLn: number | null;
}

function blockHasAddedSelected(hunk: DiffHunk, hi: number, selected: Set<LineKey>): boolean[] {
  // Mark every position inside a non-context block whose block contains at
  // least one selected 'added' line — used to auto-pair the matching
  // 'removed' lines so a modification is staged/unstaged in one click.
  const result = new Array(hunk.lines.length).fill(false);
  let i = 0;
  while (i < hunk.lines.length) {
    if (hunk.lines[i].kind !== 'context') {
      const blockStart = i;
      let hasAddedSel = false;
      while (i < hunk.lines.length && hunk.lines[i].kind !== 'context') {
        if (hunk.lines[i].kind === 'added' && selected.has(lineKey(hi, i))) hasAddedSel = true;
        i++;
      }
      for (let j = blockStart; j < i; j++) result[j] = hasAddedSel;
    } else {
      i++;
    }
  }
  return result;
}

/**
 * Translate one diff line into a patch row, or null when the line is
 * intentionally absent from the patch (e.g. unselected '+' in stage mode).
 */
function emitLine(
  line: DiffHunk['lines'][number],
  hi: number,
  li: number,
  selected: Set<LineKey>,
  pairedViaPlus: boolean,
  isStage: boolean,
): Emitted | null {
  const content = lineContent(line.content);
  const sel = selected.has(lineKey(hi, li));

  if (line.kind === 'context') {
    return { action: ' ', content, oldLn: line.old_lineno ?? null, newLn: line.new_lineno ?? null };
  }
  if (line.kind === 'added') {
    if (isStage) {
      return sel
        ? { action: '+', content, oldLn: null, newLn: line.new_lineno ?? null }
        : null; // unselected added → not in patch
    }
    // unstage direction: in the staged diff this added line is in old (index).
    // libgit2's old_lineno for 'added' is None; reuse new_lineno for both
    // sides since the staged additions occupy the same line numbers when
    // we narrate the index→HEAD direction.
    const ln = line.new_lineno ?? null;
    return sel
      ? { action: '-', content, oldLn: ln, newLn: null }
      : { action: ' ', content, oldLn: ln, newLn: ln };
  }
  // removed
  if (isStage) {
    return sel || pairedViaPlus
      ? { action: '-', content, oldLn: line.old_lineno ?? null, newLn: null }
      : { action: ' ', content, oldLn: line.old_lineno ?? null, newLn: line.old_lineno ?? null };
  }
  // unstage
  return sel || pairedViaPlus
    ? { action: '+', content, oldLn: null, newLn: line.old_lineno ?? null }
    : null; // unselected removed in unstage → already gone, not in patch
}

/**
 * Compress the emitted line stream into a list of mini-hunks: each block
 * of non-context rows surrounded by up to CONTEXT_RADIUS context rows on
 * each side, with overlapping/adjacent blocks merged into one mini-hunk.
 *
 * Returns a list of `[startIdx, endIdx)` slice bounds into `emitted`.
 */
function findMiniHunks(emitted: Emitted[]): { start: number; end: number }[] {
  // First pass: indexes of all non-context rows.
  const changeIdxs: number[] = [];
  for (let i = 0; i < emitted.length; i++) {
    if (emitted[i].action !== ' ') changeIdxs.push(i);
  }
  if (changeIdxs.length === 0) return [];

  // Second pass: build slices [first - radius, last + radius + 1], then
  // merge adjacent/overlapping ones.
  const slices: { start: number; end: number }[] = [];
  for (const idx of changeIdxs) {
    const start = Math.max(0, idx - CONTEXT_RADIUS);
    const end   = Math.min(emitted.length, idx + CONTEXT_RADIUS + 1);
    const last  = slices[slices.length - 1];
    if (last && start <= last.end) {
      last.end = Math.max(last.end, end);
    } else {
      slices.push({ start, end });
    }
  }
  return slices;
}

function emitMiniHunk(slice: Emitted[], lineOffset: number): { header: string; body: string; netDelta: number } | null {
  if (slice.length === 0) return null;
  // The first non-'+' row carries the index-side line number, which drives
  // oldStart. newStart is derived from oldStart + cumulative line offset
  // produced by previously emitted mini-hunks — this matches how libgit2
  // stages multi-hunk patches sequentially.
  let oldStart: number | null = null;
  for (const e of slice) {
    if (e.oldLn !== null) { oldStart = e.oldLn; break; }
  }
  let oldCount = 0, newCount = 0;
  let body = '';
  for (const e of slice) {
    body += `${e.action}${e.content}`;
    if (e.action === ' ') { oldCount++; newCount++; }
    else if (e.action === '-') { oldCount++; }
    else { newCount++; }
  }
  // Pure addition (no context, no '-'): the patch inserts at the start of
  // the file → -0,0. The new lines occupy 1..newCount.
  const os = oldStart ?? (oldCount === 0 ? 0 : 1);
  const ns = oldCount === 0 && oldStart === null
    ? 1                          // pure prepend / whole-file addition
    : os + lineOffset;
  const header = `@@ -${os},${oldCount} +${ns},${newCount} @@\n`;
  return { header, body, netDelta: newCount - oldCount };
}

function buildPatch(file: DiffFile, selected: Set<LineKey>, direction: 'stage' | 'unstage'): string {
  const isStage = direction === 'stage';
  let patch = buildDiffHeader(file.path, file.old_path);
  let hasChanges = false;
  let lineOffset = 0;

  for (let hi = 0; hi < file.hunks.length; hi++) {
    const hunk = file.hunks[hi];
    const blockAddedSel = blockHasAddedSelected(hunk, hi, selected);

    const emitted: Emitted[] = [];
    for (let li = 0; li < hunk.lines.length; li++) {
      const line = hunk.lines[li];
      const sel = selected.has(lineKey(hi, li));
      const paired = !sel && line.kind === 'removed' && blockAddedSel[li];
      const e = emitLine(line, hi, li, selected, paired, isStage);
      if (e) emitted.push(e);
    }

    for (const slice of findMiniHunks(emitted)) {
      const mh = emitMiniHunk(emitted.slice(slice.start, slice.end), lineOffset);
      if (!mh) continue;
      patch += mh.header + mh.body;
      hasChanges = true;
      lineOffset += mh.netDelta;
    }
  }

  return hasChanges ? patch : '';
}

// ---------------------------------------------------------------------------
// Public API — same signature as before
// ---------------------------------------------------------------------------

export function buildStagePatch(file: DiffFile, selected: Set<LineKey>): string {
  return buildPatch(file, selected, 'stage');
}

export function buildUnstagePatch(file: DiffFile, selected: Set<LineKey>): string {
  return buildPatch(file, selected, 'unstage');
}
