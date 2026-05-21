import { getMrDetail } from '$lib/ipc/mr';
import type { MergeRequest, MrDetail } from '$lib/types/mr';
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
  /**
   * undefined = not yet checked
   * null      = checked, no GitHub/GitLab remote found
   * object    = provider detected
   */
  let providerInfo  = $state<CiProviderInfo | null | undefined>(undefined);

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

    loading = true;
    error   = null;

    // Always detect provider first (fast, no token needed — result is cached)
    const info = await detectProvider(tabId);
    if (myVersion !== loadVersion) return; // superseded

    if (!info || !info.has_token) {
      // No remote or no token — don't attempt the API call
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
    mrs = result ?? [];
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
    load,
    loadDetail,
    clearDetail,
    setFilter,
    detectProvider,
  };
}

export const mrStore = createMrStore();
