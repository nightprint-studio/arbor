// ── Conflict-marker parser ──────────────────────────────────────────────────
//
// Reads a working-copy file that git has annotated with conflict markers and
// turns it into a `Region[]`:
//
//   <<<<<<< OURS_LABEL          ← begin conflict; switch to ours bucket
//   ours line A
//   =======                     ← switch to theirs bucket
//   theirs line A
//   theirs line B
//   >>>>>>> THEIRS_LABEL        ← close conflict; emit ConflictRegion
//
// Labels after the markers (everything after `<<<<<<< ` / `>>>>>>> `) are kept
// as `oursLabel` / `theirsLabel` so the column headers and action buttons can
// surface them; when missing we fall back to `HEAD` / `THEIRS` (merge mode) or
// `HEAD` / `STASH` (stash mode). The `mode` parameter picks the default.

import type { Region } from './region-types';

export type ConflictMode = 'merge' | 'stash';

export function parseConflicts(raw: string, mode: ConflictMode = 'merge'): Region[] {
  const lines = raw.split('\n');
  const result: Region[] = [];
  let ctx: string[] = [];
  let state: 'context' | 'ours' | 'theirs' = 'context';
  let oursLines: string[] = [], theirsLines: string[] = [];
  let oursLabel = 'HEAD';
  let theirsLabel = mode === 'merge' ? 'THEIRS' : 'STASH';
  let id = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    // Drop the trailing empty line that `split('\n')` synthesises when the
    // input ends with a newline.
    if (i === lines.length - 1 && line === '') continue;

    if      (state === 'context' && line.startsWith('<<<<<<<')) {
      if (ctx.length) { result.push({ kind: 'context', lines: [...ctx] }); ctx = []; }
      oursLabel = line.slice(8).trim() || 'HEAD';
      state = 'ours'; oursLines = [];
    } else if (state === 'ours'    && line.startsWith('=======')) {
      state = 'theirs'; theirsLines = [];
    } else if (state === 'theirs'  && line.startsWith('>>>>>>>')) {
      theirsLabel = line.slice(8).trim() || (mode === 'merge' ? 'THEIRS' : 'STASH');
      result.push({ kind: 'conflict', id: id++, oursLines: [...oursLines], theirsLines: [...theirsLines], oursLabel, theirsLabel });
      state = 'context';
    } else if (state === 'ours')   oursLines.push(line);
    else if   (state === 'theirs') theirsLines.push(line);
    else                           ctx.push(line);
  }

  if (ctx.length) result.push({ kind: 'context', lines: ctx });
  return result;
}
