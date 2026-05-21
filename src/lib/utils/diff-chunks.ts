import type { DiffFile } from '$lib/types/git';
import { lineKey } from './patch-builder';

export interface ChunkAnchor {
  hunkIdx: number;
  lineIdx: number;
  key: string;
  kind: 'added' | 'removed' | 'mixed';
}

/**
 * Walk every hunk's lines and return one anchor per contiguous run of
 * non-context lines (added/removed). Used by DiffViewer's prev/next chunk
 * navigation. The anchor points at the first line of the run; the line
 * carries `data-chunk-key={key}` so it can be located via querySelector.
 *
 * In default mode each git hunk usually has 1 chunk (its core change). In
 * full-file mode there is one giant hunk for the whole file, so the chunk
 * detection scans through it line-by-line to surface every change run.
 */
export function computeChunkAnchors(file: DiffFile | null): ChunkAnchor[] {
  if (!file) return [];
  const out: ChunkAnchor[] = [];
  for (let hi = 0; hi < file.hunks.length; hi++) {
    const hunk = file.hunks[hi];
    let runStart: number | null = null;
    let sawAdd = false;
    let sawDel = false;
    for (let li = 0; li < hunk.lines.length; li++) {
      const k = hunk.lines[li].kind;
      if (k === 'context') {
        if (runStart !== null) {
          out.push(makeAnchor(hi, runStart, sawAdd, sawDel));
          runStart = null; sawAdd = false; sawDel = false;
        }
      } else {
        if (runStart === null) runStart = li;
        if (k === 'added')   sawAdd = true;
        if (k === 'removed') sawDel = true;
      }
    }
    if (runStart !== null) {
      out.push(makeAnchor(hi, runStart, sawAdd, sawDel));
    }
  }
  return out;
}

function makeAnchor(hi: number, li: number, add: boolean, del: boolean): ChunkAnchor {
  return {
    hunkIdx: hi,
    lineIdx: li,
    key: lineKey(hi, li),
    kind: add && del ? 'mixed' : add ? 'added' : 'removed',
  };
}

/** Total hunk lines (context + added + removed) across the whole file. */
export function totalDiffLines(file: DiffFile | null): number {
  if (!file) return 0;
  let n = 0;
  for (const h of file.hunks) n += h.lines.length;
  return n;
}
