/*
 * Shared rendering context handed down by PluginSidebarPanel to every
 * sub-renderer (SidebarNodeLayout, SidebarNodeCard, SidebarNodeField,
 * SidebarNodeViz, SidebarNodeConsole).
 *
 * Each `Map` lives as a `$state(new Map<>)` in the dispatcher. Svelte 5
 * does NOT track in-place `Map.set` on a proxied state value, so the
 * dispatcher reassigns the map on every mutation (`x = new Map(x); x.set(…)`).
 * Sub-renderers must NOT call `ctx.xxx.set(…)` directly — go through the
 * `set…` methods below, which perform the reassignment.
 */
export interface SidebarNodeCtx {
  pluginName: string;

  // ── reactive state (read-only views; mutate via setters below) ───────
  collapsed:          ReadonlyMap<string, boolean>;
  fieldDraft:         ReadonlyMap<string, unknown>;
  consoleDraft:       ReadonlyMap<string, string>;
  consoleHistoryIdx:  ReadonlyMap<string, number>;
  suggestVisible:     ReadonlyMap<string, boolean>;
  suggestActive:      ReadonlyMap<string, number>;
  copiedKey:          string | null;

  // ── pure helpers ─────────────────────────────────────────────────────
  nodeType: (n: unknown) => string;
  nodeKey:  (n: unknown, i: number) => string;
  fieldKey: (n: any, index: number) => string;
  fieldValue: (key: string, fallback: unknown) => unknown;

  isSectionCollapsed: (n: any, key: string) => boolean;
  toggleSection:      (key: string, current: boolean) => void;

  copyCode: (key: string, text: string) => Promise<void>;

  // ── action plumbing ──────────────────────────────────────────────────
  fireAction:         (action: string | undefined, payload?: Record<string, unknown>) => void;
  commitField:        (n: any, key: string, value: unknown) => void;
  commitColorChannel: (n: any, channel: string, value: number) => void;
  commitVecAxis:      (n: any, axis: string, value: number) => void;
  startVecDrag:       (node: any, axis: string, startValue: number, e: MouseEvent) => void;
  resetVecAxis:       (node: any, axis: string) => void;
  vecAxisValue:       (node: any, axis: string) => number;

  /** Update a field-draft entry (handles the Map-reassignment dance). */
  setFieldDraft:      (key: string, value: unknown) => void;

  // ── console_input ────────────────────────────────────────────────────
  consoleValue:       (key: string) => string;
  setConsoleValue:    (key: string, v: string) => void;
  setSuggestVisible:  (key: string, v: boolean) => void;
  setSuggestActive:   (key: string, v: number) => void;
  consoleMatches:     (n: any, text: string) => string[];
  acceptSuggestion:   (n: any, key: string, value: string) => void;
  submitConsole:      (n: any) => void;
  onConsoleKey:       (n: any, key: string, ev: KeyboardEvent) => void;
}
