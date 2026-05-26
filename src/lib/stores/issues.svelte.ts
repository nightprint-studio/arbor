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
  // ── Provider ────────────────────────────────────────────────────────────────
  let activeProvider = $state<IssueProvider | null>(null);

  // ── Auth state ──────────────────────────────────────────────────────────────
  let authStatus   = $state<IssueAuthStatus | null>(null);
  let authLoading  = $state(false);

  // ── Filter options (teams, statuses, labels…) ───────────────────────────────
  let filterOptions      = $state<IssueFilterOptions | null>(null);
  let filterOptionsError = $state<string | null>(null);

  // ── Default project filter (set from repo config, applied automatically) ────
  let defaultProjectId = $state<string | null>(null);

  // ── Active filters ──────────────────────────────────────────────────────────
  let filters = $state<IssueFilters>({ assigneeMe: false, statusIds: [], labelIds: [], issueTypeIds: [] });

  // ── Issue list ──────────────────────────────────────────────────────────────
  let issues       = $state<Issue[]>([]);
  let loading      = $state(false);
  let error        = $state<string | null>(null);

  // ── Selected issue (for detail modal) ──────────────────────────────────────
  let selectedIssue = $state<Issue | null>(null);
  let detailLoading = $state(false);
  // Tab the issue was opened from. Captured synchronously when `selectIssue`
  // / `selectAndLoadIssue` runs so the parked-dialog restore action can
  // return to the same repo even after workspace switches. Null when no
  // issue is selected.
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
   * Switch provider and reset all issue state. Auth is *not* fetched here —
   * `IssuesSidebar` has a dedicated `$effect` that calls `loadAuthStatus()`
   * when `authStatus === null && activeProvider !== null`, so the work
   * happens on demand only when the sidebar is actually mounted. This lets
   * lightweight callers (e.g. the activity bar's per-tab brand-icon
   * resolver) call `setProvider` without triggering network round-trips
   * for issues/auth on every tab switch.
   */
  function setProvider(p: IssueProvider | null) {
    if (activeProvider === p) return;
    activeProvider  = p;
    authStatus      = null;
    issues          = [];
    filterOptions   = null;
    filterOptionsError = null;
    error           = null;
    selectedIssue   = null;
    selectedIssueSourceTab = null;
    filters         = { assigneeMe: false, statusIds: [], labelIds: [], issueTypeIds: [] };
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
    filterOptions = null;
    selectedIssue = null;
    selectedIssueSourceTab = null;
    error         = null;
  }

  async function loadFilterOptions() {
    filterOptionsError = null;
    try {
      if (activeProvider === 'jira') {
        filterOptions = await jiraGetFilterOptions();
      } else {
        filterOptions = await linearGetFilterOptions();
      }
    } catch (e) {
      filterOptionsError = String(e);
    }
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

  function selectIssue(issue: Issue | null) {
    selectedIssue = issue;
    selectedIssueSourceTab = issue ? tabsStore.activeTabId : null;
  }

  /**
   * Select an issue and immediately fetch its full detail (description + comments).
   * Sets `selectedIssue` optimistically right away (light data) so the modal opens
   * instantly, then replaces it with the full payload once the API call returns.
   */
  async function selectAndLoadIssue(issue: Issue) {
    // Optimistic: show modal immediately with whatever data we already have.
    selectedIssue = issue;
    selectedIssueSourceTab = tabsStore.activeTabId;
    detailLoading  = true;
    try {
      const full = activeProvider === 'jira'
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

  async function transitionIssue(id: string, statusId: string) {
    let updated: Issue;
    if (activeProvider === 'jira') {
      updated = await jiraTransitionIssue(id, statusId);
    } else {
      updated = await linearTransitionIssue(id, statusId);
    }
    issues = issues.map(i => i.id === id ? updated : i);
    if (selectedIssue?.id === id) selectedIssue = updated;
    return updated;
  }

  async function addComment(issueId: string, body: string) {
    let comment;
    if (activeProvider === 'jira') {
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
    get filterOptions()    { return filterOptions; },
    get filters()          { return filters; },
    get issues()           { return issues; },
    get loading()          { return loading; },
    get error()            { return error; },
    get selectedIssue()    { return selectedIssue; },
    get selectedIssueSourceTab() { return selectedIssueSourceTab; },
    get detailLoading()    { return detailLoading; },
    get createOpen()       { return createOpen; },
    get contextMenuIssue() { return contextMenuIssue; },
    get contextMenuPos()   { return contextMenuPos; },
    get filterOptionsError() { return filterOptionsError; },
    get defaultProjectId()   { return defaultProjectId; },
    get sortField()          { return sortField; },
    get sortDir()            { return sortDir; },
    get sortedIssues()       { return sortedIssues; },
    // actions
    setProvider, loadProviderForTab,
    loadAuthStatus, saveToken, saveJiraBasicAuth, logout,
    loadFilterOptions, loadIssues,
    setFilters, clearFilters, setDefaultProjectId, setSort,
    selectIssue, selectAndLoadIssue, openCreate, closeCreate,
    openContextMenu, closeContextMenu,
    transitionIssue, addComment, createIssue,
  };
}

export const issuesStore = createIssuesStore();
