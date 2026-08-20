/**
 * What a revision's kind looks like on a row.
 *
 * One table, because the same vocabulary appears in the timeline, in the deleted list
 * and in the folder column — and three copies of it would be three chances for a
 * refactor to be purple in one place and grey in another.
 */

import type { RevisionKind } from '$lib/ipc/bennu/history';

/** The chip drawn beside a row. `tone` names an accent, not a colour, so a theme change
 *  moves all of them at once. */
export interface KindMeta {
  label: string;
  tone: 'muted' | 'accent' | 'warning' | 'tag' | 'error' | 'success';
}

const TABLE: Record<RevisionKind, KindMeta> = {
  created:    { label: 'baseline',  tone: 'muted' },
  saved:      { label: 'save',      tone: 'muted' },
  external:   { label: 'external',  tone: 'accent' },
  refactored: { label: 'refactor',  tone: 'tag' },
  renamed:    { label: 'renamed',   tone: 'tag' },
  deleted:    { label: 'deleted',   tone: 'error' },
};

export function kindMeta(kind: RevisionKind): KindMeta {
  return TABLE[kind] ?? TABLE.saved;
}

/** The sentence a row leads with, given what a tool said it was doing. Falls back to the
 *  kind, so a row is never blank. */
export function revisionTitle(kind: RevisionKind, title?: string): string {
  if (title) return title;
  switch (kind) {
    case 'created':    return 'First known content';
    case 'saved':      return 'Saved';
    case 'external':   return 'Changed outside Bennu';
    case 'refactored': return 'Changed by a refactoring';
    case 'renamed':    return 'Renamed';
    case 'deleted':    return 'Deleted';
  }
}
