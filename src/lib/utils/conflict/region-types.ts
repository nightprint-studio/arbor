// ── Conflict region & display item types ────────────────────────────────────
//
// Shared shapes used by the conflict-resolution UI and its pure-logic helpers
// (`conflict-diff`, `conflict-marker-parser`, `conflict-display`,
// `conflict-selection-state`). Lives in its own module so both the .svelte
// components and the .ts utilities can depend on it without a circular import.

/** Regions are the building block of a side-by-side conflict view: alternating
 *  shared context blocks and per-side conflict blocks the user composes from. */
export type ContextRegion  = { kind: 'context'; lines: string[] };

export type ConflictRegion = {
  kind: 'conflict';
  id: number;
  oursLines:   string[];
  theirsLines: string[];
  oursLabel:   string;
  theirsLabel: string;
};

export type Region = ContextRegion | ConflictRegion;

// ── Display items ──────────────────────────────────────────────────────────
//
// `Region[]` is the *logical* model. For rendering we walk it once, compute
// running line numbers per side, and emit a flat `DisplayItem[]` the template
// can consume row-by-row. Huge context blocks get clipped to a head/tail with
// a `collapsed` placeholder in the middle to keep the DOM small.

export type ContextDisplayItem = {
  kind: 'context';
  lines: string[];
  oursStart: number;
  theirsStart: number;
};

export type ConflictDisplayItem = {
  kind: 'conflict';
  regionId: number;
  oursLines: string[];
  theirsLines: string[];
  oursStart: number;
  theirsStart: number;
  oursSelected: boolean[];
  theirsSelected: boolean[];
};

/** Placeholder row for context blocks too big to render fully. Click expands
 *  the hidden lines on demand. */
export type CollapsedContextDisplayItem = {
  kind: 'collapsed';
  contextKey: string;
  hiddenLines: number;
  oursStart: number;
  theirsStart: number;
};

export type DisplayItem =
  | ContextDisplayItem
  | ConflictDisplayItem
  | CollapsedContextDisplayItem;
