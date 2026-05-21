import type {
  SecuritySummary, SecurityFinding, SecurityFilters, Severity,
  SeverityCounts, FindingState,
} from '$lib/types/security';
import { emptySecurityFilters, SEVERITY_ORDER } from '$lib/types/security';
import {
  supportsSecurity, fetchSecuritySummary, fetchSecurityFindings,
} from '$lib/ipc/security';

const STORAGE_KEY_RANGE         = 'arbor:security:range';
const STORAGE_KEY_SEVERITIES    = 'arbor:security:severity-filter';
const STORAGE_KEY_REPORT_TYPES  = 'arbor:security:report-filter';
const STORAGE_KEY_SCOPE         = 'arbor:security:state-scope';
// Note: search query is intentionally NOT persisted (matches the convention
// of other filterable panels in the app — feels noisy across sessions).

/** Two scopes for the detail-modal state filter:
 *   - `'active'` (default) → only `Detected | Confirmed` findings
 *   - `'closed'`           → only `Resolved | Dismissed` findings
 *  The dashboard counts always reflect `'active'` regardless of scope —
 *  the scope only narrows the modal's row list. */
export type FindingStateScope = 'active' | 'closed';
const STATES_ACTIVE: FindingState[] = ['detected', 'confirmed'];
const STATES_CLOSED: FindingState[] = ['resolved',  'dismissed'];

type RangeDays = 30 | 60 | 90;

function loadRange(): RangeDays {
  if (typeof localStorage === 'undefined') return 30;
  const raw = localStorage.getItem(STORAGE_KEY_RANGE);
  const n = raw ? parseInt(raw, 10) : 30;
  return n === 60 || n === 90 ? n : 30;
}

function loadSeverities(): Severity[] {
  if (typeof localStorage === 'undefined') return [];
  const raw = localStorage.getItem(STORAGE_KEY_SEVERITIES);
  if (!raw) return [];
  const valid = new Set<string>(SEVERITY_ORDER);
  return raw.split(',').filter(s => valid.has(s)) as Severity[];
}

function loadReportTypes(): string[] {
  if (typeof localStorage === 'undefined') return [];
  const raw = localStorage.getItem(STORAGE_KEY_REPORT_TYPES);
  return raw ? raw.split(',').filter(Boolean) : [];
}

function loadScope(): FindingStateScope {
  if (typeof localStorage === 'undefined') return 'active';
  return localStorage.getItem(STORAGE_KEY_SCOPE) === 'closed' ? 'closed' : 'active';
}

function persistList(key: string, values: string[]) {
  if (typeof localStorage === 'undefined') return;
  if (values.length === 0) localStorage.removeItem(key);
  else                     localStorage.setItem(key, values.join(','));
}

function emptyCounts(): SeverityCounts {
  return { critical: 0, high: 0, medium: 0, low: 0, info: 0, unknown: 0 };
}

