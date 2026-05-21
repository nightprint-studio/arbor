/**
 * Central per-tab data cache.
 *
 * Architecture
 * ────────────
 * - `tabCaches` — LRU map of tabId → TabSnapshot (graph, branches, CI/MR data)
 * - `commitDetailCache` — global SHA → CommitDetail map (commits are immutable)
 * - Auto-refresh scheduler — runs on the active tab when the app is focused;
 *   compares repo fingerprint and only triggers a reload when something changed.
 * - Config — loaded from / saved to ~/.config/arbor/config.toml via Tauri commands.
 *
 * Usage
 * ─────
 * Components call `cacheStore.loadGraph(tabId, ...)` instead of `getGraph(...)`.
 * On cache miss the function fetches from the backend and stores the result.
 * On cache hit it returns immediately — no IPC call.
 *
 * After any write operation the IPC wrapper in cache-invalidate.ts calls
 * `invalidateTabCache(tabId)`, which removes that tab's snapshot so the next
 * read goes back to the backend.
 */

import type { GraphData, CommitDetail, BranchInfo, TagInfo, StashEntry, SubmoduleInfo } from '$lib/types/git';
import type { CiProviderInfo, CiRun, PipelineDef, PipelineRun } from '$lib/types/pipeline';
import type { MergeRequest } from '$lib/types/mr';
import type { MergedMrHint } from '$lib/types/mr';
import type { CacheConfig } from '$lib/types/config';

import { getGraph, getGraphForFile, getCommitDetail, getRepoFingerprint } from '$lib/ipc/graph';
import { listLocalBranches, listRemoteBranches, listStashes, listTags, getNearestTag } from '$lib/ipc/branch';
import { listSubmodules } from '$lib/ipc/submodule';
import { getMergedMrHints } from '$lib/ipc/mr';
import { getCiProvider, fetchCiRuns, listPipelineDefs, listPipelineRuns } from '$lib/ipc/pipeline';
import { listMrs } from '$lib/ipc/mr';
import { getCacheConfig, setCacheConfig, evictTabCache } from '$lib/ipc/config';
import { registerInvalidateHandler } from '$lib/ipc/cache-invalidate';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface SidebarData {
  localBranches:  BranchInfo[];
  remoteBranches: BranchInfo[];
  stashes:        StashEntry[];
  tags:           TagInfo[];
  submodules:     SubmoduleInfo[];
  nearestTag:     string | null;
}

interface TabSnapshot {
  graph:          GraphData | null;
  sidebar:        SidebarData | null;
  mrHints:        MergedMrHint[] | null;
  ciProvider:     CiProviderInfo | null | undefined; // undefined = not yet checked
  ciRuns:         CiRun[] | null;
  pipelineDefs:   PipelineDef[] | null;
  pipelineRuns:   PipelineRun[] | null;
  mrLists:        Partial<Record<'open' | 'closed' | 'merged' | 'all', MergeRequest[]>>;
  mrProviderInfo: CiProviderInfo | null | undefined;
  fingerprint:    string | null;
  lastRefreshed:  number; // ms since epoch
  lruTick:        number; // higher = more recently used
}

const DEFAULT_CONFIG: CacheConfig = {
  enabled:                   true,
  max_tabs:                  10,
  refresh_interval_secs:     60,
  scheduler_enabled:         true,
  auto_evict_enabled:        false,
  tab_idle_secs:             300,
  evict_check_interval_secs: 60,
  close_repo_on_evict:       true,
  min_cached_tabs:           1,
  repo_browser_ttl_secs:     600,
};

// ── Store ─────────────────────────────────────────────────────────────────────

