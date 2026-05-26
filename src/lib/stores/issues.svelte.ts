import type {
  Issue, IssueFilters, IssueFilterOptions, IssueAuthStatus, IssueSortField, IssueSortDir,
} from '$lib/types/issues';
import {
  linearGetAuthStatus, linearSearchIssues, linearGetIssue, linearGetFilterOptions,
  linearTransitionIssue, linearAddComment, linearCreateIssue,
  linearSaveToken, linearLogout,
  jiraGetAuthStatus, jiraSearchIssues, jiraGetIssue, jiraGetFilterOptions,
  jiraTransitionIssue, jiraAddComment, jiraCreateIssue,
  jiraSaveBasicAuth, jiraLogout,
} from '$lib/ipc/issues';
import { getIssuesConfig, setIssuesConfig, getRepoConfig } from '$lib/ipc/config';
import { tabsStore } from '$lib/stores/tabs.svelte';

export type IssueProvider = 'linear' | 'jira';

function createIssuesStore() {
  // ── Provider (drives the sidebar's view) ───────────────────────────────────
  // The detail modal carries its own `selectedIssueProvider` snapshot so it
  // keeps working when the sidebar switches to a different tracker.
  let activeProvider = $state<IssueProvider | null>(null);

  // ── Auth state ──────────────────────────────────────────────────────────────
  let authStatus   = $state<IssueAuthStatus | null>(null);
  let authLoading  = $state(false);

  // ── Filter options cache (per-provider) ─────────────────────────────────────
  // The sidebar reads via `filterOptions` (active provider). The detail modal
  // reads via `getFilterOptionsFor(provider)` so a parked Linear modal can
  // pull Linear statuses even while the sidebar is on Jira.
  let filterOptionsByProvider      = $state<Partial<Record<IssueProvider, IssueFilterOptions>>>({});
  let filterOptionsErrorByProvider = $state<Partial<Record<IssueProvider, string>>>({});

  // ── Default project filter (set from repo config, applied automatically) ────
  let defaultProjectId = $state<string | null>(null);

  // ── Active filters ──────────────────────────────────────────────────────────
  let filters = $state<IssueFilters>({ assigneeMe: false, statusIds: [], labelIds: [], issueTypeIds: [] });

  // ── Issue list ──────────────────────────────────────────────────────────────
  let issues       = $state<Issue[]>([]);
  let loading      = $state(false);
  let error        = $state<string | null>(null);

  // ── Selected issue (for detail modal) ──────────────────────────────────────
  // Decoupled from `activeProvider` so switching the sidebar between trackers
  // never nukes a parked detail modal. The modal carries its own provider via
  // `selectedIssueProvider` (snapshot at open time).
  let selectedIssue          = $state<Issue | null>(null);
  let selectedIssueProvider  = $state<IssueProvider | null>(null);
  let detailLoading          = $state(false);
  // Tab the issue was opened from. Captured synchronously when `selectIssue`
  // / `selectAndLoadIssue` runs so the parked-dialog restore action and the
  // Linked Commits panel can pin to the original repo even after tab switches.
  let selectedIssueSourceTab = $state<string | null>(null);

  // ── Create issue modal ──────────────────────────────────────────────────────
  let createOpen = $state(false);

  // ── Sort (loaded from config.toml on first use) ───────────────────────────
  let sortField = $state<IssueSortField>('updated_at');
  let sortDir   = $state<IssueSortDir>('desc');
  let sortLoaded = false;

  async function ensureSortLoaded() {
    if (sortLoaded) return;
    sortLoaded = true;
    try {
      const cfg = await getIssuesConfig();
      sortField = (cfg.sort_field as IssueSortField) || 'updated_at';
      sortDir   = (cfg.sort_dir   as IssueSortDir)   || 'desc';
    } catch { /* use defaults */ }
  }

  // Kick off the load immediately (fire-and-forget; reactive vars will update).
  void ensureSortLoaded();

  // ── Context menu ────────────────────────────────────────────────────────────
  let contextMenuIssue = $state<Issue | null>(null);
  let contextMenuPos   = $state<{ x: number; y: number } | null>(null);

  // ---------------------------------------------------------------------------

  /**
   * Switch the sidebar's provider and reset sidebar-scoped state. Auth is
   * *not* fetched here — `IssuesSidebar` has a dedicated `$effect` that calls
   * `loadAuthStatus()` when `authStatus === null && activeProvider !== null`,
   * so the work happens on demand only when the sidebar is actually mounted.
   *
   * Note: this does NOT touch `selectedIssue` / `selectedIssueProvider` /
   * `selectedIssueSourceTab`. The detail modal lives independently so a
   * parked Linear ticket survives a sidebar switch to Jira and vice-versa.
   */
  function setProvider(p: IssueProvider | null) {
    if (activeProvider === p) return;
    activeProvider     = p;
    authStatus         = null;
    issues             = [];
    error              = null;
    filters            = { assigneeMe: false, statusIds: [], labelIds: [], issueTypeIds: [] };
  }

  async function loadAuthStatus() {
    authLoading = true;
    try {
      if (activeProvider === 'jira') {
        const s = await jiraGetAuthStatus();
        authStatus = { authenticated: s.authenticated, user: s.user, domain: s.domain, authMethod: s.authMethod };
      } else {
        authStatus = await linearGetAuthStatus();
      }
      if (authStatus?.authenticated) {
        await Promise.all([loadIssues(), loadFilterOptions()]);
      }
    } catch { /* ignore */ } finally {
      authLoading = false;
    }
  }

  /** Save a Linear Personal API token. */
  async function saveToken(token: string) {
    authLoading = true;
    try {
      const user = await linearSaveToken(token);
      authStatus = { authenticated: true, user };
      await Promise.all([loadIssues(), loadFilterOptions()]);
    } finally {
      authLoading = false;
    }
  }

  /** Save Jira Basic Auth credentials (email + API token + domain). */
  async function saveJiraBasicAuth(email: string, apiToken: string, domain: string) {
    authLoading = true;
    try {
      const user = await jiraSaveBasicAuth(email, apiToken, domain);
      authStatus = { authenticated: true, user, domain, authMethod: domain.endsWith('.atlassian.net') ? 'basic' : 'pat' };
      await Promise.all([loadIssues(), loadFilterOptions()]);
    } finally {
      authLoading = false;
    }
  }

  async function logout() {
    if (activeProvider === 'jira') {
      await jiraLogout();
    } else {
      await linearLogout();
    }
    authStatus    = { authenticated: false, user: null };
    issues        = [];
    error         = null;
    // Keep the detail modal alive — logging out of Jira from the sidebar
    // should not wipe a parked Linear ticket.
  }

  /**
   * Load filter options for a provider (defaults to the sidebar's active one).
   * Results land in the per-provider cache, so the detail modal can pull
   * statuses for whichever tracker its ticket belongs to — independent of
   * what the sidebar is currently showing.
   */
  async function loadFilterOptions(provider?: IssueProvider) {
    const p = provider ?? activeProvider;
    if (!p) return;
    // Clear any previous error for this provider before re-attempting.
    if (filterOptionsErrorByProvider[p]) {
      const next = { ...filterOptionsErrorByProvider };
      delete next[p];
      filterOptionsErrorByProvider = next;
    }
    try {
      const opts = p === 'jira' ? await jiraGetFilterOptions() : await linearGetFilterOptions();
      filterOptionsByProvider = { ...filterOptionsByProvider, [p]: opts };
    } catch (e) {
      filterOptionsErrorByProvider = { ...filterOptionsErrorByProvider, [p]: String(e) };
    }
  }

  function getFilterOptionsFor(provider: IssueProvider): IssueFilterOptions | null {
    return filterOptionsByProvider[provider] ?? null;
  }

  function getFilterOptionsErrorFor(provider: IssueProvider): string | null {
    return filterOptionsErrorByProvider[provider] ?? null;
  }

  async function loadIssues() {
    loading = true;
    error   = null;
    try {
      // Merge in the per-repo default project filter (can be overridden by user filters).
      // For Jira, projects map to `teamId`. For Linear they map to `projectId`.
      const defaultApplied = defaultProjectId
        && (activeProvider === 'jira' ? !filters.teamId : !filters.projectId);
      const effectiveFilters: IssueFilters = defaultApplied
        ? {
            ...filters,
            ...(activeProvider === 'jira'
              ? { teamId: defaultProjectId! }
              : { projectId: defaultProjectId! }),
          }
        : filters;
      if (activeProvider === 'jira') {
        issues = await jiraSearchIssues(effectiveFilters);
      } else {
        issues = await linearSearchIssues(effectiveFilters);
      }
    } catch (e) {
      error  = String(e);
      issues = [];
    } finally {
      loading = false;
    }
  }

  /** Natural sort for identifiers like "ENG-12", "ENG-100", "#42". */
  function compareIdentifier(a: string, b: string): number {
    // Split on the last numeric segment: "ENG-12" → prefix="ENG-", num=12
    const re = /^(.*?)(\d+)$/;
    const ma = re.exec(a);
    const mb = re.exec(b);
    if (ma && mb) {
      const prefixCmp = ma[1].localeCompare(mb[1]);
      if (prefixCmp !== 0) return prefixCmp;
      return parseInt(ma[2], 10) - parseInt(mb[2], 10);
    }
    return a.localeCompare(b);
  }

  function setSort(field: IssueSortField, dir: IssueSortDir) {
    sortField = field;
    sortDir   = dir;
    void setIssuesConfig({ sort_field: field, sort_dir: dir });
  }

  const sortedIssues = $derived((() => {
    const arr = [...issues];
    const dir = sortDir === 'asc' ? 1 : -1;
    arr.sort((a, b) => {
      switch (sortField) {
        case 'ticket_id':  return dir * compareIdentifier(a.identifier, b.identifier);
        case 'updated_at': return dir * (new Date(a.updatedAt).getTime() - new Date(b.updatedAt).getTime());
        case 'created_at': return dir * (new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime());
        case 'priority':   return dir * (a.priority - b.priority);
        case 'title':      return dir * a.title.localeCompare(b.title);
        case 'status': {
          const order = ['backlog','unstarted','started','completed','cancelled'];
          const ai = order.indexOf(a.status.statusType);
          const bi = order.indexOf(b.status.statusType);
          return dir * ((ai === -1 ? 99 : ai) - (bi === -1 ? 99 : bi));
        }
        default: return 0;
      }
    });
    return arr;
  })());

  function setFilters(f: Partial<IssueFilters>) {
    filters = { ...filters, ...f };
  }

  function clearFilters() {
    filters = { assigneeMe: false, statusIds: [], labelIds: [], issueTypeIds: [], projectId: undefined, milestoneId: undefined };
  }

  /**
   * Resolve the active provider from the tab's repo config (`issue_tracker`
   * field) and update the store. Null is set when no tracker is configured —
   * callers (e.g. the activity bar) can use this to drive a per-repo brand
   * icon vs. a generic fallback. Failures are swallowed silently because the
   * activity bar must never throw on a missing/unreadable config.
   */
  async function loadProviderForTab(tabId: string) {
    try {
      const cfg = await getRepoConfig(tabId);
      const t   = cfg.issue_tracker ?? null;
      setProvider(t === 'linear' || t === 'jira' ? t : null);
      setDefaultProjectId(cfg.issue_tracker_project_id ?? null, t ?? undefined);
    } catch {
      setProvider(null);
    }
  }

  /** Set the per-repo default project filter (from .arbor/config.toml). Reloads issues. */
  function setDefaultProjectId(id: string | null, _provider?: string) {
    if (defaultProjectId === id) return;
    defaultProjectId = id;
    if (authStatus?.authenticated) void loadIssues();
  }

  /**
   * Select an issue (light data, no detail fetch). Provider defaults to the
   * sidebar's active one — but callers that know the tracker explicitly
   * (e.g. the ticket-chip deep-link handler) should pass it to avoid a
   * mismatch when the chip's tracker differs from the active tab's.
   */
  function selectIssue(issue: Issue | null, provider?: IssueProvider) {
    selectedIssue          = issue;
    selectedIssueProvider  = issue ? (provider ?? activeProvider) : null;
    selectedIssueSourceTab = issue ? tabsStore.activeTabId : null;
  }

  /**
   * Select an issue and immediately fetch its full detail (description + comments).
   * Sets `selectedIssue` optimistically right away (light data) so the modal opens
   * instantly, then replaces it with the full payload once the API call returns.
   *
   * The provider is captured synchronously and used for the detail fetch, so a
   * Linear ticket opened from a Linear-configured repo will keep talking to
   * Linear even if the user switches the sidebar to a Jira repo mid-flight.
   *
   * `sourceTabId` lets the parked-dialog dock restore the modal with its
   * original source tab pinned, instead of capturing whatever tab happens
   * to be active at restore time.
   */
  async function selectAndLoadIssue(issue: Issue, provider?: IssueProvider, sourceTabId?: string) {
    const p = provider ?? activeProvider;
    // Optimistic: show modal immediately with whatever data we already have.
    selectedIssue          = issue;
    selectedIssueProvider  = p;
    selectedIssueSourceTab = sourceTabId ?? tabsStore.activeTabId;
    detailLoading          = true;
    try {
      if (!p) return;
      const full = p === 'jira'
        ? await jiraGetIssue(issue.identifier)
        : await linearGetIssue(issue.id);
      // Only apply if the user hasn't navigated away to a different issue.
      if (selectedIssue?.id === issue.id) selectedIssue = full;
    } catch { /* keep light version — description will just be empty */ } finally {
      detailLoading = false;
    }
  }

  function openCreate()  { createOpen = true; }
  function closeCreate() { createOpen = false; }

  function openContextMenu(issue: Issue, x: number, y: number) {
    contextMenuIssue = issue;
    contextMenuPos   = { x, y };
  }

  function closeContextMenu() {
    contextMenuIssue = null;
    contextMenuPos   = null;
  }

  /**
   * Transition an issue's status. Provider defaults to the sidebar's active
   * one — callers driven by the detail modal MUST pass the modal's pinned
   * provider so a Linear modal sitting on top of a Jira-active sidebar still
   * routes to Linear's API.
   *
   * The list `issues[]` is only updated when the provider matches the active
   * one, otherwise we'd be mixing trackers in a single list.
   */
  async function transitionIssue(id: string, statusId: string, provider?: IssueProvider) {
    const p = provider ?? activeProvider;
    let updated: Issue;
    if (p === 'jira') {
      updated = await jiraTransitionIssue(id, statusId);
    } else {
      updated = await linearTransitionIssue(id, statusId);
    }
    if (p === activeProvider) {
      issues = issues.map(i => i.id === id ? updated : i);
    }
    if (selectedIssue?.id === id) selectedIssue = updated;
    return updated;
  }

  /** Same provider-routing logic as `transitionIssue`. */
  async function addComment(issueId: string, body: string, provider?: IssueProvider) {
    const p = provider ?? activeProvider;
    let comment;
    if (p === 'jira') {
      comment = await jiraAddComment(issueId, body);
    } else {
      comment = await linearAddComment(issueId, body);
    }
    if (selectedIssue?.id === issueId) {
      selectedIssue = {
        ...selectedIssue,
        comments:     [...selectedIssue.comments, comment],
        commentCount: selectedIssue.commentCount + 1,
      };
    }
    return comment;
  }

  async function createIssue(params: Parameters<typeof linearCreateIssue>[0] & { issueType?: string }) {
    let issue: Issue;
    if (activeProvider === 'jira') {
      issue = await jiraCreateIssue(params);
    } else {
      issue = await linearCreateIssue(params);
    }
    issues = [issue, ...issues];
    closeCreate();
    return issue;
  }

  return {
    // state
    get activeProvider()   { return activeProvider; },
    get authStatus()       { return authStatus; },
    get authLoading()      { return authLoading; },
    /** Filter options for the sidebar's active provider (legacy alias). */
    get filterOptions()    { return activeProvider ? (filterOptionsByProvider[activeProvider] ?? null) : null; },
    get filterOptionsError() { return activeProvider ? (filterOptionsErrorByProvider[activeProvider] ?? null) : null; },
    get filters()          { return filters; },
    get issues()           { return issues; },
    get loading()          { return loading; },
    get error()            { return error; },
    get selectedIssue()    { return selectedIssue; },
    get selectedIssueProvider()  { return selectedIssueProvider; },
    get selectedIssueSourceTab() { return selectedIssueSourceTab; },
    get detailLoading()    { return detailLoading; },
    get createOpen()       { return createOpen; },
    get contextMenuIssue() { return contextMenuIssue; },
    get contextMenuPos()   { return contextMenuPos; },
    get defaultProjectId() { return defaultProjectId; },
    get sortField()        { return sortField; },
    get sortDir()          { return sortDir; },
    get sortedIssues()     { return sortedIssues; },
    // actions
    setProvider, loadProviderForTab,
    loadAuthStatus, saveToken, saveJiraBasicAuth, logout,
    loadFilterOptions, getFilterOptionsFor, getFilterOptionsErrorFor, loadIssues,
    setFilters, clearFilters, setDefaultProjectId, setSort,
    selectIssue, selectAndLoadIssue, openCreate, closeCreate,
    openContextMenu, closeContextMenu,
    transitionIssue, addComment, createIssue,
  };
}

export const issuesStore = createIssuesStore();
