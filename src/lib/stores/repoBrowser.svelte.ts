import type {
  RemoteAccount, RemoteRepo, RemoteTreeEntry,
  RemoteFileContent, NamespaceTreeNode
} from '$lib/types/repoBrowser';
import {
  rbListAccounts, rbListRepos, rbBrowseTree,
  rbGetFileContent, rbDownloadFile
} from '$lib/ipc/repoBrowser';
import { cacheStore } from './cache.svelte';

// ---------------------------------------------------------------------------
// Repo-list cache (per provider) with TTL
// ---------------------------------------------------------------------------
//
// "List all repos" against GitHub / GitLab is slow on large accounts (200+
// projects).  We persist results to localStorage so reopening the modal
// after a brief close is instant; entries past the configured TTL are
// transparently refreshed in the background of the next open.
//
// Key shape: `arbor:repoBrowser:repos:<provider>`.
// Payload: `{ fetchedAt: ms, repos: RemoteRepo[] }`.

const CACHE_KEY_PREFIX = 'arbor:repoBrowser:repos:';

interface RepoCacheEntry {
  fetchedAt: number;
  repos:     RemoteRepo[];
}

function cacheKey(provider: string): string { return `${CACHE_KEY_PREFIX}${provider}`; }

function loadCache(provider: string): RepoCacheEntry | null {
  try {
    const raw = localStorage.getItem(cacheKey(provider));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as RepoCacheEntry;
    if (!parsed.fetchedAt || !Array.isArray(parsed.repos)) return null;
    return parsed;
  } catch { return null; }
}

function saveCache(provider: string, repos: RemoteRepo[]): void {
  try {
    const entry: RepoCacheEntry = { fetchedAt: Date.now(), repos };
    localStorage.setItem(cacheKey(provider), JSON.stringify(entry));
  } catch { /* localStorage full or unavailable — silently degrade */ }
}

function clearCache(provider?: string): void {
  try {
    if (provider) {
      localStorage.removeItem(cacheKey(provider));
      return;
    }
    // Wipe every provider entry.  Iterating keys avoids touching unrelated
    // arbor:* entries.
    const toDelete: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k && k.startsWith(CACHE_KEY_PREFIX)) toDelete.push(k);
    }
    toDelete.forEach(k => localStorage.removeItem(k));
  } catch { /* ignore */ }
}

function isCacheFresh(entry: RepoCacheEntry): boolean {
  const ttlSecs = cacheStore.config.repo_browser_ttl_secs;
  if (!ttlSecs || ttlSecs <= 0) return false; // ttl=0 disables the cache
  const ageMs = Date.now() - entry.fetchedAt;
  return ageMs < ttlSecs * 1000;
}

// ---------------------------------------------------------------------------
// Fuzzy-score helper (same logic as CommandPalette)
// ---------------------------------------------------------------------------

function fuzzyScore(query: string, target: string): number {
  if (!query) return 100;
  const q = query.toLowerCase();
  const t = target.toLowerCase();
  if (t === q)               return 100;
  if (t.startsWith(q))      return 85;
  if (t.includes(q))        return 55;
  // word-boundary match
  const words = t.split(/[\s/_\-.]/)
  if (words.some(w => w.startsWith(q))) return 70;
  // fuzzy char-by-char
  let qi = 0;
  for (let i = 0; i < t.length && qi < q.length; i++) {
    if (t[i] === q[qi]) qi++;
  }
  return qi === q.length ? 30 : 0;
}

// ---------------------------------------------------------------------------
// Breadcrumb segment
// ---------------------------------------------------------------------------