function createCacheStore() {
  // ── State ──────────────────────────────────────────────────────────────────
  const tabCaches     = new Map<string, TabSnapshot>();
  const commitDetails = new Map<string, CommitDetail>();
  let   lruCounter    = 0;

  // Last fingerprint seen by refreshIfChanged — kept independent of cache
  // snapshots so it survives invalidate() calls and works even with cache off.
  const fetchFingerprints = new Map<string, string>();

  // Per-tab last-accessed timestamp — survives cache invalidation so the eviction
  // scheduler can compare against a stable baseline even after an invalidate().
  const tabLastAccessed = new Map<string, number>();

  let config          = $state<CacheConfig>({ ...DEFAULT_CONFIG });
  let schedulerHandle = $state<ReturnType<typeof setInterval> | null>(null);
  let evictHandle     = $state<ReturnType<typeof setInterval> | null>(null);
  // Incremented on every invalidation so $derived(cacheStore.stats()) reacts.
  let _cacheVersion   = $state(0);

  // Reactive: last-refreshed timestamp for the currently active tab.
  // Exposed to StatusBar via `cacheStore.activeTabLastRefreshed`.
  let _activeTabId        = $state<string | null>(null);
  let _activeTabRefreshed = $state<number | null>(null);

  // ── Init ───────────────────────────────────────────────────────────────────

  async function init(activeTabIdFn: () => string | null) {
    // Load config from backend
    try {
      const saved = await getCacheConfig();
      config = saved;
    } catch {
      // Keep defaults on error
    }

    // Register invalidation hook used by ipc mutation wrappers
    registerInvalidateHandler((tabId) => {
      invalidate(tabId);
    });

    // Start schedulers if enabled
    _startScheduler(activeTabIdFn);
    _startEvictScheduler();
  }

  // ── LRU helpers ────────────────────────────────────────────────────────────

  function _touch(tabId: string) {
    const snap = tabCaches.get(tabId);
    if (snap) snap.lruTick = ++lruCounter;
    tabLastAccessed.set(tabId, Date.now());
  }

  function _evictIfNeeded() {
    if (tabCaches.size <= config.max_tabs) return;
    // Find LRU tab (lowest tick)
    let lruId: string | null = null;
    let lruTick = Infinity;
    for (const [id, snap] of tabCaches) {
      if (snap.lruTick < lruTick) { lruTick = snap.lruTick; lruId = id; }
    }
    if (lruId) tabCaches.delete(lruId);
  }

  function _getOrCreate(tabId: string): TabSnapshot {
    let snap = tabCaches.get(tabId);
    if (!snap) {
      snap = {
        graph:          null,
        sidebar:        null,
        mrHints:        null,
        ciProvider:     undefined,
        ciRuns:         null,
        pipelineDefs:   null,
        pipelineRuns:   null,
        mrLists:        {},
        mrProviderInfo: undefined,
        fingerprint:    null,
        lastRefreshed:  0,
        lruTick:        ++lruCounter,
      };
      tabCaches.set(tabId, snap);
      _evictIfNeeded();
    }
    return snap;
  }

  function _recordRefresh(tabId: string, fingerprint?: string) {
    const snap = tabCaches.get(tabId);
    if (!snap) return;
    snap.lastRefreshed = Date.now();
    if (fingerprint !== undefined) snap.fingerprint = fingerprint;
    if (tabId === _activeTabId) _activeTabRefreshed = snap.lastRefreshed;
  }

  // ── Public: invalidation ───────────────────────────────────────────────────

  function invalidate(tabId: string) {
    tabCaches.delete(tabId);
    _cacheVersion++;
    if (tabId === _activeTabId) _activeTabRefreshed = null;
  }

  function invalidateAll() {
    tabCaches.clear();
    commitDetails.clear();
    _cacheVersion++;
    _activeTabRefreshed = null;
  }

  /** Full reset: clears frontend maps AND evicts backend caches for every tab. */
  async function clearAll() {
    const tabIds = [...tabCaches.keys()];
    invalidateAll();
    fetchFingerprints.clear();
    tabLastAccessed.clear();
    for (const tabId of tabIds) {
      evictTabCache(tabId).catch(() => {});
    }
  }

  /** Called by AppShell (or whichever component tracks the active tab) */
  function setActiveTabId(tabId: string | null) {
    _activeTabId = tabId;
    _activeTabRefreshed = tabId ? (tabCaches.get(tabId)?.lastRefreshed ?? null) : null;
    // Switching to a tab counts as accessing it — reset its idle timer.
    if (tabId) tabLastAccessed.set(tabId, Date.now());
  }

  // ── Public: cache-aware data loaders ───────────────────────────────────────

  async function loadGraph(
    tabId:      string,
    offset:     number,
    limit:      number,
    fileFilter: string | null,
  ): Promise<GraphData> {
    // Only cache full-page loads (offset=0) with no file filter active.
    if (!config.enabled || offset > 0 || fileFilter) {
      return fileFilter
        ? getGraphForFile(tabId, fileFilter, offset, limit)
        : getGraph(tabId, offset, limit);
    }

    const snap = _getOrCreate(tabId);
    if (snap.graph) {
      _touch(tabId);
      return snap.graph;
    }

    const data = await getGraph(tabId, offset, limit);
    snap.graph = data;
    _recordRefresh(tabId);
    return data;
  }

  async function loadSidebarData(tabId: string): Promise<SidebarData> {
    if (!config.enabled) return _fetchSidebarData(tabId);

    const snap = _getOrCreate(tabId);
    if (snap.sidebar) {
      _touch(tabId);
      return snap.sidebar;
    }

    const data = await _fetchSidebarData(tabId);
    snap.sidebar = data;
    _recordRefresh(tabId);
    return data;
  }

  async function loadMrHints(tabId: string): Promise<MergedMrHint[]> {
    if (!config.enabled) return getMergedMrHints(tabId).catch(() => []);

    const snap = _getOrCreate(tabId);
    if (snap.mrHints !== null) {
      _touch(tabId);
      return snap.mrHints;
    }

    const hints = await getMergedMrHints(tabId).catch(() => [] as MergedMrHint[]);
    snap.mrHints = hints;
    return hints;
  }

  async function loadCiProvider(tabId: string): Promise<CiProviderInfo | null> {
    if (!config.enabled) return getCiProvider(tabId);

    const snap = _getOrCreate(tabId);
    if (snap.ciProvider !== undefined) {
      _touch(tabId);
      return snap.ciProvider ?? null;
    }

    const info = await getCiProvider(tabId).catch(() => null);
    snap.ciProvider = info;
    return info;
  }

  async function loadCiRuns(tabId: string, force = false): Promise<CiRun[]> {
    if (!config.enabled) return fetchCiRuns(tabId);

    const snap = _getOrCreate(tabId);
    if (!force && snap.ciRuns !== null) {
      _touch(tabId);
      return snap.ciRuns;
    }

    const runs = await fetchCiRuns(tabId);
    snap.ciRuns = runs;
    _recordRefresh(tabId);
    return runs;
  }

  async function loadPipelineData(tabId?: string): Promise<{ defs: PipelineDef[]; runs: PipelineRun[] }> {
    // Pipeline defs/runs are global (not per-tab) in the current backend.
    // Use a sentinel key for the global cache entry.
    const key = tabId ?? '__global__';
    if (!config.enabled) {
      const [defs, runs] = await Promise.all([listPipelineDefs(), listPipelineRuns()]);
      return { defs, runs };
    }

    const snap = _getOrCreate(key);
    if (snap.pipelineDefs !== null && snap.pipelineRuns !== null) {
      _touch(key);
      return { defs: snap.pipelineDefs, runs: snap.pipelineRuns };
    }

    const [defs, runs] = await Promise.all([listPipelineDefs(), listPipelineRuns()]);
    snap.pipelineDefs = defs;
    snap.pipelineRuns = runs;
    return { defs, runs };
  }

  async function loadMrList(
    tabId:  string,
    filter: 'open' | 'closed' | 'merged' | 'all',
    force = false,
  ): Promise<MergeRequest[]> {
    if (!config.enabled) return listMrs(tabId, filter);

    const snap = _getOrCreate(tabId);
    const cached = snap.mrLists[filter];
    if (!force && cached !== undefined) {
      _touch(tabId);
      return cached;
    }

    const mrs = await listMrs(tabId, filter);
    snap.mrLists[filter] = mrs;
    _recordRefresh(tabId);
    return mrs;
  }

  async function loadMrProvider(tabId: string): Promise<CiProviderInfo | null> {
    if (!config.enabled) return getCiProvider(tabId).catch(() => null);

    const snap = _getOrCreate(tabId);
    if (snap.mrProviderInfo !== undefined) {
      _touch(tabId);
      return snap.mrProviderInfo ?? null;
    }

    const info = await getCiProvider(tabId).catch(() => null);
    snap.mrProviderInfo = info;
    return info;
  }

  async function loadCommitDetail(tabId: string, oid: string): Promise<CommitDetail> {
    const cached = commitDetails.get(oid);
    if (cached) return cached;
    const detail = await getCommitDetail(tabId, oid);
    commitDetails.set(oid, detail);
    return detail;
  }

  // ── Scheduler ─────────────────────────────────────────────────────────────

  function _startScheduler(activeTabIdFn: () => string | null) {
    if (schedulerHandle !== null) clearInterval(schedulerHandle);
    schedulerHandle = null;
    if (!config.scheduler_enabled) return;

    schedulerHandle = setInterval(async () => {
      if (!config.enabled || !config.scheduler_enabled) return;
      if (!document.hasFocus()) return;

      const tabId = activeTabIdFn();
      if (!tabId) return;

      try {
        const fingerprint = await getRepoFingerprint(tabId);
        const snap = tabCaches.get(tabId);

        if (!snap) {
          // No cached data yet — nothing to compare, first load will populate it.
          return;
        }

        if (fingerprint === snap.fingerprint) {
          // No changes detected — do NOT refresh the UI.
          return;
        }

        // Changes detected: invalidate this tab and trigger a reload.
        invalidate(tabId);
        // Import graphStore dynamically to avoid circular imports at module level.
        const { graphStore } = await import('./graph.svelte');
        graphStore.refresh();
      } catch {
        // Silently ignore — network errors, tab closed, etc.
      }
    }, config.refresh_interval_secs * 1000);
  }

  function restartScheduler(activeTabIdFn: () => string | null) {
    _startScheduler(activeTabIdFn);
  }

  // ── Idle-eviction scheduler ────────────────────────────────────────────────

  /** Evict backend + frontend caches for tabs idle longer than `tab_idle_secs`.
   *  The `min_cached_tabs` most-recently-used tabs are always kept. */
  async function _evictIdleTabs() {
    if (!config.enabled || !config.auto_evict_enabled) return;

    const now    = Date.now();
    const idleMs = config.tab_idle_secs * 1000;

    // Determine which tabs are protected (the N most recently accessed).
    const minKeep = Math.max(1, config.min_cached_tabs);
    const sortedByRecency = [...tabCaches.keys()].sort((a, b) => {
      const la = tabLastAccessed.get(a) ?? 0;
      const lb = tabLastAccessed.get(b) ?? 0;
      return lb - la; // descending: most recent first
    });
    const protected_ = new Set(sortedByRecency.slice(0, minKeep));

    const toEvict: string[] = [];
    for (const tabId of tabCaches.keys()) {
      if (protected_.has(tabId)) continue;
      const last = tabLastAccessed.get(tabId) ?? 0;
      if (now - last >= idleMs) toEvict.push(tabId);
    }

    for (const tabId of toEvict) {
      tabCaches.delete(tabId);
      tabLastAccessed.delete(tabId);
      fetchFingerprints.delete(tabId);
      // Sync backend (stats_cache + ticket_caches)
      evictTabCache(tabId).catch(() => { /* ignore */ });
    }
  }

  function _startEvictScheduler() {
    if (evictHandle !== null) clearInterval(evictHandle);
    evictHandle = null;
    if (!config.auto_evict_enabled) return;

    const intervalMs = Math.max(config.evict_check_interval_secs, 10) * 1000;
    evictHandle = setInterval(() => { _evictIdleTabs(); }, intervalMs);
  }

  // ── Config persistence ─────────────────────────────────────────────────────

  async function saveConfig(newConfig: CacheConfig, activeTabIdFn: () => string | null) {
    config = { ...newConfig };
    await setCacheConfig(newConfig);
    restartScheduler(activeTabIdFn);
    _startEvictScheduler();
  }

  // ── Private helpers ────────────────────────────────────────────────────────

  async function _fetchSidebarData(tabId: string): Promise<SidebarData> {
    const [local, remote, stashes, tags, subs, nearest] = await Promise.all([
      listLocalBranches(tabId),
      listRemoteBranches(tabId),
      listStashes(tabId),
      listTags(tabId),
      listSubmodules(tabId),
      getNearestTag(tabId),
    ]);
    return { localBranches: local, remoteBranches: remote, stashes, tags, submodules: subs, nearestTag: nearest };
  }

  // ── Expose fingerprint update for scheduler ────────────────────────────────

  /** Save a freshly-fetched fingerprint into the active tab's snapshot.
   *  Called by CommitGraph after loading the graph so the scheduler has a
   *  baseline to compare against. */
  function recordFingerprint(tabId: string, fingerprint: string) {
    const snap = tabCaches.get(tabId);
    if (snap) snap.fingerprint = fingerprint;
  }

  // ── Fingerprint-aware refresh (used by arbor://graph-refresh handler) ────────

  /** Light status refresh — re-fetches `RepoStatus` and pushes it to the
   *  repoStore so the WIP row above the graph reflects current local edits.
   *  Cheap (no graph walk, no diff compute) and crucially does NOT touch
   *  graph data, so it can run on every fetch tick without a spinner flash. */
  async function refreshStatusOnly(tabId: string): Promise<void> {
    try {
      const { getStatus } = await import('$lib/ipc/stage');
      const { repoStore } = await import('./repo.svelte');
      const s = await getStatus(tabId);
      repoStore.setStatus(s);
    } catch {
      // Status read is best-effort — leave the previous value in place.
    }
  }

  /** Check fingerprint before refreshing — avoids re-rendering when a fetch
   *  completes but nothing actually changed in the repo.
   *
   *  On the FIRST call for a tab there is no `fetchFingerprints` baseline,
   *  so we fall back to the cached snapshot's fingerprint (i.e. what the
   *  user currently sees). This is the critical case the previous version
   *  got wrong: the manual Fetch button would set a baseline silently and
   *  the user would never see the new commits until they did something
   *  else that invalidated the cache (e.g. branch checkout).
   *
   *  Status (working-tree changes) is ALWAYS refreshed regardless of the
   *  fingerprint check — the fingerprint only covers refs, so a fetch that
   *  brought no new commits would otherwise leave the WIP row stale even
   *  though the user may have edited files locally since the last reload.
   *  Status refresh is cheap and skips the spinner path entirely. */
  async function refreshIfChanged(tabId: string): Promise<void> {
    try {
      const fingerprint = await getRepoFingerprint(tabId);
      const last     = fetchFingerprints.get(tabId);
      const snap     = tabCaches.get(tabId);
      // Prefer the recorded fetch baseline; otherwise compare against
      // whatever fingerprint is currently shown by the cached graph.
      const baseline = last ?? snap?.fingerprint;

      // Always update the recorded baseline so subsequent calls compare
      // against the most-recent live fingerprint.
      fetchFingerprints.set(tabId, fingerprint);

      // No baseline at all (no cached data, no prior fetch) — the next
      // loadGraph will populate naturally; no need to force a refresh.
      if (baseline === undefined) {
        await refreshStatusOnly(tabId);
        return;
      }

      if (fingerprint === baseline) {
        // Refs unchanged — but local files may have changed since the
        // last full load. Refresh just the status so the WIP row stays
        // accurate without paying for a full graph reload.
        await refreshStatusOnly(tabId);
        return;
      }

      // Refs changed: invalidate cache + reload graph (loadGraph will
      // refresh status as part of its Promise.all, no need to do it here).
      invalidate(tabId);
      const { graphStore } = await import('./graph.svelte');
      graphStore.refresh();
    } catch {
      // On error fall back to an unconditional refresh.
      const { graphStore } = await import('./graph.svelte');
      graphStore.refresh();
    }
  }

  // ── Debug / stats ──────────────────────────────────────────────────────────

  function stats() {
    void _cacheVersion; // track reactive version so $derived re-runs on invalidation
    return {
      cachedTabs:    tabCaches.size,
      cachedCommits: commitDetails.size,
      config:        { ...config },
    };
  }

  return {
    // Config
    get config()               { return config; },
    get activeTabLastRefreshed() { return _activeTabRefreshed; },

    // Init
    init,
    setActiveTabId,

    // Cache-aware loaders
    loadGraph,
    loadSidebarData,
    loadMrHints,
    loadCiProvider,
    loadCiRuns,
    loadPipelineData,
    loadMrList,
    loadMrProvider,
    loadCommitDetail,

    // Invalidation
    invalidate,
    invalidateAll,
    clearAll,
    recordFingerprint,
    refreshIfChanged,

    // Config
    saveConfig,
    restartScheduler,

    // Debug
    stats,
  };
}

export const cacheStore = createCacheStore();