function createSecurityStore() {
  // Per-tab support cache so the activity bar gating doesn't re-probe on
  // every tab activation. Keyed by tab id; absent ⇒ unknown (yet to probe).
  //
  // Plain record (not Map) on purpose: Svelte 5's `$state` proxy reliably
  // tracks property reads/writes on objects, but Map mutations don't
  // always propagate to downstream `$derived` reads — consumers like the
  // StatusBar chip and the SecurityPanel were missing the cache update
  // until they re-mounted.
  const providerSupport = $state<Record<string, boolean>>({});

  let summary       = $state<SecuritySummary | null>(null);
  let findings      = $state<SecurityFinding[]>([]);
  let loading       = $state(false);
  /** Set while a filter-driven re-fetch is in flight (separate from the
   *  initial summary load so the UI can dim the list without rebooting
   *  the whole panel). */
  let findingsLoading = $state(false);
  let error         = $state<string | null>(null);
  let rangeDays     = $state<RangeDays>(loadRange());
  const initialScope: FindingStateScope = loadScope();
  let stateScope    = $state<FindingStateScope>(initialScope);
  let filters       = $state<SecurityFilters>({
    ...emptySecurityFilters(),
    severities:   loadSeverities(),
    report_types: loadReportTypes(),
    states:       initialScope === 'closed' ? [...STATES_CLOSED] : [...STATES_ACTIVE],
  });
  /** Tab the current `summary` / `findings` snapshot belongs to. */
  let snapshotTabId = $state<string | null>(null);

  /** Whether the active tab's provider supports the dashboard. */
  function providerSupportsSecurity(tabId: string | null): boolean {
    if (!tabId) return false;
    return providerSupport[tabId] === true;
  }

  /** Tri-state probe result for UI gating:
   *   - `'unknown'`     → no probe has finished yet for this tab (show spinner)
   *   - `'supported'`   → the provider exposes the dashboard for this repo
   *   - `'unsupported'` → no GitHub/GitLab remote, missing token, feature off,
   *                       or the GraphQL/REST probe came back negative
   *
   *  The dedicated `'unknown'` slot is what lets the SecurityPanel show a
   *  loading state while AppShell's probe is in flight, instead of flashing
   *  the "not available" copy and then swapping to data. */
  function providerSupportState(tabId: string | null): 'unknown' | 'supported' | 'unsupported' {
    if (!tabId) return 'unsupported';
    if (!(tabId in providerSupport)) return 'unknown';
    return providerSupport[tabId] ? 'supported' : 'unsupported';
  }

  /**
   * Probe `supports_security` for a tab and cache the result. Called from
   * the AppShell on `set_active_tab` so the gating UI has the answer ready
   * before the panel/statusbar try to render.
   */
  async function probeSupport(tabId: string | null): Promise<boolean> {
    if (!tabId) return false;
    if (tabId in providerSupport) return providerSupport[tabId];
    try {
      const ok = await supportsSecurity(tabId);
      providerSupport[tabId] = ok;
      return ok;
    } catch {
      providerSupport[tabId] = false;
      return false;
    }
  }

  /** Drop the cached probe result (e.g. after the user signs in / changes token). */
  function invalidateSupport(tabId?: string) {
    if (tabId) {
      delete providerSupport[tabId];
    } else {
      for (const k of Object.keys(providerSupport)) delete providerSupport[k];
    }
  }

  /** In-flight `loadSummary` promises keyed by tab id, used to deduplicate
   *  the AppShell pre-load and the SecurityPanel mount-time fetch. Without
   *  this, both paths fire concurrently, both reset `loading`/`summary`,
   *  and the panel can latch onto a stale `loading=true` state when the
   *  effects interleave just so. */
  const loadInFlight = new Map<string, Promise<void>>();
  /** Same trick for findings — the filter bar can also trigger a fetch
   *  while the mount-time one is still running. */
  const findingsInFlight = new Map<string, Promise<void>>();

  function setRangeDays(days: RangeDays) {
    if (days === rangeDays) return;
    rangeDays = days;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY_RANGE, String(days));
    }
  }

  /** Load the headline summary for the active tab. Always clears the
   *  cached findings list — those are scoped to a (tab, filters) pair and
   *  must be re-fetched on demand whenever the summary is refreshed.
   *
   *  Concurrent calls for the same `tabId` share a single IPC: the second
   *  caller awaits the first instead of firing its own fetch. This is
   *  what keeps the AppShell pre-load and the SecurityPanel mount fetch
   *  from racing each other into a stuck loading state. */
  async function loadSummary(tabId: string | null) {
    if (!tabId) {
      findings = [];
      summary = null;
      snapshotTabId = null;
      return;
    }
    const existing = loadInFlight.get(tabId);
    if (existing) return existing;
    const p = (async () => {
      findings = [];
      loading = true;
      error   = null;
      try {
        summary = await fetchSecuritySummary(tabId, rangeDays);
        snapshotTabId = tabId;
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
        summary = null;
      } finally {
        loading = false;
      }
    })();
    loadInFlight.set(tabId, p);
    try { await p; } finally { loadInFlight.delete(tabId); }
  }

  /** Plug a fresh summary in without going through the IPC again — used by
   *  the `arbor://security-refresh` listener so a plugin-driven refresh
   *  (auto-refresh plugin, manual `arbor.security.refresh_active_tab()`
   *  call) instantly updates the dashboard without a second round-trip.
   *
   *  Findings are intentionally cleared: they're scoped to a (tab, filters)
   *  pair and may be stale relative to the new summary. The detail modal
   *  re-fetches them lazily when next opened, same as after a manual
   *  refresh. */
  function applySummary(tabId: string, next: SecuritySummary) {
    if (!tabId) return;
    summary       = next;
    findings      = [];
    snapshotTabId = tabId;
    error         = null;
  }

  /** Load the findings list for the current `filters`. Same deduplication
   *  guarantee as `loadSummary` — the filter bar fires its own re-fetch
   *  on every change, and that can collide with the mount-time fetch. */
  async function loadFindings(tabId: string | null) {
    if (!tabId) { findings = []; return; }
    const existing = findingsInFlight.get(tabId);
    if (existing) return existing;
    const p = (async () => {
      findingsLoading = true;
      error           = null;
      try {
        findings = await fetchSecurityFindings(tabId, filters);
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
        findings = [];
      } finally {
        findingsLoading = false;
      }
    })();
    findingsInFlight.set(tabId, p);
    try { await p; } finally { findingsInFlight.delete(tabId); }
  }

  function setFilters(next: SecurityFilters) {
    filters = next;
    persistList(STORAGE_KEY_SEVERITIES,   next.severities);
    persistList(STORAGE_KEY_REPORT_TYPES, next.report_types);
  }

  function setSearch(q: string) {
    // Search is host-side and intentionally not persisted.
    filters = { ...filters, search: q.trim() ? q : null };
  }

  function toggleSeverity(sev: Severity) {
    const has = filters.severities.includes(sev);
    const next = has
      ? filters.severities.filter(s => s !== sev)
      : [...filters.severities, sev];
    filters = { ...filters, severities: next };
    persistList(STORAGE_KEY_SEVERITIES, next);
  }

  function toggleReportType(rt: string) {
    const has = filters.report_types.includes(rt);
    const next = has
      ? filters.report_types.filter(r => r !== rt)
      : [...filters.report_types, rt];
    filters = { ...filters, report_types: next };
    persistList(STORAGE_KEY_REPORT_TYPES, next);
  }

  function clearFilters() {
    // Reset severity / report-type / search but **keep the state scope** —
    // the user's choice of "view active vs view closed" is orthogonal to
    // the narrow-by-attribute filters. Clearing it on every Clear-button
    // tap would be surprising.
    filters = {
      ...emptySecurityFilters(),
      states: stateScope === 'closed' ? [...STATES_CLOSED] : [...STATES_ACTIVE],
    };
    persistList(STORAGE_KEY_SEVERITIES,   []);
    persistList(STORAGE_KEY_REPORT_TYPES, []);
  }

  /** Switch between active-only and closed-only finding scopes. The
   *  detail modal calls this; the panel's count grid stays unaffected
   *  because `filteredCounts` always counts active. */
  function setStateScope(scope: FindingStateScope) {
    if (scope === stateScope) return;
    stateScope = scope;
    filters = {
      ...filters,
      states: scope === 'closed' ? [...STATES_CLOSED] : [...STATES_ACTIVE],
    };
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY_SCOPE, scope);
    }
  }

  /** True if any *narrowing* filter is currently active. The state scope
   *  is excluded — it's a "view mode", not a narrowing filter, and always
   *  has a non-empty value. */
  function hasActiveFilters(): boolean {
    return filters.severities.length > 0
        || filters.report_types.length > 0
        || (filters.search?.trim().length ?? 0) > 0;
  }

  /**
   * Counts for the dashboard counter grid, **always restricted to active
   * states** (Detected | Confirmed). Decouples the panel from the detail
   * modal's state-scope toggle: flipping the modal to "Closed" doesn't
   * make the dashboard suddenly show 50 high findings that are actually
   * all resolved.
   *
   * Behaviour:
   *   - `summary` not loaded yet → all-zeros placeholder
   *   - Scope is `'closed'`, no narrowing filter active, or no findings
   *     loaded → use `summary.counts` (active-only, enforced backend-side)
   *   - Scope is `'active'` AND a severity / type / search filter is on →
   *     count from the loaded active findings (which already match the
   *     filter, since `loadFindings` round-trips it through the backend)
   */
  function filteredCounts(): SeverityCounts {
    if (!summary) return emptyCounts();
    if (stateScope !== 'active' || !hasActiveFilters() || findings.length === 0) {
      return summary.counts;
    }
    const out = emptyCounts();
    for (const f of findings) {
      if (f.state !== 'detected' && f.state !== 'confirmed') continue;
      out[f.severity]++;
    }
    return out;
  }

  /** Distinct report types observed in the current findings list. The
   *  filter dropdown reads this so the option list auto-populates from
   *  whatever the provider returned — we never hard-code GitLab/GitHub
   *  report-type strings on the frontend. */
  function availableReportTypes(): string[] {
    const set = new Set<string>();
    for (const f of findings) {
      if (f.report_type) set.add(f.report_type);
    }
    return Array.from(set).sort();
  }

  function reset() {
    summary         = null;
    findings        = [];
    loading         = false;
    findingsLoading = false;
    error           = null;
    snapshotTabId   = null;
    stateScope      = 'active';
    filters         = {
      ...emptySecurityFilters(),
      states: [...STATES_ACTIVE],
    };
    persistList(STORAGE_KEY_SEVERITIES,   []);
    persistList(STORAGE_KEY_REPORT_TYPES, []);
    if (typeof localStorage !== 'undefined') {
      localStorage.removeItem(STORAGE_KEY_SCOPE);
    }
  }

  return {
    // state getters
    get summary()         { return summary; },
    get findings()        { return findings; },
    get loading()         { return loading; },
    get findingsLoading() { return findingsLoading; },
    get error()           { return error; },
    get rangeDays()       { return rangeDays; },
    get filters()         { return filters; },
    get snapshotTabId()   { return snapshotTabId; },
    get stateScope()      { return stateScope; },

    // gating
    providerSupportsSecurity,
    providerSupportState,
    probeSupport,
    invalidateSupport,

    // derived helpers
    hasActiveFilters,
    filteredCounts,
    availableReportTypes,

    // actions
    setRangeDays,
    setStateScope,
    loadSummary,
    loadFindings,
    applySummary,
    setFilters,
    setSearch,
    toggleSeverity,
    toggleReportType,
    clearFilters,
    reset,
  };
}

export const securityStore = createSecurityStore();
