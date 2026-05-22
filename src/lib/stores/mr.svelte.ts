import { getMrDetail } from '$lib/ipc/mr';
import type { MergeRequest, MrDetail, MrFeatureStatus } from '$lib/types/mr';
import type { CiProviderInfo } from '$lib/types/pipeline';
import { withLoading } from '$lib/utils/async-state';
import { cacheStore } from './cache.svelte';

type MrStateFilter = 'open' | 'closed' | 'merged' | 'all';

function createMrStore() {
  let mrs           = $state<MergeRequest[]>([]);
  let loading       = $state(false);
  let error         = $state<string | null>(null);
  let stateFilter   = $state<MrStateFilter>('open');
  let activeNumber  = $state<number | null>(null);
  let detail        = $state<MrDetail | null>(null);
  let detailLoading = $state(false);
  let detailError   = $state<string | null>(null);
  /// Separate cache for "all states" — populated by `loadAll` and consumed
  /// by surfaces (like the Command Palette autocomplete) that need to see
  /// merged + closed + open in a single list without changing the sidebar's
  /// `stateFilter`. Keyed implicitly to the most recently loaded tab; cleared
  /// on `clearAll()` (called from cache invalidation paths).
  let allMrs        = $state<MergeRequest[]>([]);
  let allLoading    = $state(false);
  let allLoadedTab  = $state<string | null>(null);
  /**
   * undefined = not yet checked
   * null      = checked, no GitHub/GitLab remote found
   * object    = provider detected
   */
  let providerInfo  = $state<CiProviderInfo | null | undefined>(undefined);
  /**
   * Result of the MR-feature probe (archived / merge_requests disabled / …).
   * `undefined` = not yet probed. Sidebar + palette gate on `enabled === false`.
   */
  let mrFeature     = $state<MrFeatureStatus | undefined>(undefined);

  // Monotonic counter used to discard results from stale `load()` calls.
  // Protects against rapid filter-tab switches where a slower request would
  // otherwise overwrite the newer one's data.
  let loadVersion = 0;

  const provider = $derived(providerInfo?.provider ?? null);

  async function detectProvider(tabId: string): Promise<CiProviderInfo | null> {
    try {
      const info = await cacheStore.loadMrProvider(tabId);
      providerInfo = info;
      return info;
    } catch {
      providerInfo = null;
      return null;
    }
  }

  async function load(tabId: string, filter?: MrStateFilter, force = false) {
    if (filter) stateFilter = filter;
    const myVersion       = ++loadVersion;
    const requestedFilter = stateFilter;
    // A force-refresh from the sidebar should also invalidate the "all
    // states" cache used by the Command Palette autocomplete — otherwise
    // the palette keeps showing the stale list until the next tab change.
    if (force) { allLoadedTab = null; }

    loading   = true;
    error     = null;
    // Reset feature gate so a previous tab's "disabled" state doesn't leak
    // into the new tab's view while the probe is in flight.
    mrFeature = undefined;

    // Always detect provider first (fast, no token needed — result is cached)
    const info = await detectProvider(tabId);
    if (myVersion !== loadVersion) return; // superseded

    if (!info || !info.has_token) {
      // No remote or no token — don't attempt the API call
      loading = false;
      mrs     = [];
      return;
    }

    // Probe MR feature availability before hitting list_mrs (which would
    // otherwise surface a 404 as a raw error). The probe is cached per tab
    // and re-fetched on explicit `force` (sidebar refresh button).
    const feature = await cacheStore.loadMrFeature(tabId, force);
    if (myVersion !== loadVersion) return; // superseded
    mrFeature = feature;
    if (!feature.enabled) {
      loading = false;
      mrs     = [];
      return;
    }

    const result = await withLoading(
      v => { if (myVersion === loadVersion) loading = v; },
      v => { if (myVersion === loadVersion) error   = v; },
      () => cacheStore.loadMrList(tabId, requestedFilter, force),
    );
    if (myVersion !== loadVersion) return; // superseded
    // The cheap probe (archived/disabled) misses some causes (fork-mirrors,
    // branch-protection blocking PRs). If list_mrs surfaces a 404, retroactively
    // mark the feature as unavailable so the sidebar + palette gate kicks in.
    if (result === null && error && /\bnot found\b/i.test(error) && /404/.test(error)) {
      const status: MrFeatureStatus = {
        enabled: false,
        reason:  info.provider === 'gitlab'
          ? 'Merge requests are not available on this GitLab project.'
          : 'Pull requests are not available on this GitHub repository.',
      };
      mrFeature = status;
      cacheStore.setMrFeature(tabId, status);
      error = null;
      mrs   = [];
      return;
    }
    mrs = result ?? [];
  }

  /// Load (or return from cache) the MR list across *all* states — open +
  /// merged + closed in a single list. Independent from `load()` / `mrs` /
  /// `stateFilter` so the sidebar's filter selection isn't disturbed when a
  /// different surface (e.g. the Command Palette MR autocomplete) needs the
  /// full set. Runs the same provider + feature gates as `load()` so a repo
  /// without a configured GitHub/GitLab remote, or with MR/PRs disabled,
  /// resolves to an empty list rather than surfacing a raw error.
  async function loadAll(tabId: string, force = false) {
    if (allLoading) return;
    // Treat any prior load for the same tab as "cached" — even an empty
    // result counts (no token, archived repo, …) so we don't hammer the
    // network every time the user opens the palette on such a repo.
    if (!force && allLoadedTab === tabId) return;

    allLoading = true;
    try {
      // Provider must exist and have a token, otherwise we'd hit a guaranteed
      // failure path inside `cacheStore.loadMrList`. Mirrors `load()`'s gate.
      const info = await detectProvider(tabId);
      if (!info || !info.has_token) {
        allMrs       = [];
        allLoadedTab = tabId;
        return;
      }
      const feature = await cacheStore.loadMrFeature(tabId, force);
      mrFeature = feature;
      if (!feature.enabled) {
        allMrs       = [];
        allLoadedTab = tabId;
        return;
      }
      const list = await cacheStore.loadMrList(tabId, 'all', force).catch(() => [] as MergeRequest[]);
      allMrs       = list;
      allLoadedTab = tabId;
    } finally {
      allLoading = false;
    }
  }

  /// Drop the cached "all states" list — used when the active tab changes so
  /// the next consumer (Command Palette) does a fresh fetch instead of
  /// showing the previous tab's MRs.
  function clearAll() {
    allMrs       = [];
    allLoadedTab = null;
  }

  async function loadDetail(tabId: string, number: number) {
    activeNumber = number;
    detail       = null;
    const result = await withLoading(
      v => { detailLoading = v; },
      v => { detailError = v; },
      () => getMrDetail(tabId, number),
    );
    detail = result;
  }

  function clearDetail() {
    activeNumber  = null;
    detail        = null;
    detailError   = null;
    detailLoading = false;
  }

  function setFilter(f: MrStateFilter) {
    stateFilter = f;
  }

  return {
    get mrs()           { return mrs; },
    get loading()       { return loading; },
    get error()         { return error; },
    get stateFilter()   { return stateFilter; },
    get activeNumber()  { return activeNumber; },
    get detail()        { return detail; },
    get detailLoading() { return detailLoading; },
    get detailError()   { return detailError; },
    get provider()      { return provider; },
    get providerInfo()  { return providerInfo; },
    get mrFeature()     { return mrFeature; },
    get allMrs()        { return allMrs; },
    get allLoading()    { return allLoading; },
    get allLoadedTab()  { return allLoadedTab; },
    load,
    loadAll,
    clearAll,
    loadDetail,
    clearDetail,
    setFilter,
    detectProvider,
  };
}

export const mrStore = createMrStore();