export interface BreadcrumbSegment {
  name:  string;
  path:  string;   // empty string = repo root
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

function createRepoBrowserStore() {
  // ── Accounts / repo list ──────────────────────────────────────────────────
  let accounts         = $state<RemoteAccount[]>([]);
  let selectedProvider = $state<string | null>(null);
  let repos            = $state<RemoteRepo[]>([]);
  let reposLoading     = $state(false);
  let reposError       = $state<string | null>(null);
  /** When the currently displayed repo list was fetched.  Drives the
   *  "Last updated · Xm ago" hint next to the refresh button.  null when
   *  no list has been loaded for the current provider. */
  let reposFetchedAt   = $state<number | null>(null);
  /** True while the displayed list came from a cache that's still within
   *  its TTL.  The user can click refresh to force-bypass it. */
  let reposFromCache   = $state(false);

  // ── Search ────────────────────────────────────────────────────────────────
  let searchQuery      = $state('');

  // ── Namespace groups (collapsed state persisted in memory) ───────────────
  let expandedNs       = $state<Set<string>>(new Set());

  // ── Selected repo + file tree ─────────────────────────────────────────────
  let selectedRepo     = $state<RemoteRepo | null>(null);
  let currentBranch    = $state('');
  let breadcrumbs      = $state<BreadcrumbSegment[]>([]);
  let treeEntries      = $state<RemoteTreeEntry[]>([]);
  let treeLoading      = $state(false);
  let treeError        = $state<string | null>(null);

  // ── File preview ─────────────────────────────────────────────────────────
  let selectedFile     = $state<RemoteTreeEntry | null>(null);
  let fileContent      = $state<RemoteFileContent | null>(null);
  let fileLoading      = $state(false);
  let fileError        = $state<string | null>(null);

  // ── Derived: filtered + grouped repos ────────────────────────────────────
  const filteredRepos = $derived.by(() => {
    const q = searchQuery.trim();
    if (!q) return repos;
    return repos
      .map(r => ({ repo: r, score: Math.max(
        fuzzyScore(q, r.name),
        fuzzyScore(q, r.full_name),
        fuzzyScore(q, r.namespace),
        r.description ? fuzzyScore(q, r.description) * 0.5 : 0,
      )}))
      .filter(x => x.score > 0)
      .sort((a, b) => b.score - a.score)
      .map(x => x.repo);
  });

  const namespaceGroups = $derived.by((): NamespaceTreeNode[] => {
    const forceExpand = searchQuery.trim() !== '';
    const allNodes    = new Map<string, NamespaceTreeNode>();

    function getOrCreate(fullPath: string): NamespaceTreeNode {
      if (!allNodes.has(fullPath)) {
        allNodes.set(fullPath, {
          segment:  fullPath.split('/').pop()!,
          fullPath,
          repos:    [],
          children: [],
          expanded: forceExpand || expandedNs.has(fullPath),
        });
      }
      return allNodes.get(fullPath)!;
    }

    for (const repo of filteredRepos) {
      // Ensure every ancestor path exists
      const parts = repo.namespace.split('/');
      for (let i = 1; i <= parts.length; i++) {
        getOrCreate(parts.slice(0, i).join('/'));
      }
      getOrCreate(repo.namespace).repos.push(repo);
    }

    // Wire parent → children
    const roots: NamespaceTreeNode[] = [];
    for (const [fullPath, node] of allNodes) {
      const parts = fullPath.split('/');
      if (parts.length === 1) {
        roots.push(node);
      } else {
        const parentPath = parts.slice(0, -1).join('/');
        const parent = allNodes.get(parentPath);
        if (parent) parent.children.push(node);
        else         roots.push(node);
      }
    }

    function sortNode(n: NamespaceTreeNode) {
      n.children.sort((a, b) => a.segment.localeCompare(b.segment));
      n.repos.sort((a, b) => a.name.localeCompare(b.name));
      n.children.forEach(sortNode);
    }

    roots.sort((a, b) => a.segment.localeCompare(b.segment));
    roots.forEach(sortNode);
    return roots;
  });

  const currentPath = $derived(
    breadcrumbs.length > 0
      ? breadcrumbs[breadcrumbs.length - 1].path
      : ''
  );

  // ── Account management ────────────────────────────────────────────────────

  async function loadAccounts() {
    accounts = await rbListAccounts();
    if (accounts.length > 0 && !selectedProvider) {
      await selectProvider(accounts[0].provider);
    }
  }

  async function selectProvider(provider: string) {
    if (selectedProvider === provider && repos.length > 0) return;
    selectedProvider = provider;
    selectedRepo     = null;
    treeEntries      = [];
    fileContent      = null;
    breadcrumbs      = [];
    await loadRepos(provider);
  }

  /**
   * Load the repo list for a provider, honouring the localStorage cache by
   * default.  Pass `forceRefresh=true` to bypass the cache (refresh button).
   *
   * Cache rules:
   * - Hit && fresh         → return cached, no network call
   * - Hit && stale         → show cached immediately, refetch in background
   * - Hit && forceRefresh  → discard, refetch synchronously
   * - Miss                 → fetch synchronously
   */
  async function loadRepos(provider: string, forceRefresh = false) {
    reposError = null;

    const cached = forceRefresh ? null : loadCache(provider);
    if (cached) {
      // Show the cached list immediately so the modal feels instant.
      applyRepoList(cached.repos);
      reposFetchedAt = cached.fetchedAt;
      reposFromCache = true;
      if (isCacheFresh(cached)) return;
      // Stale: kick a background refresh but keep the cached list visible
      // so the user can already start browsing.
      backgroundRefresh(provider);
      return;
    }

    reposLoading = true;
    try {
      const fetched = await rbListRepos(provider);
      applyRepoList(fetched);
      saveCache(provider, fetched);
      reposFetchedAt = Date.now();
      reposFromCache = false;
    } catch (err) {
      reposError = String(err).replace(/^.*error:/i, '').trim();
      repos      = [];
    } finally {
      reposLoading = false;
    }
  }

  /** Force-refresh: discard cache and re-fetch.  Drives the refresh button. */
  async function refreshRepos() {
    if (!selectedProvider) return;
    await loadRepos(selectedProvider, true);
  }

  /** Quietly refetch in the background and swap the list when done.  Used
   *  when we showed a stale-but-valid cache: the user sees the old list
   *  immediately and gets the fresh one transparently. */
  function backgroundRefresh(provider: string) {
    rbListRepos(provider).then(fetched => {
      // Bail if the user navigated away to a different provider mid-flight.
      if (selectedProvider !== provider) return;
      applyRepoList(fetched);
      saveCache(provider, fetched);
      reposFetchedAt = Date.now();
      reposFromCache = false;
    }).catch(() => { /* keep showing stale — error stays silent in background */ });
  }

  function applyRepoList(list: RemoteRepo[]) {
    repos = list;
    if (list.length > 0 && expandedNs.size === 0) {
      expandedNs.add(list[0].namespace.split('/')[0]);
      expandedNs = new Set(expandedNs);
    }
  }

  /** Wipe the on-disk repo cache for one provider (or all if omitted).
   *  Called by the Settings → Cache "Clear repo browser cache" button. */
  function clearRepoCache(provider?: string) {
    clearCache(provider);
    if (!provider || provider === selectedProvider) {
      reposFetchedAt = null;
      reposFromCache = false;
    }
  }

  // ── Namespace toggle ──────────────────────────────────────────────────────

  function toggleNamespace(ns: string) {
    if (expandedNs.has(ns)) {
      expandedNs.delete(ns);
    } else {
      expandedNs.add(ns);
    }
    expandedNs = new Set(expandedNs);
  }

  // ── Repo selection ────────────────────────────────────────────────────────

  async function selectRepo(repo: RemoteRepo) {
    selectedRepo  = repo;
    currentBranch = repo.default_branch;
    breadcrumbs   = [];
    selectedFile  = null;
    fileContent   = null;
    await loadTree('');
  }

  async function switchBranch(branch: string) {
    currentBranch = branch;
    breadcrumbs   = [];
    selectedFile  = null;
    fileContent   = null;
    await loadTree('');
  }

  // ── Tree navigation ────────────────────────────────────────────────────────

  async function navigateToDir(entry: RemoteTreeEntry) {
    breadcrumbs = [
      ...breadcrumbs,
      { name: entry.name, path: entry.path },
    ];
    selectedFile = null;
    fileContent  = null;
    await loadTree(entry.path);
  }

  async function navigateToBreadcrumb(idx: number) {
    if (idx < 0) {
      // root
      breadcrumbs  = [];
      selectedFile = null;
      fileContent  = null;
      await loadTree('');
    } else {
      breadcrumbs  = breadcrumbs.slice(0, idx + 1);
      selectedFile = null;
      fileContent  = null;
      await loadTree(breadcrumbs[idx].path);
    }
  }

  async function loadTree(path: string) {
    if (!selectedRepo) return;
    treeLoading = true;
    treeError   = null;
    treeEntries = [];
    try {
      treeEntries = await rbBrowseTree(
        selectedRepo.provider, selectedRepo.full_name, path, currentBranch
      );
    } catch (err) {
      treeError = String(err).replace(/^.*error:/i, '').trim();
    } finally {
      treeLoading = false;
    }
  }

  // ── File preview ──────────────────────────────────────────────────────────

  async function selectFile(entry: RemoteTreeEntry) {
    if (!selectedRepo) return;
    selectedFile = entry;
    fileLoading  = true;
    fileError    = null;
    fileContent  = null;
    try {
      fileContent = await rbGetFileContent(
        selectedRepo.provider, selectedRepo.full_name, entry.path, currentBranch
      );
    } catch (err) {
      fileError = String(err).replace(/^.*error:/i, '').trim();
    } finally {
      fileLoading = false;
    }
  }

  async function downloadFile(entry: RemoteTreeEntry, destPath: string) {
    if (!selectedRepo) return;
    await rbDownloadFile(
      selectedRepo.provider, selectedRepo.full_name, entry.path, currentBranch, destPath
    );
  }

  // ── Close file preview ────────────────────────────────────────────────────

  function closeFilePreview() {
    selectedFile = null;
    fileContent  = null;
    fileError    = null;
  }

  // ── Reset ─────────────────────────────────────────────────────────────────

  function reset() {
    selectedRepo  = null;
    treeEntries   = [];
    fileContent   = null;
    selectedFile  = null;
    breadcrumbs   = [];
    searchQuery   = '';
  }

  return {
    // State getters
    get accounts()         { return accounts; },
    get selectedProvider() { return selectedProvider; },
    get repos()            { return repos; },
    get reposLoading()     { return reposLoading; },
    get reposError()       { return reposError; },
    get reposFetchedAt()   { return reposFetchedAt; },
    get reposFromCache()   { return reposFromCache; },
    get searchQuery()      { return searchQuery; },
    get filteredRepos()    { return filteredRepos; },
    get namespaceGroups()  { return namespaceGroups; },
    get selectedRepo()     { return selectedRepo; },
    get currentBranch()    { return currentBranch; },
    get breadcrumbs()      { return breadcrumbs; },
    get currentPath()      { return currentPath; },
    get treeEntries()      { return treeEntries; },
    get treeLoading()      { return treeLoading; },
    get treeError()        { return treeError; },
    get selectedFile()     { return selectedFile; },
    get fileContent()      { return fileContent; },
    get fileLoading()      { return fileLoading; },
    get fileError()        { return fileError; },

    // Setters
    set searchQuery(v: string) { searchQuery = v; },

    // Actions
    loadAccounts,
    selectProvider,
    refreshRepos,
    clearRepoCache,
    toggleNamespace,
    selectRepo,
    switchBranch,
    navigateToDir,
    navigateToBreadcrumb,
    selectFile,
    downloadFile,
    closeFilePreview,
    reset,
  };
}

export const repoBrowserStore = createRepoBrowserStore();
