/**
 * studioStore — file index backing the built-in Studio sidebar.
 *
 * Holds the result of the most recent `studio_scan_repo` call, the
 * per-tree expansion state, and a filter string. Loads lazily — the
 * sidebar panel calls `ensureLoadedFor(tabId)` on mount and on tab
 * switches, which skips the IPC when the same tab is already cached.
 *
 * Designed multi-format from the start: same store powers `.ron`, `.json`,
 * `.toml` (and any future kind we add to `StudioFileKind`). The kind
 * filter lives in the store so the sidebar's chip row toggles are
 * persistent across renders.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  studioScanRepo, studioScanCrossRefs, studioFindUsages, studioScanBrokenRefs,
  studioToggleExclude, studioBindSchema, studioUnbindSchema, studioGetConfig,
  studioRefreshIndex, getStudioSettings, setStudioSettings,
  studioToggleReferenceField,
  studioAddExternal, studioRemoveExternal,
  type StudioFileEntry, type StudioFileKind, type CrossRefDef, type UsageMatch,
  type BrokenRef, type StudioConfig, type StudioSettings,
  type IndexProgressEvent, type IndexDoneEvent,
} from '$lib/ipc/studio';

function createStudioStore() {
  let files       = $state<StudioFileEntry[]>([]);
  let loading     = $state(false);
  let error       = $state<string | null>(null);
  let loadedTabId = $state<string | null>(null);
  let lastLoadedAt = $state<number>(0);

  /** Active filter string (case-insensitive substring match on the path). */
  let filter = $state('');

  /** Kind filter — empty set means "show everything". When one or more
   *  kinds are toggled on, only those appear in the tree. */
  let activeKinds = $state<Set<StudioFileKind>>(new Set());

  /** Folder expansion — keyed by POSIX path relative to the repo root.
   *  Persisted only in-memory; tab switches clear it. */
  let expanded = $state<Set<string>>(new Set());

  /** When false (default) excluded files are hidden from the tree.
   *  Toggling on surfaces them with a struck-through label so the user
   *  can re-include them. */
  let showExcluded = $state(false);

  /** Cached snapshot of the repo's `.ron-studio.toml`. Refreshed inside
   *  `load()` so the panel can tell folder-level excludes (`foo/**`)
   *  apart from per-file ones without re-fetching. */
  let config = $state<StudioConfig>({ excludes: [], overrides: [] });

  /** Host-wide Studio tunables (persistent index toggle, …). Loaded
   *  lazily on first access — most callers don't care, the sidebar
   *  forces a load on mount via `ensureSettingsLoaded`. */
  let settings        = $state<StudioSettings>({ use_index: false });
  let settingsLoaded  = $state(false);

  /** Live progress of the background index-refresh job, when one is
   *  running. `null` when idle. Drives the "Indexing N/M…" badge in
   *  the Studio sidebar toolbar. */
  let indexProgress = $state<{ processed: number; total: number; tabId: string } | null>(null);
  let indexJobRunning = $state(false);

  let indexListeners: UnlistenFn[] = [];
  let listenersInstalled = false;

  /** Project-wide cross-reference index for `.ron` files. RON-only by
   *  default — JSON / TOML caches live in `*ByKind` below since Phase
   *  3.c. The RON-only fields stay to avoid churning every existing
   *  caller (RON modal, sidebar) while introducing the kind-aware API. */
  let crossRefs       = $state<Map<string, CrossRefDef[]>>(new Map());
  let crossRefsLoading = $state(false);
  let crossRefsTabId  = $state<string | null>(null);

  /** Project-wide broken-reference cache — every `*_id` / `*_ref`
   *  field whose value doesn't match any known def. Loaded lazily
   *  via `loadBrokenRefs`; survives tab switches by keying on the
   *  loading tab id. The cache is invalidated whenever the index
   *  job completes OR a config change tears down the cross-ref
   *  cache (broken refs depend on the same def-namespace). */
  let brokenRefs        = $state<BrokenRef[]>([]);
  let brokenRefsLoading = $state(false);
  let brokenRefsTabId   = $state<string | null>(null);

  /** Reverse-lookup cache (target id → matching reference fields). Keyed
   *  by `${tabId}::${target}` so it survives tab switches without
   *  cross-pollinating between repos. The query is on-demand — populated
   *  lazily by `loadUsages`. */
  let usagesCache   = $state<Map<string, UsageMatch[]>>(new Map());
  let usagesLoading = $state<Set<string>>(new Set());

  /** Per-kind cross-ref / broken-ref / usage caches (added Phase 3.c).
   *  RON keeps its own dedicated state (above) so the existing modal +
   *  sidebar continue to compile without change; non-RON formats route
   *  through these maps. Empty entries return `null`/`[]`/`new Map()` so
   *  callers don't have to null-check the kind itself. */
  let crossRefsByKind       = $state<Map<StudioFileKind, Map<string, CrossRefDef[]>>>(new Map());
  let crossRefsTabIdByKind  = $state<Map<StudioFileKind, string>>(new Map());
  let crossRefsLoadingByKind = $state<Set<StudioFileKind>>(new Set());
  let brokenRefsByKind      = $state<Map<StudioFileKind, BrokenRef[]>>(new Map());
  let brokenRefsTabIdByKind = $state<Map<StudioFileKind, string>>(new Map());
  let brokenRefsLoadingByKind = $state<Set<StudioFileKind>>(new Set());
  let usagesCacheByKind     = $state<Map<StudioFileKind, Map<string, UsageMatch[]>>>(new Map());
  let usagesLoadingByKind   = $state<Map<StudioFileKind, Set<string>>>(new Map());

  async function load(tabId: string): Promise<void> {
    loading = true;
    error   = null;
    try {
      // Files + config in parallel so the sidebar can render folder-
      // excluded badges without a second round-trip. Config failure is
      // soft — empty config is a valid state and just means "nothing
      // bound, nothing excluded".
      const empty: StudioConfig = { excludes: [], overrides: [] };
      const [next, cfg] = await Promise.all([
        studioScanRepo(tabId, []),
        studioGetConfig(tabId).catch(() => empty),
      ]);
      files        = next;
      // Normalise: the backend uses `skip_serializing_if = "Vec::is_empty"`,
      // so empty arrays come back as `undefined` over IPC. Default both
      // collection fields so iterate / `.includes` calls never NPE.
      config = {
        excludes:  Array.isArray(cfg?.excludes)  ? cfg.excludes  : [],
        overrides: Array.isArray(cfg?.overrides) ? cfg.overrides : [],
        default:   cfg?.default,
      };
      loadedTabId  = tabId;
      lastLoadedAt = Date.now();
    } catch (e) {
      files = [];
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /** Load the index for `tabId` if it hasn't been loaded yet or the cache
   *  belongs to a different tab. Re-mounting the sidebar shouldn't trigger
   *  a re-scan — that's wasteful on big repos. */
  async function ensureLoadedFor(tabId: string): Promise<void> {
    if (loadedTabId === tabId && files.length > 0) return;
    await load(tabId);
  }

  async function refresh(tabId: string): Promise<void> {
    expanded = new Set();  // reset expansion on explicit refresh
    await load(tabId);
  }

  function setFilter(v: string): void { filter = v; }

  async function ensureSettingsLoaded(): Promise<void> {
    if (settingsLoaded) return;
    try {
      settings = await getStudioSettings();
    } catch (e) {
      console.warn('studio settings load failed', e);
    } finally {
      settingsLoaded = true;
    }
  }

  async function updateSettings(next: StudioSettings): Promise<void> {
    settings = { ...next };
    try { await setStudioSettings(next); }
    catch (e) { console.warn('studio settings save failed', e); }
  }

  /** Subscribe to the background index job's progress + done events.
   *  Called once on first sidebar mount; idempotent. */
  async function installIndexListeners(): Promise<void> {
    if (listenersInstalled) return;
    listenersInstalled = true;
    try {
      indexListeners.push(
        await listen<IndexProgressEvent>('arbor://studio-index-progress', (e) => {
          const p = e.payload;
          indexProgress = { processed: p.processed, total: p.total, tabId: p.tab_id };
          indexJobRunning = p.processed < p.total;
        }),
        await listen<IndexDoneEvent>('arbor://studio-index-done', (e) => {
          indexJobRunning = false;
          indexProgress = null;
          // After a successful build, the cross-ref / usage / broken-ref
          // caches are stale — clear so the next query hits the fresh
          // index. Broken refs depend on BOTH defs and refs being
          // current, so they invalidate alongside cross-refs.
          crossRefs       = new Map();
          crossRefsTabId  = null;
          usagesCache     = new Map();
          usagesLoading   = new Set();
          brokenRefs      = [];
          brokenRefsTabId = null;
          crossRefsByKind       = new Map();
          crossRefsTabIdByKind  = new Map();
          brokenRefsByKind      = new Map();
          brokenRefsTabIdByKind = new Map();
          usagesCacheByKind     = new Map();
          usagesLoadingByKind   = new Map();
          // Refresh the file list too — entries may have appeared or
          // disappeared since the last scan.
          if (loadedTabId === e.payload.tab_id) {
            void load(e.payload.tab_id);
          }
        }),
      );
    } catch (e) {
      console.warn('studio index listener install failed', e);
    }
  }

  /** Trigger a background refresh of the persistent index for `tabId`.
   *  Returns immediately — progress flows through `indexProgress`. */
  async function refreshIndex(tabId: string): Promise<void> {
    if (indexJobRunning) return;
    indexJobRunning = true;
    indexProgress = { processed: 0, total: 0, tabId };
    try {
      await studioRefreshIndex(tabId);
    } catch (e) {
      console.warn('studio_refresh_index spawn failed', e);
      indexJobRunning = false;
      indexProgress = null;
    }
  }
  function setShowExcluded(v: boolean): void { showExcluded = v; }
  function toggleShowExcluded(): void { showExcluded = !showExcluded; }
  function setActiveKinds(next: Set<StudioFileKind>): void { activeKinds = new Set(next); }
  function toggleKind(k: StudioFileKind): void {
    const next = new Set(activeKinds);
    if (next.has(k)) next.delete(k); else next.add(k);
    activeKinds = next;
  }

  function toggleFolder(path: string): void {
    const next = new Set(expanded);
    if (next.has(path)) next.delete(path); else next.add(path);
    expanded = next;
  }
  function expandAll(): void {
    const next = new Set<string>();
    for (const f of files) {
      const segs = f.relative_path.split('/');
      for (let i = 1; i < segs.length; i++) {
        next.add(segs.slice(0, i).join('/'));
      }
    }
    expanded = next;
  }
  function collapseAll(): void { expanded = new Set(); }

  /** Forget any cached index — used when the active repo path changes
   *  out from under us (e.g. workspace switch). */
  function clear(): void {
    files        = [];
    loadedTabId  = null;
    lastLoadedAt = 0;
    expanded     = new Set();
    filter       = '';
    error        = null;
    crossRefs    = new Map();
    crossRefsTabId = null;
    usagesCache   = new Map();
    usagesLoading = new Set();
    brokenRefs    = [];
    brokenRefsTabId = null;
    crossRefsByKind       = new Map();
    crossRefsTabIdByKind  = new Map();
    brokenRefsByKind      = new Map();
    brokenRefsTabIdByKind = new Map();
    usagesCacheByKind     = new Map();
    usagesLoadingByKind   = new Map();
  }

  /** Build (or refresh) the project-wide cross-reference index for
   *  `tabId`. Lightweight when the same tab is already indexed and
   *  `force` is false — useful to call from $effects without thrashing. */
  async function loadCrossRefs(tabId: string, force: boolean = false): Promise<void> {
    if (!force && crossRefsTabId === tabId && crossRefs.size > 0) return;
    crossRefsLoading = true;
    try {
      const defs = await studioScanCrossRefs(tabId);
      const grouped = new Map<string, CrossRefDef[]>();
      for (const d of defs) {
        const arr = grouped.get(d.id_value);
        if (arr) arr.push(d);
        else     grouped.set(d.id_value, [d]);
      }
      crossRefs       = grouped;
      crossRefsTabId  = tabId;
      // A forced reload typically follows a save, which may have moved
      // a reference somewhere else — usage cache becomes suspect.
      if (force) {
        usagesCache   = new Map();
        usagesLoading = new Set();
      }
    } catch (e) {
      // Cross-ref is a soft feature — log and move on so a single
      // parse failure doesn't block the modal from opening.
      console.warn('studio cross-ref scan failed', e);
    } finally {
      crossRefsLoading = false;
    }
  }

  /** Kind-aware variant — fans out to `crossRefsByKind`. The RON kind
   *  routes through the legacy `loadCrossRefs` so the existing modal
   *  state stays in one place. */
  async function loadCrossRefsForKind(
    tabId: string,
    kind:  StudioFileKind,
    force: boolean = false,
  ): Promise<void> {
    if (kind === 'ron') { await loadCrossRefs(tabId, force); return; }
    const existing = crossRefsByKind.get(kind);
    const cachedTab = crossRefsTabIdByKind.get(kind);
    if (!force && cachedTab === tabId && existing && existing.size > 0) return;
    const next = new Set(crossRefsLoadingByKind); next.add(kind); crossRefsLoadingByKind = next;
    try {
      const defs = await studioScanCrossRefs(tabId, [kind]);
      const grouped = new Map<string, CrossRefDef[]>();
      for (const d of defs) {
        const arr = grouped.get(d.id_value);
        if (arr) arr.push(d);
        else     grouped.set(d.id_value, [d]);
      }
      const xrMap = new Map(crossRefsByKind); xrMap.set(kind, grouped); crossRefsByKind = xrMap;
      const tabMap = new Map(crossRefsTabIdByKind); tabMap.set(kind, tabId); crossRefsTabIdByKind = tabMap;
      if (force) {
        const ucNext = new Map(usagesCacheByKind); ucNext.delete(kind); usagesCacheByKind = ucNext;
        const ulNext = new Map(usagesLoadingByKind); ulNext.delete(kind); usagesLoadingByKind = ulNext;
      }
    } catch (e) {
      console.warn(`studio cross-ref scan failed (${kind})`, e);
    } finally {
      const after = new Set(crossRefsLoadingByKind); after.delete(kind); crossRefsLoadingByKind = after;
    }
  }

  function crossRefsForKind(kind: StudioFileKind): Map<string, CrossRefDef[]> {
    if (kind === 'ron') return crossRefs;
    return crossRefsByKind.get(kind) ?? new Map();
  }

  function crossRefsLoadingForKind(kind: StudioFileKind): boolean {
    if (kind === 'ron') return crossRefsLoading;
    return crossRefsLoadingByKind.has(kind);
  }

  /** Resolve `id` to its definitions (zero, one, or many). Callers
   *  decide whether to disambiguate via a picker or jump to the
   *  first hit. */
  function findCrossRefs(id: string): CrossRefDef[] {
    return crossRefs.get(id) ?? [];
  }

  /** Project-wide broken-reference scan. Same dedupe rule as
   *  `loadCrossRefs`: same `tabId` already scanned ⇒ no-op, unless
   *  `force` is set (typically after a save / config change).
   *  An empty result is the happy path — `brokenRefsTabId` records
   *  that we ran, so subsequent calls don't re-fire. */
  async function loadBrokenRefs(tabId: string, force: boolean = false): Promise<void> {
    if (!force && brokenRefsTabId === tabId) return;
    brokenRefsLoading = true;
    try {
      const refs = await studioScanBrokenRefs(tabId);
      brokenRefs      = refs;
      brokenRefsTabId = tabId;
    } catch (e) {
      console.warn('studio broken-ref scan failed', e);
      brokenRefs      = [];
      brokenRefsTabId = tabId;   // still mark as scanned — failure shouldn't trigger retry storm
    } finally {
      brokenRefsLoading = false;
    }
  }

  /** Kind-aware variant — fans out to `brokenRefsByKind`. RON keeps
   *  its legacy slot for back-compat. */
  async function loadBrokenRefsForKind(
    tabId: string,
    kind:  StudioFileKind,
    force: boolean = false,
  ): Promise<void> {
    if (kind === 'ron') { await loadBrokenRefs(tabId, force); return; }
    const cachedTab = brokenRefsTabIdByKind.get(kind);
    if (!force && cachedTab === tabId) return;
    const next = new Set(brokenRefsLoadingByKind); next.add(kind); brokenRefsLoadingByKind = next;
    try {
      const refs = await studioScanBrokenRefs(tabId, [kind]);
      const brMap = new Map(brokenRefsByKind); brMap.set(kind, refs); brokenRefsByKind = brMap;
      const tabMap = new Map(brokenRefsTabIdByKind); tabMap.set(kind, tabId); brokenRefsTabIdByKind = tabMap;
    } catch (e) {
      console.warn(`studio broken-ref scan failed (${kind})`, e);
      const brMap = new Map(brokenRefsByKind); brMap.set(kind, []); brokenRefsByKind = brMap;
      const tabMap = new Map(brokenRefsTabIdByKind); tabMap.set(kind, tabId); brokenRefsTabIdByKind = tabMap;
    } finally {
      const after = new Set(brokenRefsLoadingByKind); after.delete(kind); brokenRefsLoadingByKind = after;
    }
  }

  function brokenRefsForKind(kind: StudioFileKind): BrokenRef[] {
    if (kind === 'ron') return brokenRefs;
    return brokenRefsByKind.get(kind) ?? [];
  }

  function brokenRefsLoadingForKind(kind: StudioFileKind): boolean {
    if (kind === 'ron') return brokenRefsLoading;
    return brokenRefsLoadingByKind.has(kind);
  }

  function brokenRefsTabIdForKind(kind: StudioFileKind): string | null {
    if (kind === 'ron') return brokenRefsTabId;
    return brokenRefsTabIdByKind.get(kind) ?? null;
  }

  function usagesKey(tabId: string, target: string): string {
    return `${tabId}\x00${target}`;
  }

  /** Read the usage cache without triggering a fetch. Returns `null`
   *  when nothing is known yet (so callers can decide whether to show a
   *  spinner or just hide the panel until the load completes). */
  function readUsages(tabId: string, target: string): UsageMatch[] | null {
    return usagesCache.get(usagesKey(tabId, target)) ?? null;
  }

  function isUsagesLoading(tabId: string, target: string): boolean {
    return usagesLoading.has(usagesKey(tabId, target));
  }

  /** Fetch the reverse-lookup list for `target` in the active repo,
   *  cached afterwards. Concurrent calls for the same key dedupe via
   *  `usagesLoading`. */
  async function loadUsages(tabId: string, target: string): Promise<UsageMatch[]> {
    const k = usagesKey(tabId, target);
    const cached = usagesCache.get(k);
    if (cached) return cached;
    if (usagesLoading.has(k)) {
      // Spin until the in-flight fetch finishes — Map mutations don't
      // trigger Svelte rerenders unless we copy, so the cleanest signal
      // is to await the next microtask and retry.
      while (usagesLoading.has(k)) await Promise.resolve();
      return usagesCache.get(k) ?? [];
    }
    const next = new Set(usagesLoading); next.add(k); usagesLoading = next;
    try {
      const matches = await studioFindUsages(tabId, target);
      const cacheNext = new Map(usagesCache); cacheNext.set(k, matches); usagesCache = cacheNext;
      return matches;
    } catch (e) {
      console.warn('studio find_usages failed', e);
      return [];
    } finally {
      const after = new Set(usagesLoading); after.delete(k); usagesLoading = after;
    }
  }

  /** Kind-aware usage fetch — RON falls through to `loadUsages` for
   *  back-compat; other kinds go through `usagesCacheByKind`. */
  async function loadUsagesForKind(
    tabId:  string,
    target: string,
    kind:   StudioFileKind,
  ): Promise<UsageMatch[]> {
    if (kind === 'ron') return loadUsages(tabId, target);
    const k = usagesKey(tabId, target);
    const cache = usagesCacheByKind.get(kind);
    const cached = cache?.get(k);
    if (cached) return cached;
    let loadingSet = usagesLoadingByKind.get(kind);
    if (loadingSet?.has(k)) {
      while (usagesLoadingByKind.get(kind)?.has(k)) await Promise.resolve();
      return usagesCacheByKind.get(kind)?.get(k) ?? [];
    }
    const nextLoading = new Set<string>(loadingSet ?? new Set<string>()); nextLoading.add(k);
    const ulNext = new Map(usagesLoadingByKind); ulNext.set(kind, nextLoading); usagesLoadingByKind = ulNext;
    try {
      const matches = await studioFindUsages(tabId, target, [kind]);
      const ucNext = new Map(usagesCacheByKind);
      const cMap = new Map(ucNext.get(kind) ?? new Map());
      cMap.set(k, matches);
      ucNext.set(kind, cMap);
      usagesCacheByKind = ucNext;
      return matches;
    } catch (e) {
      console.warn(`studio find_usages failed (${kind})`, e);
      return [];
    } finally {
      const after = new Set<string>(usagesLoadingByKind.get(kind) ?? new Set<string>()); after.delete(k);
      const ulAfter = new Map(usagesLoadingByKind); ulAfter.set(kind, after); usagesLoadingByKind = ulAfter;
    }
  }

  function readUsagesForKind(tabId: string, target: string, kind: StudioFileKind): UsageMatch[] | null {
    if (kind === 'ron') return readUsages(tabId, target);
    return usagesCacheByKind.get(kind)?.get(usagesKey(tabId, target)) ?? null;
  }

  function isUsagesLoadingForKind(tabId: string, target: string, kind: StudioFileKind): boolean {
    if (kind === 'ron') return isUsagesLoading(tabId, target);
    return usagesLoadingByKind.get(kind)?.has(usagesKey(tabId, target)) ?? false;
  }

  function findCrossRefsForKind(id: string, kind: StudioFileKind): CrossRefDef[] {
    return crossRefsForKind(kind).get(id) ?? [];
  }

  /** Drop every usage entry — called after Save when the index could
   *  have shifted. Cheap, no IO. */
  function invalidateUsages(): void {
    usagesCache   = new Map();
    usagesLoading = new Set();
  }

  /** Toggle an exclude entry for a repo-relative path (single file) or
   *  a glob (e.g. `foo/**` for a whole folder), then re-scan so the
   *  sidebar reflects the new state without needing the user to click
   *  Refresh. Returns the new state. */
  async function toggleExcludeFor(tabId: string, pattern: string): Promise<boolean> {
    const now = await studioToggleExclude(tabId, pattern);
    await load(tabId);
    // Cross-ref / usage / broken-ref caches all depended on the old
    // exclude set; nuke them so the next call rescans.
    crossRefs       = new Map();
    crossRefsTabId  = null;
    usagesCache     = new Map();
    usagesLoading   = new Set();
    brokenRefs      = [];
    brokenRefsTabId = null;
    crossRefsByKind       = new Map();
    crossRefsTabIdByKind  = new Map();
    brokenRefsByKind      = new Map();
    brokenRefsTabIdByKind = new Map();
    usagesCacheByKind     = new Map();
    usagesLoadingByKind   = new Map();
    return now;
  }

  /** Folder glob the sidebar uses for "Exclude folder" — keep both
   *  ends in sync so we can detect "this folder is already excluded
   *  as a whole" by exact-matching the recorded pattern. */
  function folderExcludeGlob(relPath: string): string {
    return relPath.endsWith('/**') ? relPath : `${relPath}/**`;
  }

  /** Register an external location (file or folder) under the active
   *  project. After persisting the config we trigger a full refresh
   *  so the new entries show up in the sidebar without needing the
   *  user to click Rescan. Cross-ref / broken-ref caches are nuked
   *  for the same reason. */
  async function addExternal(tabId: string, path: string, label?: string): Promise<void> {
    await studioAddExternal(tabId, path, label);
    crossRefs       = new Map();
    crossRefsTabId  = null;
    usagesCache     = new Map();
    usagesLoading   = new Set();
    brokenRefs      = [];
    brokenRefsTabId = null;
    crossRefsByKind       = new Map();
    crossRefsTabIdByKind  = new Map();
    brokenRefsByKind      = new Map();
    brokenRefsTabIdByKind = new Map();
    usagesCacheByKind     = new Map();
    usagesLoadingByKind   = new Map();
    await load(tabId);
    if (settings.use_index && !indexJobRunning) {
      void refreshIndex(tabId);
    }
  }

  /** Drop an external registration. Idempotent — no-op (and no
   *  rescan) when nothing matched. */
  async function removeExternal(tabId: string, path: string): Promise<boolean> {
    const removed = await studioRemoveExternal(tabId, path);
    if (!removed) return false;
    crossRefs       = new Map();
    crossRefsTabId  = null;
    usagesCache     = new Map();
    usagesLoading   = new Set();
    brokenRefs      = [];
    brokenRefsTabId = null;
    crossRefsByKind       = new Map();
    crossRefsTabIdByKind  = new Map();
    brokenRefsByKind      = new Map();
    brokenRefsTabIdByKind = new Map();
    usagesCacheByKind     = new Map();
    usagesLoadingByKind   = new Map();
    await load(tabId);
    if (settings.use_index && !indexJobRunning) {
      void refreshIndex(tabId);
    }
    return true;
  }

  function isFolderExcluded(relPath: string): boolean {
    const g = folderExcludeGlob(relPath);
    return config.excludes.includes(g);
  }

  /** Look up the per-folder schema binding (override whose glob is the
   *  folder's `<path>/**` form). Returns `undefined` when nothing is
   *  bound at this level — files inside may still inherit from a
   *  higher folder or a per-file override. */
  /** Find the exact-glob override that targets `relPath` as a file
   *  (i.e. glob === relPath, no wildcards). Used by the bind modal to
   *  seed its `reference_fields` chip editor with the existing list. */
  function fileOverride(relPath: string): {
    rs_file: string;
    root_type: string;
    reference_fields?: string[];
  } | undefined {
    const norm = relPath.replace(/\\/g, '/');
    const hit = config.overrides.find(o => o.glob === norm);
    return hit
      ? { rs_file: hit.rs_file, root_type: hit.root_type, reference_fields: hit.reference_fields }
      : undefined;
  }

  function folderBinding(relPath: string): {
    rs_file: string;
    root_type: string;
    reference_fields?: string[];
  } | undefined {
    const g = folderExcludeGlob(relPath);
    const hit = config.overrides.find(o => o.glob === g);
    return hit
      ? { rs_file: hit.rs_file, root_type: hit.root_type, reference_fields: hit.reference_fields }
      : undefined;
  }

  // ── Reference-field pattern resolution ─────────────────────────────────
  // Mirrors `studio::config::resolve_reference_fields` on the host so the
  // frontend's Ctrl+click highlight uses the exact same rules as the
  // host-side find_usages scan. Empty / null = caller falls back to the
  // built-in convention list.

  function clientGlobMatch(pattern: string, path: string): boolean {
    // Very small glob: supports `*` (any non-/) and `**` (any chars).
    // Anchored to the full path. Sufficient for the config's matching
    // needs — `foo/**`, `foo/*.ron`, exact paths.
    const pat = pattern.replace(/\\/g, '/');
    const txt = path.replace(/\\/g, '/');
    const dfs = (pi: number, ti: number): boolean => {
      while (true) {
        if (pi === pat.length) return ti === txt.length;
        const c = pat[pi];
        if (c === '*') {
          const dbl = pat[pi + 1] === '*';
          const rest = dbl ? pi + 2 : pi + 1;
          for (let k = ti; k <= txt.length; k++) {
            if (dfs(rest, k)) return true;
            if (k < txt.length && !dbl && txt[k] === '/') return false;
            if (k === txt.length) return false;
          }
          return false;
        }
        if (ti === txt.length) return false;
        if (c !== txt[ti]) return false;
        pi++; ti++;
      }
    };
    return dfs(0, 0);
  }

  /** Where would a `toggle_reference_field` call against `relPath`
   *  land in `.ron-studio.toml`? Mirrors the backend's matching order
   *  so the modal can surface the scope in the context-menu label
   *  ("(folder: enemies/)" vs "(file)" vs "(default)") before the user
   *  confirms — avoiding the surprise of toggling at a broader scope
   *  than intended. */
  type BindingScope =
    | { kind: 'file';    glob: string }
    | { kind: 'folder';  glob: string; folder: string }
    | { kind: 'glob';    glob: string }   // override whose glob isn't a folder or an exact file
    | { kind: 'default' }
    | { kind: 'new' };                    // no binding yet → will create per-file override

  function resolveBindingScope(relPath: string): BindingScope {
    const norm = relPath.replace(/\\/g, '/');
    for (const o of config.overrides) {
      if (!clientGlobMatch(o.glob, norm)) continue;
      if (o.glob === norm) return { kind: 'file', glob: o.glob };
      if (o.glob.endsWith('/**')) {
        return { kind: 'folder', glob: o.glob, folder: o.glob.slice(0, -3) };
      }
      return { kind: 'glob', glob: o.glob };
    }
    if (config.default) return { kind: 'default' };
    return { kind: 'new' };
  }

  /** Resolve the reference-field patterns configured for `relPath`.
   *  Returns `null` when nothing is configured (the caller then falls
   *  back to the built-in convention). */
  function referenceFieldsFor(relPath: string): string[] | null {
    const norm = relPath.replace(/\\/g, '/');
    for (const o of config.overrides) {
      if (clientGlobMatch(o.glob, norm)) {
        if (o.reference_fields && o.reference_fields.length > 0) {
          return o.reference_fields;
        }
        // First-match-wins: stop here even if this override has no
        // custom list — fall through to default.
        break;
      }
    }
    if (config.default?.reference_fields && config.default.reference_fields.length > 0) {
      return config.default.reference_fields;
    }
    return null;
  }

  /** Pattern matcher — matches `*` / `*suffix` / `prefix*` / exact. */
  function matchesPattern(pattern: string, key: string): boolean {
    if (pattern === '*') return true;
    if (pattern.startsWith('*')) return key.endsWith(pattern.slice(1));
    if (pattern.endsWith('*'))   return key.startsWith(pattern.slice(0, -1));
    return pattern === key;
  }

  async function bindSchemaFor(
    tabId:           string,
    relativePath:    string,
    rsFile:          string,
    rootType:        string,
    /** `null` keeps existing patterns, `[]` clears them, array sets them. */
    referenceFields: string[] | null = null,
  ): Promise<void> {
    await studioBindSchema(tabId, relativePath, rsFile, rootType, referenceFields);
    await load(tabId);
  }

  async function unbindSchemaFor(tabId: string, relativePath: string): Promise<boolean> {
    const removed = await studioUnbindSchema(tabId, relativePath);
    if (removed) await load(tabId);
    return removed;
  }

  /** Toggle a single field name in the reference-field patterns of the
   *  binding matching `relativePath`. Re-loads the file index + nukes
   *  cross-ref / usage caches afterwards so the tree decoration on the
   *  flipped field appears/disappears immediately. */
  async function toggleReferenceFieldFor(
    tabId:        string,
    relativePath: string,
    field:        string,
  ): Promise<boolean> {
    const now = await studioToggleReferenceField(tabId, relativePath, field);
    await load(tabId);
    crossRefs       = new Map();
    crossRefsTabId  = null;
    usagesCache     = new Map();
    usagesLoading   = new Set();
    brokenRefs      = [];
    brokenRefsTabId = null;
    crossRefsByKind       = new Map();
    crossRefsTabIdByKind  = new Map();
    brokenRefsByKind      = new Map();
    brokenRefsTabIdByKind = new Map();
    usagesCacheByKind     = new Map();
    usagesLoadingByKind   = new Map();
    if (settings.use_index && !indexJobRunning) {
      void refreshIndex(tabId);
    }
    return now;
  }

  return {
    get files()        { return files; },
    get loading()      { return loading; },
    get error()        { return error; },
    get loadedTabId()  { return loadedTabId; },
    get lastLoadedAt() { return lastLoadedAt; },
    get filter()       { return filter; },
    get activeKinds()  { return activeKinds; },
    get expanded()         { return expanded; },
    get crossRefs()        { return crossRefs; },
    get crossRefsLoading() { return crossRefsLoading; },
    get brokenRefs()        { return brokenRefs; },
    get brokenRefsLoading() { return brokenRefsLoading; },
    get brokenRefsTabId()   { return brokenRefsTabId; },
    get crossRefsTabId()   { return crossRefsTabId; },
    load,
    ensureLoadedFor,
    refresh,
    setFilter,
    setActiveKinds,
    toggleKind,
    toggleFolder,
    expandAll,
    collapseAll,
    clear,
    loadCrossRefs,
    loadBrokenRefs,
    findCrossRefs,
    loadUsages,
    readUsages,
    isUsagesLoading,
    invalidateUsages,
    // Kind-aware variants (Phase 3.c).
    loadCrossRefsForKind,
    loadBrokenRefsForKind,
    loadUsagesForKind,
    readUsagesForKind,
    isUsagesLoadingForKind,
    findCrossRefsForKind,
    crossRefsForKind,
    crossRefsLoadingForKind,
    brokenRefsForKind,
    brokenRefsLoadingForKind,
    brokenRefsTabIdForKind,
    get showExcluded()     { return showExcluded; },
    get config()           { return config; },
    get settings()         { return settings; },
    get indexProgress()    { return indexProgress; },
    get indexJobRunning()  { return indexJobRunning; },
    ensureSettingsLoaded,
    updateSettings,
    installIndexListeners,
    refreshIndex,
    setShowExcluded,
    toggleShowExcluded,
    toggleExcludeFor,
    addExternal,
    removeExternal,
    folderExcludeGlob,
    isFolderExcluded,
    folderBinding,
    fileOverride,
    referenceFieldsFor,
    resolveBindingScope,
    matchesPattern,
    /** Glob-match a project-relative path against a `.ron-studio.toml`
     *  override pattern. Supports `*` (one path segment) and `**`
     *  (any chars including `/`). Used by the modal's Bindings panel
     *  to attribute each cross-ref definition to the binding whose
     *  scope covers its file. */
    globMatch: clientGlobMatch,
    bindSchemaFor,
    unbindSchemaFor,
    toggleReferenceFieldFor,
  };
}

export const studioStore = createStudioStore();
