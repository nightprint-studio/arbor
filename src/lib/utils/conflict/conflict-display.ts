// ── Conflict display walker ─────────────────────────────────────────────────
//
// Walks a `Region[]` for a single file and produces a flat `DisplayItem[]`
// the template iterates over. Responsibilities:
//   1. Compute running line numbers per side (`oursStart` / `theirsStart`) —
//      kept synced because every region advances both sides by the same
//      amount for context, and by `oursLines.length` / `theirsLines.length`
//      respectively inside a conflict region.
//   2. Clip oversized context blocks (more than `CONTEXT_MAX` lines) to
//      `CONTEXT_HEAD_TAIL` head + tail with a `collapsed` placeholder in the
//      middle. Without this, files with thousands of context lines used to
//      freeze the renderer mounting per-line `<div>`s + Prism highlights.
//   3. Read selection state (`oursSelected` / `theirsSelected`) from the
//      caller-supplied maps, falling back to defaults if a region has no
//      explicit selection yet (no ours selected, all theirs selected — the
//      "accept theirs" default used by stash apply).
//
// The `fullFile` flag bypasses clipping entirely (drives the toolbar's "Show
// full file context" toggle). `expandedKeys` is the per-block escape hatch:
// users can expand individual collapsed blocks on demand.

import type { ConflictRegion, DisplayItem, Region } from './region-types';

export const CONTEXT_MAX       = 30;
export const CONTEXT_HEAD_TAIL = 12;

export type SelectionMap = Record<number, boolean[]>;

export interface BuildDisplayOptions {
  regions: Region[];
  oursSelected: SelectionMap;
  theirsSelected: SelectionMap;
  /** Stable per-file key — combined with the context index to form
   *  `expandedKeys` lookups so two files' context blocks don't collide. */
  fileKey: string;
  /** When true, never clip context blocks — emit them in full. */
  fullFile: boolean;
  expandedKeys: Set<string>;
}

export function buildDisplayItems(opts: BuildDisplayOptions): DisplayItem[] {
  const { regions, oursSelected, theirsSelected, fileKey, fullFile, expandedKeys } = opts;
  const out: DisplayItem[] = [];
  let oursN = 1, theirsN = 1, ctxIdx = 0;

  for (const r of regions) {
    if (r.kind === 'context') {
      emitContext(r.lines, oursN, theirsN, `${fileKey}|${ctxIdx++}`, fullFile, expandedKeys, out);
      oursN   += r.lines.length;
      theirsN += r.lines.length;
    } else {
      out.push({
        kind: 'conflict',
        regionId: r.id,
        oursLines: r.oursLines,
        theirsLines: r.theirsLines,
        oursStart: oursN,
        theirsStart: theirsN,
        oursSelected:   oursSelected[r.id]   ?? r.oursLines.map(() => false),
        theirsSelected: theirsSelected[r.id] ?? r.theirsLines.map(() => true),
      });
      oursN   += r.oursLines.length;
      theirsN += r.theirsLines.length;
    }
  }
  return out;
}

function emitContext(
  lines:        string[],
  oursStart:    number,
  theirsStart:  number,
  contextKey:   string,
  fullFile:     boolean,
  expandedKeys: Set<string>,
  into:         DisplayItem[],
) {
  if (fullFile || lines.length <= CONTEXT_MAX || expandedKeys.has(contextKey)) {
    into.push({ kind: 'context', lines, oursStart, theirsStart });
    return;
  }
  into.push({
    kind: 'context',
    lines: lines.slice(0, CONTEXT_HEAD_TAIL),
    oursStart, theirsStart,
  });
  into.push({
    kind: 'collapsed',
    contextKey,
    hiddenLines: lines.length - CONTEXT_HEAD_TAIL * 2,
    oursStart:   oursStart   + CONTEXT_HEAD_TAIL,
    theirsStart: theirsStart + CONTEXT_HEAD_TAIL,
  });
  const tailIdx = lines.length - CONTEXT_HEAD_TAIL;
  into.push({
    kind: 'context',
    lines: lines.slice(tailIdx),
    oursStart:   oursStart   + tailIdx,
    theirsStart: theirsStart + tailIdx,
  });
}

// ── Aggregate "side state" across all conflict regions of a file ───────────
//
// Drives the master checkbox in each column header:
//   'all'      → every line on this side is selected (checked, not indet.)
//   'none'     → no line on this side is selected (unchecked, not indet.)
//   'partial'  → mixed (checkbox renders as indeterminate)

export type SideState = 'all' | 'none' | 'partial';

export function computeSideState(
  regions: Region[],
  selection: { ours: SelectionMap; theirs: SelectionMap },
  side: 'ours' | 'theirs',
): SideState {
  let total = 0, sel = 0;
  for (const r of regions) {
    if (r.kind !== 'conflict') continue;
    const lines = side === 'ours' ? r.oursLines : r.theirsLines;
    const arr   = (side === 'ours' ? selection.ours : selection.theirs)[r.id] ?? [];
    for (let i = 0; i < lines.length; i++) {
      total++;
      if (arr[i]) sel++;
    }
  }
  if (total === 0 || sel === 0) return 'none';
  if (sel === total) return 'all';
  return 'partial';
}

// ── Initial selection presets ──────────────────────────────────────────────
//
// Default after loading a file: ours unchecked, theirs checked — i.e. "take
// the incoming version unless the user says otherwise". Matches both git's
// behaviour on stash apply (their version wins) and the merge UX where the
// user typically eyeballs the incoming diff and keeps it.

export function initBlockingSelections(regions: Region[]): {
  os: SelectionMap;
  ts: SelectionMap;
} {
  const os: SelectionMap = {};
  const ts: SelectionMap = {};
  for (const r of regions) {
    if (r.kind === 'conflict') {
      os[r.id] = r.oursLines.map(() => false);
      ts[r.id] = r.theirsLines.map(() => true);
    }
  }
  return { os, ts };
}

// ── Result computation ─────────────────────────────────────────────────────
//
// Builds the final merged text from a region list + per-region selections:
// context lines pass through verbatim, conflict regions emit the ours-then-
// theirs lines for which the user ticked the box. Lines are joined with
// `\n` — no trailing newline, matching git's expectation.
//
// Symmetric to `initBlockingSelections`: ours then theirs, in declared order.

export function computeRegionsResult(
  regions: Region[],
  selection: { ours: SelectionMap; theirs: SelectionMap },
): string {
  const lines: string[] = [];
  for (const r of regions) {
    if (r.kind === 'context') {
      lines.push(...r.lines);
    } else {
      const os = selection.ours[r.id]   ?? r.oursLines.map(() => false);
      const ts = selection.theirs[r.id] ?? r.theirsLines.map(() => true);
      r.oursLines.forEach((l, i)   => { if (os[i]) lines.push(l); });
      r.theirsLines.forEach((l, i) => { if (ts[i]) lines.push(l); });
    }
  }
  return lines.join('\n');
}

// Type guard helper used by the file-action bar to count navigable
// conflict blocks within a DisplayItem stream.
export function isConflictItem(d: DisplayItem): d is Extract<DisplayItem, { kind: 'conflict' }> {
  return d.kind === 'conflict';
}

// Re-export the conflict region type for callers that need to refine.
export type { ConflictRegion };
