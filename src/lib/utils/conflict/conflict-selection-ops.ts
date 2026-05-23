// ── Conflict selection ops (mode-agnostic) ──────────────────────────────────
//
// All the per-region / bulk / manual-edit operations the conflict editor
// performs on a *file's* selection state, expressed as pure functions that
// return the next file-data record. Both `merge` and `stash blocking` modes
// were carrying parallel copies of this logic (toggleBlockingOurs vs
// toggleMergeOurs, …). Collapsing them here gives one bug-fixable surface.
//
// Convention: the caller owns a `Record<path, FileSelectionState>` map keyed
// by file path. Each op clones the entry it touches, applies the patch, and
// resets `manualResult` to `null` (any selection change invalidates the
// user-typed override). The consumer then reassigns the map so Svelte's
// reactivity sees the change.

import type { ConflictRegion, Region } from './region-types';
import type { SelectionMap } from './conflict-display';

export interface FileSelectionState {
  regions:        Region[];
  oursSelected:   SelectionMap;
  theirsSelected: SelectionMap;
  /** Non-null = user manually typed the result, bypassing per-line selection. */
  manualResult:   string | null;
  // Consumers may extend this with additional per-file context (e.g.
  // `content`, encoding labels, …); ops here only touch the fields above.
  [k: string]: unknown;
}

type Map<T extends FileSelectionState> = Record<string, T>;

function findRegion(regions: Region[], id: number): ConflictRegion | undefined {
  return regions.find((r): r is ConflictRegion => r.kind === 'conflict' && r.id === id);
}

/** Flip a single ours/theirs checkbox in a region. */
export function toggleLine<T extends FileSelectionState>(
  map: Map<T>, path: string, side: 'ours' | 'theirs', regionId: number, lineIdx: number,
): Map<T> {
  const d = map[path]; if (!d) return map;
  const key = side === 'ours' ? 'oursSelected' : 'theirsSelected';
  const arr = [...(d[key][regionId] ?? [])];
  arr[lineIdx] = !arr[lineIdx];
  return { ...map, [path]: { ...d, [key]: { ...d[key], [regionId]: arr }, manualResult: null } };
}

/** Take all lines of one side of a single conflict region, clearing the other. */
export function acceptSide<T extends FileSelectionState>(
  map: Map<T>, path: string, regionId: number, side: 'ours' | 'theirs',
): Map<T> {
  const d = map[path]; if (!d) return map;
  const r = findRegion(d.regions, regionId); if (!r) return map;
  return {
    ...map,
    [path]: {
      ...d,
      oursSelected:   { ...d.oursSelected,   [regionId]: r.oursLines.map(()   => side === 'ours') },
      theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => side === 'theirs') },
      manualResult: null,
    },
  };
}

/** Take both sides of a single conflict region. */
export function acceptBoth<T extends FileSelectionState>(
  map: Map<T>, path: string, regionId: number,
): Map<T> {
  const d = map[path]; if (!d) return map;
  const r = findRegion(d.regions, regionId); if (!r) return map;
  return {
    ...map,
    [path]: {
      ...d,
      oursSelected:   { ...d.oursSelected,   [regionId]: r.oursLines.map(()   => true) },
      theirsSelected: { ...d.theirsSelected, [regionId]: r.theirsLines.map(() => true) },
      manualResult: null,
    },
  };
}

/** Master checkbox: flag/unflag every line on one side across the whole file. */
export function setAllSide<T extends FileSelectionState>(
  map: Map<T>, path: string, side: 'ours' | 'theirs', checked: boolean,
): Map<T> {
  const d = map[path]; if (!d) return map;
  const next: SelectionMap = {};
  for (const r of d.regions) {
    if (r.kind !== 'conflict') continue;
    const lines = side === 'ours' ? r.oursLines : r.theirsLines;
    next[r.id] = lines.map(() => checked);
  }
  const patch = side === 'ours' ? { oursSelected: next } : { theirsSelected: next };
  return { ...map, [path]: { ...d, ...patch, manualResult: null } };
}

/** User typed something into the result textarea — overrides computed result. */
export function setManualResult<T extends FileSelectionState>(
  map: Map<T>, path: string, value: string,
): Map<T> {
  const d = map[path]; if (!d) return map;
  return { ...map, [path]: { ...d, manualResult: value } };
}

/** Clear the manual override; computed result takes over again. */
export function resetManualResult<T extends FileSelectionState>(
  map: Map<T>, path: string,
): Map<T> {
  const d = map[path]; if (!d) return map;
  return { ...map, [path]: { ...d, manualResult: null } };
}
