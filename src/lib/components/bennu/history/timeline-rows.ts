/**
 * One row model for the timeline, whatever scope produced it.
 *
 * A file's history is a list of revisions; a folder's is a list of operations. Rendering
 * those with two components would mean two places to fix the day a row grows a label
 * chip — so both are flattened to this, and the column stays one component.
 */

import type { ChangeGroup, Revision, RevisionKind } from '$lib/ipc/bennu/history';

export interface TimelineRow {
  /** Stable within the list — the revision id, or the change-set id. */
  id: string;
  at: number;
  kind: RevisionKind;
  title?: string;
  label?: string;
  /** Absolute path of the file this row diffs when selected. */
  file: string;
  /** The revision id on that file. */
  revision: string;
  /** How many files the operation touched. `1` for a plain save. */
  files: number;
  /** Content size, when the row stands for exactly one revision. */
  size?: number;
}

/** A single file's revisions as timeline rows. */
export function rowsFromRevisions(file: string, revisions: Revision[]): TimelineRow[] {
  return revisions.map((r) => ({
    id: r.id,
    at: r.at,
    kind: r.kind,
    title: r.title,
    label: r.label,
    file,
    revision: r.id,
    files: 1,
    size: r.size,
  }));
}

/** A folder's operations as timeline rows. `abs` turns the backend's project-relative
 *  paths into the absolute ones every other call takes. */
export function rowsFromGroups(groups: ChangeGroup[], abs: (rel: string) => string): TimelineRow[] {
  return groups
    .filter((g) => g.files.length > 0)
    .map((g) => ({
      id: g.id,
      at: g.at,
      kind: g.kind,
      title: g.title,
      label: g.label,
      // The first file is what the diff opens on; the others are reachable from the
      // folder column, which highlights every file the row touched.
      file: abs(g.files[0].path),
      revision: g.files[0].revision,
      files: g.files.length,
    }));
}
