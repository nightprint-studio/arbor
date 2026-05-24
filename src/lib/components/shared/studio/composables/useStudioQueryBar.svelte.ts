/**
 * useStudioQueryBar — query-bar state shared by every Studio modal.
 * Owns the bound state (`query / queryHits / querying / queryError /
 * currentHitIdx`), the known-key set used by the path-segment
 * autocomplete, and the auto-open dismissal for the Query results
 * sidecar.
 *
 * Tree-pane accessors (`getChildKeysForPath` / `ensureChildrenLoadedForPath`
 * / `jumpToQueryHit`) are passed via config — the composable doesn't
 * touch the `<StudioTreePane>` controller directly so the wrapper can
 * decide when the controller is ready.
 */

import type { StudioQueryHit } from '$lib/ipc/studio-format';

export interface QueryBarController {
  focus(): void;
  clear(): void;
  nav(d: number): void;
  getHitCount(): number;
}

export interface QueryBarConfig<TKind extends string> {
  /** Read the active right pane — the composable auto-opens the query
   *  sidecar on first non-empty query. */
  getRightPane: () => string | null;
  /** Switch the right pane to a target. Used to auto-open the query
   *  sidecar when the user types. */
  setRightPane: (pane: 'query') => void;
  /** Toggle the query sidecar — wired to the right-rail button. */
  toggleQueryPane: () => void;
  /** Suppress unused-generic warnings — TKind threads through `queryHits`. */
  _phantom?: TKind;
}

export interface QueryBar<TKind extends string> {
  // Bound state — wrapper passes these to `<StudioQueryBar bind:*>`.
  query: string;
  queryHits: StudioQueryHit<TKind>[];
  queryError: string | null;
  querying: boolean;
  currentHitIdx: number;
  queryBar: QueryBarController | undefined;

  // Internal state.
  readonly knownKeys: Set<string>;
  noteKeys(items: { path: string[]; key?: string }[]): void;

  // Wiring helpers.
  onQueryActiveChange(active: boolean): void;
  onQueryToggleRightPane(): void;

  /** Reset transient state on doc close. */
  resetForDocClose(): void;
}

export function useStudioQueryBar<TKind extends string>(config: QueryBarConfig<TKind>): QueryBar<TKind> {
  let knownKeys = $state<Set<string>>(new Set());
  function noteKeys(items: { path: string[]; key?: string }[]) {
    if (items.length === 0) return;
    const next = new Set(knownKeys);
    let changed = false;
    for (const it of items) {
      const candidates: string[] = [];
      if (it.key && !/^\d+$/.test(it.key)) candidates.push(it.key);
      for (const seg of it.path) if (!/^\d+$/.test(seg)) candidates.push(seg);
      for (const c of candidates) if (!next.has(c)) { next.add(c); changed = true; }
    }
    if (changed) knownKeys = next;
  }

  let queryBar: QueryBarController | undefined = $state();
  let query         = $state('');
  let queryHits     = $state<StudioQueryHit<TKind>[]>([]);
  let queryError    = $state<string | null>(null);
  let querying      = $state(false);
  let currentHitIdx = $state(0);

  let queryAutoOpenDismissed = $state(false);

  function onQueryActiveChange(active: boolean): void {
    if (active && config.getRightPane() !== 'query' && !queryAutoOpenDismissed) {
      config.setRightPane('query');
    }
    if (!active) queryAutoOpenDismissed = false;
  }
  function onQueryToggleRightPane(): void {
    if (config.getRightPane() === 'query') queryAutoOpenDismissed = true;
    config.toggleQueryPane();
  }

  function resetForDocClose(): void {
    query                 = '';
    queryHits             = [];
    queryError            = null;
    currentHitIdx         = 0;
    knownKeys             = new Set();
    queryAutoOpenDismissed = false;
  }

  return {
    get query()           { return query; },
    set query(v: string)  { query = v; },
    get queryHits()       { return queryHits; },
    set queryHits(v)      { queryHits = v; },
    get queryError()      { return queryError; },
    set queryError(v)     { queryError = v; },
    get querying()        { return querying; },
    set querying(v)       { querying = v; },
    get currentHitIdx()       { return currentHitIdx; },
    set currentHitIdx(v: number) { currentHitIdx = v; },
    get queryBar()        { return queryBar; },
    set queryBar(v)       { queryBar = v; },
    get knownKeys()       { return knownKeys; },
    noteKeys,
    onQueryActiveChange,
    onQueryToggleRightPane,
    resetForDocClose,
  };
}
