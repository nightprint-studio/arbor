import type { GraphData, CommitNode, CommitDetail, EdgeType, StashRef } from '../types/git';
import type { StashEntry } from '../types/git';

export type PanelMode = 'commit' | 'stash' | 'workdir';

function createGraphStore() {
  let graphData        = $state<GraphData | null>(null);
  /** Tab id whose data lives in `graphData`.  Updated by `setGraph(data, tabId)`.
   *  Lets cross-tab consumers (deep-link dispatcher) tell "graph for this tab
   *  is loaded" from "graph for previous tab is still in memory". */
  let graphDataTabId   = $state<string | null>(null);
  let selectedOid      = $state<string | null>(null);
  let selectedDetail   = $state<CommitDetail | null>(null);
  let isLoading        = $state(false);
  // Raw value bound to the search input; filtered results use `searchQueryDebounced`
  // so we don't re-run an O(n) .filter() over thousands of commits on every keystroke.
  let searchQuery        = $state('');
  let searchQueryDebounced = $state('');
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let highlightedOids    = $state<Set<string>>(new Set());
  let highlightedList    = $state<string[]>([]);  // ordered — used for prev/next navigation
  let currentMatchIdx    = $state(0);
  let scrollToBranchName    = $state<string | null>(null);
  let highlightedBranchName = $state<string | null>(null);
  let scrollToHeadTick    = $state(0);
  let scrollToCommitOid   = $state<string | null>(null);

  // ── Graph-loaded readiness signal ──────────────────────────────────────
  // Push-based notification used by the deep-link dispatcher to wait until
  // the graph for a freshly-activated tab has been populated.  Without this
  // a `scrollToCommit` from the dispatcher fires before `data` is loaded,
  // and even when the effect re-runs on data change it races the auto-
  // scroll-to-HEAD effect (which uses tick().then and wins the race).
  // Subscribe BEFORE triggering the work that should produce the signal,
  // so the notify-fired-then-await never deadlocks.
  const graphLoadedWaiters = new Map<string, Array<() => void>>();

  function notifyGraphLoaded(tabId: string) {
    const queue = graphLoadedWaiters.get(tabId);
    if (!queue || queue.length === 0) return;
    graphLoadedWaiters.delete(tabId);
    for (const fn of queue) fn();
  }

  function awaitGraphLoaded(tabId: string, timeoutMs = 5000): Promise<void> {
    return new Promise<void>(resolve => {
      let done = false;
      const finish = () => {
        if (done) return;
        done = true;
        const queue = graphLoadedWaiters.get(tabId);
        if (queue) {
          const idx = queue.indexOf(finish);
          if (idx >= 0) queue.splice(idx, 1);
          if (queue.length === 0) graphLoadedWaiters.delete(tabId);
        }
        resolve();
      };

      // Subscribe FIRST so a notify fired between the fast-path check and
      // setTimeout can't slip through.
      let queue = graphLoadedWaiters.get(tabId);
      if (!queue) { queue = []; graphLoadedWaiters.set(tabId, queue); }
      queue.push(finish);

      // Fast-path: if graphData is already populated for THIS tab and not
      // mid-load, resolve synchronously.  Tab-aware via `graphDataTabId`
      // — without that check, a deep-link arriving while the previous
      // tab's data is still in memory would resolve immediately and the
      // scroll would fire against the wrong commits.
      if (
        graphDataTabId === tabId &&
        graphData != null &&
        graphData.nodes.length > 0 &&
        !isLoading
      ) {
        finish();
        return;
      }

      setTimeout(finish, timeoutMs);
    });
  }
  let panelMode        = $state<PanelMode>('commit');
  let selectedStash    = $state<StashEntry | null>(null);
  /** Incremented by refresh() to force effects that track it to re-run. */
  let refreshTick      = $state(0);
  /** When set, the graph shows only commits that touched this file path. */
  let fileFilter       = $state<string | null>(null);

  const selectedNode = $derived<CommitNode | null>(
    graphData?.nodes.find(n => n.oid === selectedOid) ?? null
  );

  // Filtered list uses the *debounced* query so O(n) filtering only happens
  // once the user pauses typing (150ms window) — prevents input lag on repos
  // with tens of thousands of commits.
  const filteredNodes = $derived<CommitNode[]>(
    searchQueryDebounced.trim()
      ? (graphData?.nodes.filter(n =>
          n.oid.startsWith(searchQueryDebounced) ||
          n.summary.toLowerCase().includes(searchQueryDebounced.toLowerCase()) ||
          n.author.name.toLowerCase().includes(searchQueryDebounced.toLowerCase())
        ) ?? [])
      : (graphData?.nodes ?? [])
  );

  function setGraph(data: GraphData, tabId: string | null = null) {
    graphData = data;
    if (tabId !== null) graphDataTabId = tabId;
  }

  /** Replace the in-memory stash markers list without touching commits or
   *  edges.  Used after `stash save / drop / pop / apply` so the markers
   *  appear/disappear without paying the lane-assignment cost of a full
   *  `getGraph` round-trip.  No-op when graphData is null. */
  function setStashes(stashes: StashRef[]) {
    if (!graphData) return;
    graphData = { ...graphData, stashes };
  }

  /** Move the HEAD marker on the in-memory graph after a checkout, without
   *  re-fetching the graph from the backend (which would re-run the gitk
   *  lane-assignment over the whole history).  No-op when graphData is null
   *  or the new HEAD oid isn't in the loaded page (paginated graph + checkout
   *  to a commit beyond the page — fall back to a full `refresh()` then). */
  function applyHeadMove(newHeadOid: string) {
    if (!graphData) return;
    let changed = false;
    const newNodes = graphData.nodes.map(n => {
      const isHead = n.oid === newHeadOid;
      if (n.is_head !== isHead) {
        changed = true;
        return { ...n, is_head: isHead };
      }
      return n;
    });
    if (changed) graphData = { ...graphData, nodes: newNodes };
  }
  function appendGraph(data: GraphData)   {
    if (!graphData) { graphData = data; return; }

    // The backend now emits absolute row numbers (offset + page-local index),
    // so pages can be concatenated directly without any coordinate fixup.
    //
    // Trailing edges (parent outside the loaded page) carry `to_parent_oid`.
    // Repair them now that the parent's row AND lane are known from the new page.
    // Fixing to_lane is critical: if the parent commit's lane was computed
    // differently on the previous page (e.g. due to cross-page lane assignment),
    // the edge would draw to the wrong column.
    const newOidToRow  = new Map<string, number>();
    const newOidToLane = new Map<string, number>();
    for (const n of data.nodes) {
      newOidToRow.set(n.oid, n.row);
      newOidToLane.set(n.oid, n.lane);
    }

    const repairedOldEdges = graphData.edges.map(e => {
      if (e.to_parent_oid) {
        const actualRow  = newOidToRow.get(e.to_parent_oid);
        const actualLane = newOidToLane.get(e.to_parent_oid);
        if (actualRow !== undefined) {
          // eslint-disable-next-line @typescript-eslint/no-unused-vars
          const { to_parent_oid: _, ...rest } = e;
          const toL = actualLane ?? e.to_lane;
          // Apply the same closing/opening color rule used by the Rust backend:
          // closing edge (from_lane > to_lane) → feature branch color (from_lane)
          // straight / opening (from_lane <= to_lane) → destination color (to_lane)
          const colorIndex = actualLane !== undefined
            ? (e.from_lane > toL ? e.from_lane % 10 : toL % 10)
            : e.color_index;
          // Recompute edge_type direction now that we know the actual to_lane.
          // The Rust backend encodes fork/merge distinction as ForkLeft/MergeLeft
          // for trailing edges (placeholder direction); fix the direction here.
          const isMerge = e.edge_type === 'merge_left' || e.edge_type === 'merge_right';
          const isFork  = e.edge_type === 'fork_left'  || e.edge_type === 'fork_right';
          let edgeType: EdgeType = e.edge_type;
          if (isMerge || isFork) {
            if (e.from_lane > toL)      edgeType = isMerge ? 'merge_left' : 'fork_left';
            else if (e.from_lane < toL) edgeType = isMerge ? 'merge_right' : 'fork_right';
            else                         edgeType = 'straight';
          }
          return {
            ...rest,
            to_row:      actualRow,
            to_lane:     toL,
            color_index: colorIndex,
            edge_type:   edgeType,
          };
        }
      }
      return e;
    });

    graphData = {
      ...data,
      lane_count:    Math.max(graphData.lane_count, data.lane_count),
      total_commits: data.total_commits,
      nodes: [...graphData.nodes, ...data.nodes],
      edges: [...repairedOldEdges, ...data.edges],
    };
  }
  function selectCommit(oid: string | null) {
    selectedOid = oid;
    panelMode = 'commit';
    selectedStash = null;
    highlightedBranchName = null;
    if (!oid) selectedDetail = null;
  }
  function setDetail(detail: CommitDetail | null) { selectedDetail = detail; }
  function setLoading(v: boolean)                 { isLoading = v; }
  function setSearch(q: string) {
    searchQuery = q;
    // Debounce the derived-filter trigger: short queries update instantly,
    // anything longer waits 150ms so fast typing doesn't filter N times.
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    if (!q.trim()) {
      searchQueryDebounced = '';
    } else {
      searchDebounceTimer = setTimeout(() => { searchQueryDebounced = q; }, 150);
    }
  }
  function setHighlighted(oids: string[]) {
    highlightedOids  = new Set(oids);
    highlightedList  = oids;
    currentMatchIdx  = 0;
  }

  function nextMatch() {
    if (highlightedList.length === 0) return;
    currentMatchIdx = (currentMatchIdx + 1) % highlightedList.length;
  }

  function prevMatch() {
    if (highlightedList.length === 0) return;
    currentMatchIdx = (currentMatchIdx - 1 + highlightedList.length) % highlightedList.length;
  }
  function scrollToBranch(name: string)  { scrollToBranchName = name; highlightedBranchName = name; }
  function setHighlightedBranch(name: string | null) { highlightedBranchName = name; }
  function clearScrollTarget()           { scrollToBranchName = null; }
  function scrollToHead()                { scrollToHeadTick++; }
  function scrollToCommit(oid: string)   { selectCommit(oid); scrollToCommitOid = oid; }
  function clearScrollToCommit()         { scrollToCommitOid = null; }
  function setPanelMode(mode: PanelMode) { panelMode = mode; }
  function setSelectedStash(stash: StashEntry | null) {
    selectedStash = stash;
    panelMode = stash ? 'stash' : 'commit';
    selectedOid = null;
    selectedDetail = null;
  }
  function setWorkdirMode() {
    panelMode = 'workdir';
    selectedOid = null;
    selectedDetail = null;
    selectedStash = null;
  }

  /** Trigger a full graph + sidebar reload by bumping the refresh counter.
   *
   *  Also flips `isLoading` to true synchronously so consumers that subscribe
   *  to `awaitGraphLoaded` *between* this call and the loadGraph effect's
   *  next microtask see "load in progress" instead of fast-resolving against
   *  the stale data.  loadGraph itself sets isLoading on entry, so this is
   *  effectively a no-op except for that synchronous gap. */
  function refresh() { isLoading = true; refreshTick++; }

  function filterByFile(path: string) { fileFilter = path; refreshTick++; }
  function clearFileFilter()          { fileFilter = null; refreshTick++; }

  function clear() {
    graphData = null; graphDataTabId = null; selectedOid = null; selectedDetail = null;
    isLoading = false; searchQuery = ''; searchQueryDebounced = ''; highlightedOids = new Set();
    highlightedList = []; currentMatchIdx = 0;
    scrollToBranchName = null; highlightedBranchName = null; scrollToHeadTick = 0; scrollToCommitOid = null; panelMode = 'commit';
    selectedStash = null; fileFilter = null;
  }

  return {
    get graphData()          { return graphData; },
    get graphDataTabId()     { return graphDataTabId; },
    get selectedOid()        { return selectedOid; },
    get selectedDetail()     { return selectedDetail; },
    get selectedNode()       { return selectedNode; },
    get isLoading()          { return isLoading; },
    get searchQuery()        { return searchQuery; },
    get highlightedOids()    { return highlightedOids; },
    get highlightedList()    { return highlightedList; },
    get currentMatchIdx()    { return currentMatchIdx; },
    get currentMatchOid()    { return highlightedList[currentMatchIdx] ?? null; },
    get filteredNodes()      { return filteredNodes; },
    get scrollToBranchName()      { return scrollToBranchName; },
    get highlightedBranchName()   { return highlightedBranchName; },
    get scrollToHeadTick()    { return scrollToHeadTick; },
    get scrollToCommitOid()   { return scrollToCommitOid; },
    get panelMode()          { return panelMode; },
    get selectedStash()      { return selectedStash; },
    get refreshTick()        { return refreshTick; },
    get fileFilter()         { return fileFilter; },
    notifyGraphLoaded, awaitGraphLoaded,
    setGraph, appendGraph, applyHeadMove, setStashes, selectCommit, setDetail,
    refresh, filterByFile, clearFileFilter,
    setLoading, setSearch, setHighlighted,
    nextMatch, prevMatch,
    scrollToBranch, setHighlightedBranch, clearScrollTarget, scrollToHead, scrollToCommit, clearScrollToCommit,
    setPanelMode, setSelectedStash, setWorkdirMode,
    clear,
  };
}

export const graphStore = createGraphStore();
