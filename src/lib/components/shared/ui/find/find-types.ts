/**
 * The data shapes a find-results surface is fed with.
 *
 * Product-agnostic on purpose: a "group" is a file, a note, a table — whatever the host
 * groups its hits under — and a "hit" is one line of it. The host maps its own records onto
 * these and keeps the originals; every callback hands back the flat hit **index**, which is
 * how the host finds its record again.
 */

/** A run of a hit's text, either matched or not. Rendered as `<mark>` or as itself. */
export interface FindSegment {
  text: string;
  hit: boolean;
}

export interface FindHitItem {
  /** Stable identity, for list reconciliation. */
  key: string;
  /** Where in the group (`12:4`). Omitted when the hit has no position, e.g. a note excerpt. */
  location?: string;
  /** The line shown. Single-line: the list is windowed, so a row is one fixed height. */
  text: string;
}

export interface FindGroup {
  key: string;
  /** The group's name (file name, note title). */
  label: string;
  /** Muted trail after the name (folder). Truncated from the left, so the tail stays visible. */
  detail?: string;
  /** Small pill before the count (module, artifact). */
  badge?: string;
  /** Hover text for the header row. */
  tooltip?: string;
  /** Shown at the right of the header; omitted when it adds nothing. */
  count?: number;
  /** Read-only content (a dependency): tinted, because you can read it and not change it. */
  external?: boolean;
  hits: FindHitItem[];
}

/** A message standing where the list would be. */
export interface FindNotice {
  message: string;
  description?: string;
}
