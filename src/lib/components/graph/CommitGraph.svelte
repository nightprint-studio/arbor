<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import GraphNode from './GraphNode.svelte';
  import GraphEdge from './GraphEdge.svelte';
  import BranchLabel from './BranchLabel.svelte';
  import GraphSearch from './GraphSearch.svelte';
  import GraphContextMenu from './GraphContextMenu.svelte';
  import WipRow from './WipRow.svelte';
  import CreateBranchModal from './CreateBranchModal.svelte';
  import CreateTagModal from './CreateTagModal.svelte';
  import TicketChip from './TicketChip.svelte';
  import TicketPickerModal from '../shared/TicketPickerModal.svelte';
  import NotesModal from './NotesModal.svelte';
  import { PanelBottom, X, ArrowUpToLine, FileSearch, Archive, StickyNote, Download, Layers, Loader, Link2, Globe } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { bisectStore } from '$lib/stores/bisect.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { getGraph, getGraphForFile, getRepoFingerprint, exportGraphSvg } from '$lib/ipc/graph';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { graphConfigStore } from '$lib/stores/graph_config.svelte';
  import { getStatus } from '$lib/ipc/stage';
  import { getCommitDiffMeta, getWorkdirDiff } from '$lib/ipc/diff';
  import { createBranch, createTag, checkoutBranch, stashSave } from '$lib/ipc/branch';
  import { applyPostStashChange } from '$lib/utils/applyPostStashChange';
  import { getBranchPolicy, assertBranchNameAllowed } from '$lib/utils/branch-policy';
  import { pushBranch, openInBrowser } from '$lib/ipc/remote';
  import { localTagTracker } from '$lib/stores/local-tags.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { ticketLinksStore } from '$lib/stores/ticket_links.svelte';
  import { notesStore } from '$lib/stores/notes.svelte';
  import { linkedWorktreesStore } from '$lib/stores/linkedWorktrees.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import {
    svgWidth, svgHeight, ROW_HEIGHT, NODE_RADIUS,
    nodeX, nodeY, visibleRows, edgePath, laneColor,
  } from '$lib/utils/graph-renderer';
  import { ensureAvatar } from '$lib/stores/avatars.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';
  import type { CommitNode, GraphEdge as GraphEdgeData, RepoStatus, TicketLink, StashRef } from '$lib/types/git';
  import type { MergedMrHint } from '$lib/types/mr';
  import RepoActions from '../sidebar/RepoActions.svelte';

  const PAGE_SIZE = 500;
  // When pagination is disabled we request the entire history in one shot.
  const ALL_COMMITS = 999_999;

  // Date-format cache keyed by unix timestamp. `new Date().toLocaleDateString()`
  // is surprisingly expensive when called per-row per-render; since commits in
  // the same day share a timestamp-within-a-day it's a very high hit rate.
  const dateCache = new Map<number, string>();
  function formatCommitDate(timestamp: number): string {
    // Bucket by day so cache keys stay small (~1 entry per day, not per commit).
    const day = Math.floor(timestamp / 86400);
    let s = dateCache.get(day);
    if (s === undefined) {
      s = new Date(timestamp * 1000).toLocaleDateString();
      dateCache.set(day, s);
    }
    return s;
  }

  let scrollEl  = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(600);
  let scrollRafId = 0;
  // Dynamic buffer: small while scrolling (keeps DOM churn minimal), expanded
  // when the user pauses so that the next jump lands inside pre-rendered rows.
  let isScrolling = $state(false);
  let scrollStopTimer: ReturnType<typeof setTimeout> | null = null;
  const rowBuffer = $derived(isScrolling ? 80 : 300);
  let contextMenu    = $state<{ x: number; y: number; node: CommitNode } | null>(null);
  let wipContextMenu = $state<{ x: number; y: number } | null>(null);
  let bgContextMenu  = $state<{ x: number; y: number } | null>(null);
  let showExportModal = $state(false);
  let mrHints = $state<MergedMrHint[]>([]);

  // Experimental: squash-merge ghost edges (reads setting each time the graph loads).
  // Default OFF — opt-in via Settings → Experimental.  See ExperimentalSection
  // for the rationale (slow API round-trip on rate-limited tokens / slow links).
  const squashHintsEnabled = () =>
    (localStorage.getItem('arbor:experimental:squash-merge-hints') ?? 'false') === 'true';

  // Track the last tab we auto-scrolled to HEAD for — prevents re-scrolling on
  // the same tab when the user has scrolled away manually.
  let lastAutoScrolledTabId = '';

  // Modal state — lifted here so modals survive context-menu unmount.
  let modalBranchNode = $state<CommitNode | null>(null);
  let modalTagNode    = $state<CommitNode | null>(null);
  // Ticket picker modal (right-click → Link to ticket…)
  let ticketPickerNode = $state<CommitNode | null>(null);
  // Notes modal
  let notesNode = $state<CommitNode | null>(null);

  const tab = $derived(tabsStore.activeTab);
  const data = $derived(graphStore.graphData);
  const searchActive = $derived(graphStore.highlightedOids.size > 0);

  // ── Linked Worktrees badge — shows when the active tab is in a sync link ──
  const tabRepoId = $derived(
    tab ? (workspacesStore.registry.find(r => r.path === tab.path)?.id ?? null) : null,
  );
  const tabLink = $derived(linkedWorktreesStore.linkForRepo(tabRepoId));
  const tabExpectedBranch = $derived(
    tabLink && tabRepoId ? linkedWorktreesStore.expectedBranchFor(tabLink, tabRepoId) : null,
  );
  const tabOutOfSync = $derived(
    !!tabLink && !!tabExpectedBranch && !!tab?.currentBranch && tab.currentBranch !== tabExpectedBranch,
  );
  const tabLinkSyncing = $derived(tabLink ? linkedWorktreesStore.isSyncing.has(tabLink.id) : false);

  // Bisect mark map: full OID → 'bad' | 'good' | 'next'
  // Computed once per bisectStore.state change (IIFE inside $derived).
  // Template lookups are O(1) without allocating a new Map per node.
  const bisectMarks = $derived((() => {
    const s = bisectStore.state;
    const m = new Map<string, 'bad' | 'good' | 'next' | 'result'>();
    if (!s?.active) return m;
    for (const h of s.bad_hashes)  m.set(h, 'bad');
    for (const h of s.good_hashes) m.set(h, 'good');
    // Show the "next to test" ring only when a real range exists (bad + good),
    // and only if the commit isn't already explicitly marked good/bad.
    const hasGood = s.good_hashes.length > 0;
    if (hasGood && s.current_hash && !m.has(s.current_hash)) m.set(s.current_hash, 'next');
    // result_hash overrides everything — the culprit gets its own mark type.
    if (s.result_hash) m.set(s.result_hash, 'result');
    return m;
  })());

  const visRange = $derived(visibleRows(scrollTop, viewportH, rowBuffer));
  const visFirst = $derived(visRange[0]);
  const visLast  = $derived(visRange[1]);

  // Nodes are guaranteed sequential by `row` (appendGraph concatenates pages so
  // that nodes[i].row === i), so slice is O(1) and equivalent to a filter.
  const visibleNodes = $derived(
    data ? data.nodes.slice(Math.max(0, visFirst), visLast + 1) : []
  );

  // Ghost edges derived from MR hints: connect the squash-commit on the target
  // branch to the feature-branch tip using the API-provided merge_commit_sha.
  // Find a graph node by full or abbreviated commit SHA.
  function findNodeBySha(nodes: NonNullable<typeof data>['nodes'], sha: string) {
    if (!sha) return undefined;
    return nodes.find(n =>
      n.oid === sha || n.oid.startsWith(sha) || sha.startsWith(n.short_oid)
    );
  }

  const ghostEdges = $derived.by((): GraphEdgeData[] => {
    if (!data || mrHints.length === 0) return [];
    const nodes = data.nodes;
    const result: GraphEdgeData[] = [];
    for (const hint of mrHints) {
      // The feature branch tip — prefer the local ref label, fall back to headSha.
      const featureNode =
        nodes.find(n =>
          n.refs.some(r =>
            r.name === hint.sourceBranch ||
            r.name.endsWith('/' + hint.sourceBranch)
          )
        ) ?? findNodeBySha(nodes, hint.headSha);
      if (!featureNode) continue;

      // Target anchor on the destination branch:
      //   Primary  — the merge/squash commit (exists locally only after git fetch).
      //   Fallback — the destination branch tip just before the merge (baseSha),
      //              which is always present locally and lets us show the ghost edge
      //              even when the merge commit hasn't been fetched yet.
      const targetNode =
        findNodeBySha(nodes, hint.mergeCommitSha) ??
        findNodeBySha(nodes, hint.baseSha);
      if (!targetNode) continue;

      if (targetNode.oid === featureNode.oid) continue;

      // If a real merge edge already links these specific nodes (by both row AND
      // lane), skip the ghost — the graph already shows the relationship natively.
      // We must check lanes too: a straight edge on lane 0 that happens to span
      // the same rows does NOT mean these two nodes are already connected.
      const r1 = targetNode.row,  l1 = targetNode.lane;
      const r2 = featureNode.row, l2 = featureNode.lane;
      const alreadyLinked = data.edges.some(e =>
        (
          (e.from_row === r1 && e.from_lane === l1 && e.to_row === r2 && e.to_lane === l2) ||
          (e.from_row === r2 && e.from_lane === l2 && e.to_row === r1 && e.to_lane === l1)
        ) && e.edge_type !== 'squash_merge'
      );
      if (alreadyLinked) continue;

      // Always draw newer → older (smaller row → larger row).
      const [fromNode, toNode] = r1 < r2
        ? [targetNode, featureNode]
        : [featureNode, targetNode];
      result.push({
        from_row:    fromNode.row,
        from_lane:   fromNode.lane,
        to_row:      toNode.row,
        to_lane:     toNode.lane,
        color_index: featureNode.color_index,
        edge_type:   'squash_merge',
      });
    }
    return result;
  });

  // Pre-sort edges by from_row once per graphData change so visibleEdges can
  // binary-search the upper bound and skip O(E) full scans every scroll frame.
  const sortedEdges = $derived.by(() => {
    const all = [...(data?.edges ?? []), ...ghostEdges];
    all.sort((a, b) => a.from_row - b.from_row);
    return all;
  });

  const visibleEdges = $derived.by(() => {
    const arr = sortedEdges;
    // Binary search: first index where from_row > visLast + 1
    let lo = 0, hi = arr.length;
    const maxFrom = visLast + 1;
    while (lo < hi) {
      const m = (lo + hi) >>> 1;
      if (arr[m].from_row <= maxFrom) lo = m + 1; else hi = m;
    }
    // Among edges that start before/at the viewport, keep those whose span
    // reaches into it. With multi-row edges, from_row can be far above while
    // to_row sits inside the viewport — the endpoint check catches that.
    const minTo = visFirst - 1;
    const out: GraphEdgeData[] = [];
    for (let i = 0; i < lo; i++) {
      if (arr[i].to_row >= minTo) out.push(arr[i]);
    }
    return out;
  });

  const totalRows = $derived(data?.nodes.length ?? 0);
  const svgW = $derived(svgWidth(data?.lane_count ?? 1));

  // ── Stash markers ──────────────────────────────────────────────────────────
  // GitKraken-style: a small dashed line + bubble anchored on the commit the
  // stash was created from. Parent OIDs come straight from the backend's
  // stash reflog so there's no naming/metadata magic; stashes whose parent
  // isn't in the loaded graph are skipped (per user directive: "non mostrarli
  // sul grafo").
  const stashesByParentOid = $derived.by((): Map<string, StashRef[]> => {
    const m = new Map<string, StashRef[]>();
    const list = data?.stashes ?? [];
    for (const s of list) {
      const bucket = m.get(s.parentOid);
      if (bucket) bucket.push(s);
      else        m.set(s.parentOid, [s]);
    }
    // Keep stash@{N} order predictable inside each bucket (lowest index first).
    for (const arr of m.values()) arr.sort((a, b) => a.index - b.index);
    return m;
  });

  let stashPopup = $state<{ x: number; y: number; stashes: StashRef[] } | null>(null);

  /**
   * Strip the noisy prefix git stash auto-adds to messages and surface the
   * branch separately.  Two formats to handle:
   *   - automatic (`git stash`)             → `WIP on <branch>: <sha> <subject>`
   *   - explicit message (`git stash -m`)   → `On <branch>: <user-message>`
   * Anything else (custom tooling, missing prefix) falls through unchanged.
   */
  function parseStashMessage(raw: string): { branch: string | null; subject: string } {
    let m = raw.match(/^WIP on ([^:]+):\s*[0-9a-f]+\s+(.*)$/i);
    if (m) return { branch: m[1], subject: m[2] };
    m = raw.match(/^On ([^:]+):\s+(.*)$/i);
    if (m) return { branch: m[1], subject: m[2] };
    return { branch: null, subject: raw };
  }

  const stashPopupItems = $derived.by<MenuItem[]>(() => {
    if (!stashPopup) return [];
    // Interleave a separator between each stash entry so the list reads as
    // distinct items rather than a wall of text.
    const items: MenuItem[] = [];
    stashPopup.stashes.forEach((s, i) => {
      if (i > 0) items.push({ id: `sep-${s.index}`, label: '', separator: true });
      const { branch, subject } = parseStashMessage(s.message);
      const subtitle = branch
        ? `stash@{${s.index}} · on ${branch}`
        : `stash@{${s.index}}`;
      items.push({
        id:        `stash-${s.index}`,
        label:     subject || `stash@{${s.index}}`,
        subtitle,
        icon:      Archive,
        iconColor: 'var(--color-stash)',
        badge:     s.oid.slice(0, 7),
      });
    });
    return items;
  });

  function openStashPopupAt(x: number, y: number, stashes: StashRef[]) {
    stashPopup = { x, y, stashes };
  }

  async function handleStashPopupSelect(id: string) {
    const idx = parseInt(id.replace(/^stash-/, ''), 10);
    if (Number.isNaN(idx)) return;
    const popup = stashPopup;
    if (!popup) return;
    const s = popup.stashes.find(x => x.index === idx);
    stashPopup = null;
    if (!s || !tab) return;

    // Mirror StashList.selectStash: mark as selected, route the bottom panel
    // to "detail" so the diff pane is visible, then lazy-load the stash diff
    // into diffStore. Without this step the popup only selected the stash
    // but never triggered the diff fetch, so the detail pane stayed empty.
    graphStore.setSelectedStash({ index: s.index, oid: s.oid, message: s.message });
    uiStore.setActiveBottomSection('detail');
    try {
      // Lazy load: get metadata fast, hunks fetched per-file on selection.
      diffStore.setCommitContext(tab.id, s.oid);
      const files = await getCommitDiffMeta(tab.id, s.oid);
      diffStore.setFiles(files);
    } catch (err) {
      uiStore.showToast(`Failed to load stash diff: ${err}`, 'error');
    }
  }

  // Maps local branch names to their lane color_index so that remote tracking
  // branches (e.g. "origin/develop") can inherit the same color as the local
  // branch ("develop") even when they point to different commits on different lanes.
  const localBranchColorMap = $derived.by((): Map<string, number> => {
    const map = new Map<string, number>();
    if (!data) return map;
    for (const node of data.nodes) {
      for (const ref of node.refs) {
        if (ref.ref_type === 'local_branch') {
          map.set(ref.name, node.color_index);
        }
      }
    }
    return map;
  });

  // Resolves ANY ref name (local or remote) to the color_index that should be
  // used to render its label. Pre-computed once per graphData change so the
  // template hot path avoids both the regex strip and the `.get()` fallback
  // chain — it's a single Map lookup per ref per render.
  const refColorByName = $derived.by((): Map<string, number> => {
    const map = new Map<string, number>();
    if (!data) return map;
    for (const node of data.nodes) {
      for (const ref of node.refs) {
        if (ref.ref_type === 'local_branch') {
          map.set(ref.name, node.color_index);
        }
      }
    }
    for (const node of data.nodes) {
      for (const ref of node.refs) {
        if (ref.ref_type === 'remote_branch') {
          const slash = ref.name.indexOf('/');
          const bare  = slash >= 0 ? ref.name.slice(slash + 1) : ref.name;
          map.set(ref.name, map.get(bare) ?? node.color_index);
        }
      }
    }
    return map;
  });
  const svgH = $derived(svgHeight(totalRows));

  $effect(() => {
    graphStore.refreshTick;         // re-run when auto-fetch or manual refresh fires
    void graphConfigStore.ready;    // wait for config, and re-run when it arrives
    void graphConfigStore.paginate; // re-run when pagination setting changes
    if (!graphConfigStore.ready) return;
    if (tab) {
      loadGraph(tab.id);
      ticketLinksStore.clearForTab();
      ticketLinksStore.loadConfig(tab.id);
    } else {
      graphStore.clear();
    }
  });

  // Load note counts for visible nodes (only uncached OIDs).
  // Debounced so a quick scroll through 200 rows doesn't fire 200 parallel
  // invoke() calls — we wait for the viewport to settle, then issue the batch.
  let notesDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    if (!tab) return;
    // Snapshot the OIDs that need loading right now (inside the reactive block
    // so Svelte tracks `visibleNodes`), then debounce the actual IPC burst.
    const tabId = tab.id;
    const pending: string[] = [];
    for (const node of visibleNodes) {
      if (!notesStore.notesByOid.has(node.oid) && !notesStore.isLoading(node.oid)) {
        pending.push(node.oid);
      }
    }
    if (pending.length === 0) return;

    if (notesDebounceTimer) clearTimeout(notesDebounceTimer);
    notesDebounceTimer = setTimeout(() => {
      // Cap concurrency: 20 at a time is enough to fill the viewport without
      // saturating the IPC queue when the user is scrolling fast.
      for (const oid of pending.slice(0, 20)) {
        if (!notesStore.notesByOid.has(oid) && !notesStore.isLoading(oid)) {
          notesStore.load(tabId, oid);
        }
      }
    }, 80);

    return () => { if (notesDebounceTimer) clearTimeout(notesDebounceTimer); };
  });

  // Avatar loading — debounced so a fast scroll through 1000 rows doesn't fire
  // 1000 ensureAvatar() calls; we wait for the viewport to settle.
  let avatarDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const nodes = visibleNodes;
    if (avatarDebounceTimer) clearTimeout(avatarDebounceTimer);
    const tabId = tab?.id ?? null;
    avatarDebounceTimer = setTimeout(() => {
      for (const node of nodes) {
        if (!node.is_merge) ensureAvatar(node.author.email, node.author.name, tabId);
      }
    }, 80);
    return () => { if (avatarDebounceTimer) clearTimeout(avatarDebounceTimer); };
  });

  // Ticket links — debounced for the same reason: the store builds a request
  // array on every call, and we don't need to refresh links mid-drag.
  let ticketDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    if (!tab || !ticketLinksStore.isEnabled()) return;
    const tabId = tab.id;
    const nodes = visibleNodes;
    if (ticketDebounceTimer) clearTimeout(ticketDebounceTimer);
    ticketDebounceTimer = setTimeout(() => {
      const commits = nodes.map(n => ({
        sha:     n.oid,
        message: n.summary,
        refs:    n.refs.map(r => r.name),
      }));
      ticketLinksStore.fetchLinks(tabId, commits);
    }, 120);
    return () => { if (ticketDebounceTimer) clearTimeout(ticketDebounceTimer); };
  });

  // Scroll to branch on request
  $effect(() => {
    const branchName = graphStore.scrollToBranchName;
    const el = scrollEl;
    if (!branchName || !data || !el) return;
    const node = data.nodes.find(n =>
      n.refs.some(r => r.name === branchName || r.name.endsWith('/' + branchName))
    );
    graphStore.clearScrollTarget();
    if (!node) return;
    graphStore.selectCommit(node.oid);
    el.scrollTo({ top: centerScrollTop(nodeY(node.row)), behavior: 'smooth' });
  });

  // Scroll to the current search match (changes when user presses next/prev)
  $effect(() => {
    const oid = graphStore.currentMatchOid;
    if (!oid || !data || !scrollEl) return;
    const node = data.nodes.find(n => n.oid === oid);
    if (!node) return;
    scrollEl.scrollTo({ top: centerScrollTop(nodeY(node.row)), behavior: 'smooth' });
  });

  // Scroll to a specific commit OID on request (e.g. from Git Blame modal,
  // "Vai allo stash" action, etc.). When the commit is outside the currently
  // paginated page we ask the user before loading the full history: pulling
  // in thousands of commits just to find a target is expensive on big repos,
  // so the confirmation keeps the cost opt-in.
  let pendingScrollOid = $state<string | null>(null);
  let loadingFullForScroll = $state(false);
  /** scrollTop value that places `nodeY` centred in the visible area.
   *  Accounts for `.scroll-area`'s `padding-top` — the padding scrolls with
   *  the content, so `nodeY(row)` (relative to the SVG body) is offset by
   *  `padding-top` inside the scrollable coordinate system.  Without this
   *  fix the target lands `padding-top` below visual centre. */
  function centerScrollTop(rowY: number): number {
    const padTop = scrollEl ? parseFloat(getComputedStyle(scrollEl).paddingTop) || 0 : 0;
    return Math.max(0, rowY + padTop - viewportH / 2);
  }
  /** Same idea as `centerScrollTop` but biases the target toward the top of
   *  the viewport (used by HEAD-scroll: HEAD lives at the bottom of the
   *  history so we want it ~1/3 down rather than centred). */
  function topThirdScrollTop(rowY: number): number {
    const padTop = scrollEl ? parseFloat(getComputedStyle(scrollEl).paddingTop) || 0 : 0;
    return Math.max(0, rowY + padTop - viewportH / 3);
  }

  // Debounce timer for clearing `scrollToCommitOid` — see effect below.
  let scrollToCommitClearTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const oid = graphStore.scrollToCommitOid;
    // Track viewportH so any layout shift after the initial scroll
    // (bottom panel slide-in, sidebar transition, fonts loading, …)
    // re-fires this effect and re-centres the target against the new
    // viewport height.  Without this, a scroll computed against a mid-
    // animation `viewportH` lands ~5 rows off when the layout settles.
    void viewportH;
    if (!oid || !data || !scrollEl) return;
    const node = data.nodes.find(n => n.oid === oid);
    if (node) {
      scrollEl.scrollTo({ top: centerScrollTop(nodeY(node.row)), behavior: 'auto' });
      // DON'T clear scrollToCommitOid yet — keep the effect armed so it
      // re-fires on viewportH changes within the next 400ms.  Resetting
      // the debounce on every fire means a sequence of resizes (panel
      // animation) keeps re-targeting until the layout settles.
      if (scrollToCommitClearTimer) clearTimeout(scrollToCommitClearTimer);
      scrollToCommitClearTimer = setTimeout(() => {
        graphStore.clearScrollToCommit();
        scrollToCommitClearTimer = null;
      }, 400);
      // Claim the auto-scroll-to-HEAD slot for this tab so its
      // `tick().then(scrollTo HEAD)` callback (queued after `setLoading(false)`)
      // doesn't yank us back to HEAD.
      if (tab?.id) lastAutoScrolledTabId = tab.id;
      return;
    }
    // Commit not loaded. With pagination off the graph is already full, so
    // the commit really isn't in this history — nothing to do. With pagination
    // on, prompt the user: load everything to locate it?
    if (graphConfigStore.paginate && tab) {
      pendingScrollOid = oid;
    }
    graphStore.clearScrollToCommit();
  });

  async function confirmLoadFullForScroll() {
    const oid = pendingScrollOid;
    const tabId = tab?.id;
    if (!oid || !tabId) { pendingScrollOid = null; return; }
    loadingFullForScroll = true;
    try {
      const filter = graphStore.fileFilter;
      const gd = await cacheStore.loadGraph(tabId, 0, ALL_COMMITS, filter);
      if (tabId !== tabsStore.activeTabId) return;
      graphStore.setGraph(gd, tabId);
      const node = gd.nodes.find(n => n.oid === oid);
      if (!node) {
        uiStore.showToast('Commit non trovato nella history completa', 'warning');
        return;
      }
      await tick();
      if (tabId !== tabsStore.activeTabId || !scrollEl) return;
      scrollEl.scrollTo({ top: centerScrollTop(nodeY(node.row)), behavior: 'smooth' });
    } catch (e) {
      uiStore.showToast(`Load failed: ${e}`, 'error');
    } finally {
      loadingFullForScroll = false;
      pendingScrollOid = null;
    }
  }

  // Scroll to HEAD on request (from toolbar button or keyboard shortcut).
  // `data` is read inside untrack() so that appendGraph() updating graphData
  // does NOT re-trigger this effect and accidentally scroll back to top.
  $effect(() => {
    graphStore.scrollToHeadTick; // only this tick drives re-runs
    const el = scrollEl;
    if (!el) return;
    untrack(() => {
      if (!data) return;
      const head = data.nodes.find(n => n.is_head) ?? data.nodes[0];
      if (!head) return;
      graphStore.selectCommit(head.oid);
      el.scrollTo({ top: topThirdScrollTop(nodeY(head.row)), behavior: 'smooth' });
    });
  });

  // ── Auto-refresh the Commit Detail panel when selection changes ───────────
  // Every entry point that focuses a commit (graph click, Ctrl+Home, branch
  // sidebar focus, deep-link, search-match-next/prev, …) ultimately routes
  // through `graphStore.selectCommit(oid)`. Centralising the detail fetch
  // here keeps the panel in sync without each call site having to remember
  // to invoke loadCommitContent explicitly.
  $effect(() => {
    const oid    = graphStore.selectedOid;
    const mode   = graphStore.panelMode;
    const bottom = uiStore.activeBottomSection;
    if (!tab) return;
    if (!oid) return;
    if (mode !== 'commit') return;          // stash/workdir have their own loaders
    if (bottom !== 'detail') return;        // panel closed → don't preload
    if (graphStore.selectedDetail?.oid === oid) return; // already loaded
    untrack(() => { loadCommitContent(oid); });
  });

  // ── Auto-scroll to HEAD when switching tab ────────────────────────────────
  // Reactive: fires only when ALL of these are true simultaneously:
  //   1. We have a tab
  //   2. The graph for that tab has been loaded (data is set)
  //   3. isLoading is false (so the graph-body is rendered, not the spinner)
  //   4. scrollEl is bound (component is mounted)
  //   5. We haven't already auto-scrolled for this tab
  //
  // Using an effect (instead of setTimeout inside loadGraph) guarantees we
  // can't fire before DOM is ready — we also await tick() to be absolutely
  // sure Svelte has flushed its mutations before touching scrollEl.
  $effect(() => {
    const currentTabId = tab?.id;
    const d = graphStore.graphData;
    const loading = graphStore.isLoading;
    const el = scrollEl;

    if (!currentTabId) return;
    if (!d || d.nodes.length === 0) return;
    if (loading) return;
    if (!el) return;
    if (currentTabId === lastAutoScrolledTabId) return;

    // A pending scroll-to-commit (typically from the deep-link dispatcher)
    // takes priority over auto-scroll-to-HEAD.  Without this guard, the
    // commit-jump effect runs synchronously then auto-scroll-to-HEAD
    // overrides it via its `tick().then` callback (microtask wins the
    // race).  Mark the tab as auto-scrolled so we don't fight on later
    // renders, and bail out — the scroll-to-commit effect handles the rest.
    if (graphStore.scrollToCommitOid) {
      lastAutoScrolledTabId = currentTabId;
      return;
    }

    // Mark synchronously to prevent retriggering while tick() is awaited.
    lastAutoScrolledTabId = currentTabId;

    untrack(() => {
      const head = d.nodes.find(n => n.is_head);
      if (head) {
        // Wait for Svelte to flush pending DOM mutations, then scroll.
        // Without tick(), the graph-body height (svgH) may not be applied yet.
        tick().then(() => {
          if (currentTabId !== tabsStore.activeTabId) return;
          if (!scrollEl) return;
          scrollEl.scrollTo({
            top: topThirdScrollTop(nodeY(head.row)),
            behavior: 'smooth',
          });
        });
      } else if (graphConfigStore.paginate) {
        // HEAD is beyond the loaded page — load the full graph then scroll.
        scrollToHeadLoadingFull(currentTabId);
      }
    });
  });

  async function scrollToHeadLoadingFull(tabId: string) {
    if (tabId !== tabsStore.activeTabId) return;
    try {
      const filter = graphStore.fileFilter;
      const gd = await cacheStore.loadGraph(tabId, 0, ALL_COMMITS, filter);
      if (tabId !== tabsStore.activeTabId) return;
      graphStore.setGraph(gd, tabId);
      const head = gd.nodes.find(n => n.is_head);
      if (!head) return;
      // Wait for Svelte to render the full graph (the graph-body height must
      // be applied before we can scroll to a position further than the first page).
      await tick();
      if (tabId !== tabsStore.activeTabId || !scrollEl) return;
      scrollEl.scrollTo({ top: topThirdScrollTop(nodeY(head.row)), behavior: 'smooth' });
    } catch { /* non-critical — leave scroll as-is */ }
  }

  async function loadGraph(tabId: string) {
    graphStore.setLoading(true);
    let stale  = false;
    let loaded = false;
    try {
      const filter = graphStore.fileFilter;

      const limit = graphConfigStore.paginate ? PAGE_SIZE : ALL_COMMITS;

      // Clear stale hints from the previous tab so old ghost edges don't
      // flash over the new graph while the API call below is in flight.
      mrHints = [];

      // MR hints are loaded SEPARATELY (non-blocking) because they involve
      // a network round-trip to GitHub / GitLab that can take many seconds.
      // They only feed `ghostEdges` — purely visual decoration for squash-
      // merge anchors — so blocking the graph render on them was making
      // initial open take 10s+ on repos with a configured remote provider.
      // They're applied to `mrHints` reactive state when the API responds;
      // ghostEdges re-derive automatically and the squash hints fade in.
      if (squashHintsEnabled()) {
        cacheStore.loadMrHints(tabId)
          .then(hints => {
            // Drop late-arriving hints if the user has already switched tab.
            if (tabId === tabsStore.activeTabId) mrHints = hints;
          })
          .catch(() => { /* network failure is non-fatal — no ghost edges */ });
      }

      const [gd, s] = await Promise.all([
        cacheStore.loadGraph(tabId, 0, limit, filter),
        getStatus(tabId),
      ]);

      // Guard: tab changed while we were awaiting — discard stale response.
      if (tabId !== tabsStore.activeTabId) { stale = true; return; }

      // Tag the data with its owning tab so tab-aware consumers (deep-link
      // dispatcher's awaitGraphLoaded) can distinguish "graph for this tab
      // is loaded" from "previous tab's graph is still in memory".
      graphStore.setGraph(gd, tabId);
      repoStore.setStatus(s);
      // Drop the loading flag NOW (before awaiting the fingerprint or notifying
      // waiters): the template gates `<div class="graph-body" style="height:
      // {svgH}px">` on `!isLoading`, so until this flips the scroll-area's
      // scrollHeight is just the spinner's height (~tens of px).  A waiter that
      // calls `scrollTo({ top: 1780 })` against that gets capped to 0 by the
      // browser, and the deep-link "doesn't scroll".  Then we tick() so Svelte
      // flushes the {#else if} branch and the graph-body's height style lands
      // before notifyGraphLoaded fires.
      graphStore.setLoading(false);
      loaded = true;
      await tick();
      graphStore.notifyGraphLoaded(tabId);

      // Reload bisect state here (not just on tab switch) so that a hard
      // webview refresh (Ctrl+F5) restores it correctly — at this point the
      // repo is guaranteed to be registered in the backend.
      bisectStore.load(tabId);

      // After a cache miss + fresh fetch, record the repo fingerprint so the
      // scheduler can later detect whether anything has changed.
      try {
        const fp = await getRepoFingerprint(tabId);
        cacheStore.recordFingerprint(tabId, fp);
      } catch { /* non-critical */ }
    } catch (err) {
      if (tabId !== tabsStore.activeTabId) { stale = true; return; }
      uiStore.showToast(`Failed to load graph: ${err}`, 'error');
    } finally {
      // Safety net: if we threw before reaching the in-try setLoading(false),
      // make sure the spinner doesn't get stuck on screen.
      if (!stale && !loaded) graphStore.setLoading(false);
    }
  }

  let loadingMore = false;

  async function loadMore() {
    if (!graphConfigStore.paginate || loadingMore || !tab || !data || data.nodes.length >= data.total_commits) return;
    loadingMore = true;
    try {
      const filter = graphStore.fileFilter;
      const offset = data.nodes.length; // snapshot before any concurrent update
      const more = filter
        ? await getGraphForFile(tab.id, filter, offset, PAGE_SIZE)
        : await getGraph(tab.id, offset, PAGE_SIZE);
      graphStore.appendGraph(more);
    } catch { /* silently ignore */ } finally {
      loadingMore = false;
    }
  }

  async function loadCommitContent(oid: string) {
    if (!tab) return;
    try {
      // Mark commit context BEFORE awaiting so a fast second click on
      // another commit bumps the sequence and drops the previous fetch.
      diffStore.setCommitContext(tab.id, oid);
      const [detail, files] = await Promise.all([
        cacheStore.loadCommitDetail(tab.id, oid),
        getCommitDiffMeta(tab.id, oid),
      ]);
      graphStore.setDetail(detail);
      diffStore.setFiles(files);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  async function handleSelectCommit(node: CommitNode) {
    if (!tab) return;
    graphStore.selectCommit(node.oid);
    uiStore.setActiveBottomSection('detail');
    // Detail fetch is driven by the auto-refresh effect above — it triggers
    // once Svelte has flushed selectCommit + setActiveBottomSection together,
    // which also avoids the parallel fetch we used to issue here.
  }

  // ── Keyboard navigation (graph viewport must have focus) ────────────────
  // Bare Arrow / Page / Home / End move the selection through the commit list.
  // Selection follows the active search filter implicitly — we walk all nodes
  // since rendering already shows all nodes (search just highlights matches).
  function isRowFullyVisible(row: number): boolean {
    const el = scrollEl;
    if (!el) return false;
    const padTop = parseFloat(getComputedStyle(el).paddingTop) || 0;
    const yMid = nodeY(row) + padTop;
    const top = el.scrollTop;
    const bottom = top + viewportH;
    const margin = ROW_HEIGHT;
    return (yMid - ROW_HEIGHT / 2) >= (top + margin)
        && (yMid + ROW_HEIGHT / 2) <= (bottom - margin);
  }

  function ensureRowVisible(row: number) {
    if (!scrollEl) return;
    if (isRowFullyVisible(row)) return;
    scrollEl.scrollTo({ top: centerScrollTop(nodeY(row)), behavior: 'smooth' });
  }

  // Walk along the anchor's lane (= same column in the graph — gitk's lane
  // reuse means this maps to "stay on the visual branch line" rather than to
  // git's branch refs, which matches what the user sees on screen).  Falls
  // back to a single-row step when the lane has no more commits in the
  // requested direction, so the keys never feel "stuck" at branch tips.
  function moveAlongLane(idx: number, dir: 1 | -1, nodes: CommitNode[]): number {
    const last = nodes.length - 1;
    const cur = nodes[idx];
    for (let i = idx + dir; i >= 0 && i <= last; i += dir) {
      if (nodes[i].lane === cur.lane) return i;
    }
    const fb = idx + dir;
    return (fb >= 0 && fb <= last) ? fb : idx;
  }

  // Find the nearest occupied lane on the requested side (← / →) within a
  // bounded window around the anchor, then return the commit on that lane
  // whose row is closest to the current one.  Window cap keeps the scan
  // O(WINDOW) even on 100k-commit repos.
  function moveAcrossLane(idx: number, dir: 1 | -1, nodes: CommitNode[]): number {
    const last = nodes.length - 1;
    const cur = nodes[idx];
    const WINDOW = 300;
    const lo = Math.max(0, idx - WINDOW);
    const hi = Math.min(last, idx + WINDOW);
    const closestPerLane = new Map<number, { idx: number; dist: number }>();
    for (let i = lo; i <= hi; i++) {
      const n = nodes[i];
      if (dir === 1  && n.lane <= cur.lane) continue;
      if (dir === -1 && n.lane >= cur.lane) continue;
      const d = Math.abs(i - idx);
      const ex = closestPerLane.get(n.lane);
      if (!ex || d < ex.dist) closestPerLane.set(n.lane, { idx: i, dist: d });
    }
    if (closestPerLane.size === 0) return idx;
    let bestLane = -1;
    for (const lane of closestPerLane.keys()) {
      if (bestLane === -1 || Math.abs(lane - cur.lane) < Math.abs(bestLane - cur.lane)) {
        bestLane = lane;
      }
    }
    return closestPerLane.get(bestLane)!.idx;
  }

  function handleGraphKeydown(e: KeyboardEvent) {
    // Bare keys only — modifier chords (Ctrl+Home, …) belong to AppShell.
    if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
    if (!data || data.nodes.length === 0) return;

    const nodes = data.nodes;
    const last  = nodes.length - 1;
    const currentIdx = graphStore.selectedOid
      ? nodes.findIndex(n => n.oid === graphStore.selectedOid)
      : -1;
    const headIdx = nodes.findIndex(n => n.is_head);
    // Anchor: current selection → HEAD → top.
    const anchor = currentIdx >= 0 ? currentIdx : (headIdx >= 0 ? headIdx : 0);
    // Page = rows that fully fit, minus one for context overlap.
    const pageSize = Math.max(1, Math.floor(viewportH / ROW_HEIGHT) - 1);

    let target = -1;
    switch (e.key) {
      // Topology nav — ↑ / ↓ follow the current lane (visual branch line),
      // ← / → hop to the nearest occupied lane on that side.
      case 'ArrowDown':  target = moveAlongLane(anchor,  1, nodes); break;
      case 'ArrowUp':    target = moveAlongLane(anchor, -1, nodes); break;
      case 'ArrowRight': target = moveAcrossLane(anchor,  1, nodes); break;
      case 'ArrowLeft':  target = moveAcrossLane(anchor, -1, nodes); break;
      // Linear nav — Page / Home / End walk the row list, ignoring lanes.
      case 'PageDown':   target = Math.min(last, anchor + pageSize); break;
      case 'PageUp':     target = Math.max(0,    anchor - pageSize); break;
      case 'Home':       target = 0; break;
      case 'End':        target = last; break;
      default: return;
    }

    e.preventDefault();
    if (target === currentIdx) {
      // Hit the edge of the list / lane — at least keep the row on screen.
      ensureRowVisible(anchor);
      return;
    }
    const node = nodes[target];
    if (!node) return;
    void handleSelectCommit(node);
    ensureRowVisible(target);
  }

  function handleContextMenu(e: MouseEvent, node: CommitNode) {
    e.preventDefault();
    e.stopPropagation(); // don't bubble to scroll-area background handler
    contextMenu = { x: e.clientX, y: e.clientY, node };
  }

  function handleBgContextMenu(e: MouseEvent) {
    if (!tab || !data) return;
    e.preventDefault();
    bgContextMenu = { x: e.clientX, y: e.clientY };
  }

  const bgMenuItems: MenuItem[] = [
    { id: 'export-svg', label: 'Export graph as SVG…', icon: Download, iconColor: 'var(--accent)' },
  ];

  function handleBgMenuSelect(id: string) {
    bgContextMenu = null;
    if (id === 'export-svg') showExportModal = true;
  }

  async function doExportSvg(outputPath: string) {
    if (!tab) return;
    try {
      // Pass the active theme so the SVG matches the on-screen graph
      // (light themes, custom themes, lane palette overrides, …).
      await exportGraphSvg(tab.id, outputPath, themeStore.activeTheme.vars);
    } catch (err) {
      notificationsStore.add('Graph export failed', String(err), 'error');
    }
  }

  // RAF-throttled scroll handler: coalesces multiple scroll events per frame
  // (browsers on 120/144Hz displays, or intermittent bursts during scrollbar
  // drag, can fire faster than one update-per-frame). We always render the
  // LATEST scrollTop once per frame, keeping DOM work inside the frame budget
  // even during aggressive drags.
  function handleScroll(e: Event) {
    const el = e.target as HTMLElement;
    // Mark as scrolling → keeps the buffer small while moving.
    isScrolling = true;
    if (scrollStopTimer) clearTimeout(scrollStopTimer);
    // After scroll settles, flip the flag → buffer expands in the background
    // so the next jump has pre-rendered neighbours available.
    scrollStopTimer = setTimeout(() => { isScrolling = false; }, 180);

    if (scrollRafId) return;
    scrollRafId = requestAnimationFrame(() => {
      scrollRafId = 0;
      scrollTop = el.scrollTop;
      viewportH = el.clientHeight;
      if (graphConfigStore.paginate && scrollTop + viewportH > svgH - ROW_HEIGHT * 20) {
        loadMore();
      }
    });
  }

  onMount(() => {
    if (scrollEl) viewportH = scrollEl.clientHeight;

    // Track viewport height changes so scroll-to-commit / scroll-to-HEAD
    // (which compute target Y as `nodeY - viewportH / k`) never use a stale
    // value.  Without this, opening the bottom panel, dragging the splitter
    // or toggling the sidebar leaves `viewportH` pinned to whatever it was
    // at mount — the next jump centres the target against the wrong height
    // and the user sees the commit half-hidden behind the panel.
    //
    // We read `clientHeight` (padding box) instead of `entry.contentRect.height`
    // (content box) — the scrollable region IS the padding box, and using the
    // content box would lose `padding-top` from `viewportH` and skew the scroll
    // target downward by ~half a row.
    if (!scrollEl || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => {
      if (scrollEl) viewportH = scrollEl.clientHeight;
    });
    ro.observe(scrollEl);
    return () => ro.disconnect();
  });

  // Listen for new-branch keybinding event — opens branch modal at HEAD
  $effect(() => {
    function onNewBranch() {
      if (!tab) return;
      // Prefer the explicit HEAD; nodes[0] is a working assumption that breaks
      // when filteredNodes hides it or when graphData is null/loading.
      const nodes = graphStore.graphData?.nodes;
      const head  = nodes?.find(n => n.is_head) ?? nodes?.[0];
      if (!head) return;
      openCreateBranch(head);
    }
    window.addEventListener('arbor:new-branch', onNewBranch);
    return () => window.removeEventListener('arbor:new-branch', onNewBranch);
  });

  // Listen for sidebar "Create branch from here" — opens branch modal at specific oid
  $effect(() => {
    function onNewBranchFrom(e: Event) {
      const { oid } = (e as CustomEvent<{ oid: string }>).detail;
      if (!tab) return;
      const node = graphStore.graphData?.nodes.find(n => n.oid === oid);
      if (node) {
        openCreateBranch(node);
      } else {
        // oid not in current graph view; construct a minimal node
        openCreateBranch({ oid, short_oid: oid.slice(0, 7) } as CommitNode);
      }
    }
    window.addEventListener('arbor:new-branch-from', onNewBranchFrom);
    return () => window.removeEventListener('arbor:new-branch-from', onNewBranchFrom);
  });

  // Listen for palette "Create tag here" — opens tag modal at specific oid
  $effect(() => {
    function onCreateTagAt(e: Event) {
      const { oid } = (e as CustomEvent<{ oid: string }>).detail;
      if (!tab) return;
      const node = graphStore.graphData?.nodes.find(n => n.oid === oid)
        ?? ({ oid, short_oid: oid.slice(0, 7) } as CommitNode);
      openCreateTag(node);
    }
    window.addEventListener('arbor:create-tag-at', onCreateTagAt);
    return () => window.removeEventListener('arbor:create-tag-at', onCreateTagAt);
  });

  // Listen for jump-to-head keybinding event
  $effect(() => {
    function onJumpToHead() { graphStore.scrollToHead(); }
    window.addEventListener('arbor:jump-to-head', onJumpToHead);
    return () => window.removeEventListener('arbor:jump-to-head', onJumpToHead);
  });

  // Listen for focus-graph keybinding event — pulls focus into the scroll
  // area so the bare Arrow / Page / Home / End keys start driving navigation.
  $effect(() => {
    function onFocusGraph() { scrollEl?.focus(); }
    window.addEventListener('arbor:focus-graph', onFocusGraph);
    return () => window.removeEventListener('arbor:focus-graph', onFocusGraph);
  });

  // Navigate to a specific commit OID — fired by Command Palette (tags, commits)
  $effect(() => {
    function onShowCommit(e: Event) {
      const { oid } = (e as CustomEvent<{ oid: string }>).detail;
      if (!oid) return;
      graphStore.scrollToCommit(oid);
      uiStore.setActiveBottomSection('detail');
      void loadCommitContent(oid);
    }
    window.addEventListener('arbor:show-commit', onShowCommit);
    return () => window.removeEventListener('arbor:show-commit', onShowCommit);
  });

  // ── Modal callbacks (called by GraphContextMenu, handled here) ──────────

  function openCreateBranch(node: CommitNode) {
    contextMenu = null;
    modalBranchNode = node;
  }

  function openCreateTag(node: CommitNode) {
    contextMenu = null;
    modalTagNode = node;
  }

  function openLinkTicket(node: CommitNode) {
    contextMenu = null;
    ticketPickerNode = node;
  }

  function openNotes(node: CommitNode) {
    contextMenu = null;
    notesNode = node;
    // Ensure notes are loaded for this commit
    if (tab) notesStore.load(tab.id, node.oid);
  }

  function handleChipClick(link: TicketLink) {
    // Dispatch a DOM event so the issues sidebar / detail modal can react.
    window.dispatchEvent(new CustomEvent('arbor:view-issue', {
      detail: { tracker: link.tracker, ticketId: link.ticket_id },
    }));
  }

  async function doCreateBranch(name: string) {
    if (!tab || !modalBranchNode) return;
    const { oid, is_head: fromHead } = modalBranchNode;
    // Honour the require-ticket-branch policy for ALL branch creation paths,
    // not just GitFlow feature start. Mirrors the validator used by the
    // command palette's `Create Branch` verb.
    try {
      const policy = await getBranchPolicy(tab.id);
      const err    = assertBranchNameAllowed(name.trim(), policy);
      if (err) {
        uiStore.showToast(err, 'warning');
        return; // keep the modal open so the user can fix the name
      }
    } catch { /* policy unreachable — fall through and let backend decide */ }
    modalBranchNode = null;
    try {
      await createBranch(tab.id, name, oid);
      if (fromHead) {
        await checkoutBranch(tab.id, name);
        uiStore.showToast(`Branch '${name}' created and checked out`, 'success');
      } else {
        uiStore.showToast(`Branch '${name}' created`, 'success');
      }
      graphStore.refresh();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  async function doCreateTag(name: string, push = false) {
    if (!tab || !modalTagNode) return;
    const { oid } = modalTagNode;
    modalTagNode = null;
    try {
      await createTag(tab.id, name, oid);
      if (push) {
        try {
          await pushBranch(tab.id, 'origin', `refs/tags/${name}`);
          uiStore.showToast(`Tag '${name}' creato e pushato`, 'success');
        } catch (err) {
          // Push failed — tag still exists locally, mark it so the badge appears.
          await localTagTracker.markLocal(tab.id, name).catch(() => {});
          uiStore.showToast(`Tag creato ma push fallito: ${err}`, 'warning');
        }
      } else {
        await localTagTracker.markLocal(tab.id, name).catch(() => {});
        uiStore.showToast(`Tag '${name}' creato`, 'success');
      }
      // Belt-and-suspenders: explicitly drop the cached snapshot for this
      // tab (the createTag IPC wrapper already invalidates, but this makes
      // the intent obvious here) AND bump refreshTick so the Sidebar/graph
      // effects re-fetch.
      cacheStore.invalidate(tab.id);
      graphStore.refresh();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  // WIP node — total count of uncommitted changes
  const status = $derived(repoStore.status);

  // ── Pushed indicator ────────────────────────────────────────────
  // Find the row of the remote tracking branch for the current local branch.
  // Commits at row >= syncedRow have been pushed to remote; row < syncedRow = unpushed.
  const syncedRow = $derived((() => {
    if (!data || !status?.current_branch) return -1;
    const cb = status.current_branch;
    const trackNode = data.nodes.find(n =>
      n.refs.some(r => r.ref_type === 'remote_branch' && r.name.endsWith('/' + cb))
    );
    return trackNode?.row ?? -1;
  })());
  function getWipCounts(s: RepoStatus) {
    const paths = new Map<string, 'modified' | 'added' | 'deleted'>();
    for (const f of s.untracked) paths.set(f.path, 'added');
    for (const f of s.staged) {
      if (f.index_status === 'added')        paths.set(f.path, 'added');
      else if (f.index_status === 'deleted') paths.set(f.path, 'deleted');
      else                                   paths.set(f.path, 'modified');
    }
    for (const f of s.unstaged) {
      if (f.workdir_status === 'deleted') paths.set(f.path, 'deleted');
      else if (!paths.has(f.path))        paths.set(f.path, 'modified');
    }
    let modified = 0, added = 0, deleted = 0;
    for (const v of paths.values()) {
      if (v === 'modified') modified++;
      else if (v === 'added') added++;
      else deleted++;
    }
    return { modified, added, deleted, total: paths.size };
  }

  const wipCounts = $derived(status ? getWipCounts(status) : null);
  const wipCount  = $derived(wipCounts?.total ?? 0);
  const showWip   = $derived(wipCount > 0 && !!tab);

  function handleWipContextMenu(e: MouseEvent) {
    e.preventDefault();
    wipContextMenu = { x: e.clientX, y: e.clientY };
  }

  const wipMenuItems: MenuItem[] = [
    { id: 'open-stage',  label: 'Open Stage Area',    icon: PanelBottom, iconColor: 'var(--accent)',     action: 'stage_view' },
    { id: 'sep',         label: '',                    separator: true },
    { id: 'stash',       label: 'Stash Changes',       icon: Archive,     iconColor: 'var(--color-stash)', action: 'stash' },
    { id: 'stash-no-untracked', label: 'Stash (exclude untracked)', icon: Archive, iconColor: 'var(--color-stash)' },
  ];

  async function handleWipMenuSelect(id: string) {
    wipContextMenu = null;
    if (!tab) return;
    if (id === 'open-stage') {
      await handleSelectWip();
    } else if (id === 'stash' || id === 'stash-no-untracked') {
      try {
        const includeUntracked = id === 'stash';
        await stashSave(tab.id, undefined, includeUntracked);
        uiStore.showToast('Changes stashed', 'success');
        // Light refresh — stash op leaves graph topology untouched.
        await applyPostStashChange(tab.id);
      } catch (err) {
        uiStore.showToast(`Stash failed: ${err}`, 'error');
      }
    }
  }

  async function handleSelectWip() {
    if (!tab) return;
    // Workdir view doesn't use lazy commit-file loading.
    diffStore.clearCommitContext();
    try {
      const [staged, unstaged] = await Promise.all([
        getWorkdirDiff(tab.id, true),
        getWorkdirDiff(tab.id, false),
      ]);
      const seen = new Set<string>();
      const all = [...staged, ...unstaged].filter(f => {
        if (seen.has(f.path)) return false;
        seen.add(f.path);
        return true;
      });
      diffStore.setFiles(all);
      graphStore.setWorkdirMode();
      uiStore.setActiveBottomSection('stage');
    } catch (err) {
      uiStore.showToast(`Failed to load changes: ${err}`, 'error');
    }
  }

  // Re-load the currently visible diff when the user toggles "Show full file"
  // (the backend has to emit a different patch). Routes the reload to the
  // matching loader for whichever panel mode is active.
  $effect(() => {
    function onReload() {
      if (!tab) return;
      const previousPath = diffStore.selectedFile?.path ?? null;
      const restore = (files: { path: string }[]) => {
        if (previousPath && files.some(f => f.path === previousPath)) {
          diffStore.selectFile(previousPath);
        }
      };
      if (graphStore.panelMode === 'commit' && graphStore.selectedOid) {
        diffStore.setCommitContext(tab.id, graphStore.selectedOid);
        getCommitDiffMeta(tab.id, graphStore.selectedOid)
          .then(files => { diffStore.setFiles(files); restore(files); })
          .catch(() => {});
      } else if (graphStore.panelMode === 'stash' && graphStore.selectedStash) {
        diffStore.setCommitContext(tab.id, graphStore.selectedStash.oid);
        getCommitDiffMeta(tab.id, graphStore.selectedStash.oid)
          .then(files => { diffStore.setFiles(files); restore(files); })
          .catch(() => {});
      } else if (graphStore.panelMode === 'workdir') {
        Promise.all([getWorkdirDiff(tab.id, true), getWorkdirDiff(tab.id, false)])
          .then(([staged, unstaged]) => {
            const seen = new Set<string>();
            const all = [...staged, ...unstaged].filter(f => {
              if (seen.has(f.path)) return false;
              seen.add(f.path);
              return true;
            });
            diffStore.setFiles(all);
            restore(all);
          })
          .catch(() => {});
      }
    }
    window.addEventListener('arbor:reload-diff', onReload);
    return () => window.removeEventListener('arbor:reload-diff', onReload);
  });
</script>

<div class="graph-container">
  <!-- Toolbar -->
  <div class="graph-toolbar">
    {#if tab}
      <!-- <span class="project-name">{tab.name}</span> -->
      <RepoActions />
    {/if}

    <div class="toolbar-spacer"></div>

    {#if graphStore.fileFilter}
      <div class="file-filter-pill">
        <FileSearch size={11} />
        <span class="file-filter-label" use:tooltip={graphStore.fileFilter}>
          {graphStore.fileFilter.split('/').pop()}
        </span>
        <button class="file-filter-clear" onclick={() => graphStore.clearFileFilter()} use:tooltip={'Clear file filter'}>
          <X size={10} />
        </button>
      </div>
    {/if}

    {#if data}
      <span class="commit-count">{totalRows.toLocaleString()} commits</span>
    {/if}

    {#if tab}
      <button
        class="toolbar-icon-btn"
        use:tooltip={'Open repository in browser'}
        onclick={async () => {
          try { await openInBrowser(tab.id, 'repo'); }
          catch (err) { uiStore.showToast(`${err}`, 'error'); }
        }}
      >
        <Globe size={14} />
      </button>

      <button
        class="toolbar-icon-btn"
        use:tooltip={'Copy arbor:// link to open this repository'}
        onclick={() => copyDeepLink({ kind: 'repo_open' }, tab.id)}
      >
        <Link2 size={14} />
      </button>
    {/if}

    {#if data}
      <button
        class="toolbar-icon-btn"
        use:tooltip={tooltipForAction('Jump to HEAD', 'jump_to_head')}
        onclick={() => graphStore.scrollToHead()}
      >
        <ArrowUpToLine size={14} />
      </button>
    {/if}

    {#if data}
      <button
        class="toolbar-icon-btn"
        use:tooltip={'Export graph as SVG…'}
        onclick={() => (showExportModal = true)}
      >
        <Download size={14} />
      </button>
    {/if}

    {#if tabLink}
      <button
        class="link-badge"
        class:out-of-sync={tabOutOfSync}
        class:syncing={tabLinkSyncing}
        class:disabled={!tabLink.sync_enabled}
        onclick={() => uiStore.openLinkManager(tabLink.id)}
        use:tooltip={
          tabLinkSyncing   ? { content: `Link "${tabLink.name}"`, description: 'Syncing…' }
          : tabOutOfSync   ? { content: `Link "${tabLink.name}"`, description: `Expected '${tabExpectedBranch}', here '${tab?.currentBranch ?? '?'}'` }
          : !tabLink.sync_enabled ? { content: `Link "${tabLink.name}"`, description: 'Sync disabled' }
          : `Link "${tabLink.name}"`
        }
      >
        {#if tabLinkSyncing}
          <Loader size={11} class="spin" />
        {:else}
          <Layers size={11}/>
        {/if}
        <span class="link-name-label">{tabLink.name}</span>
        {#if tabOutOfSync}<span class="dot dot-warn"></span>{/if}
        {#if !tabLink.sync_enabled}<span class="off-pill">off</span>{/if}
      </button>
    {/if}

  </div>

  <!-- Search bar (shown only when Ctrl+F is pressed) -->
  {#if uiStore.searchVisible}
    <div class="search-bar">
      <GraphSearch />
      <button class="mac-close-btn" onclick={() => { uiStore.setSearchVisible(false); graphStore.setSearch(''); graphStore.setHighlighted([]); }} use:tooltip={{ content: 'Close search', shortcut: 'Esc' }} aria-label="Close search"></button>
    </div>
  {/if}

  <!-- WIP node: shown above the graph when there are uncommitted changes -->
  {#if showWip}
    <WipRow
      {svgW}
      {wipCounts}
      {status}
      active={graphStore.panelMode === 'workdir'}
      onclick={handleSelectWip}
      oncontextmenu={handleWipContextMenu}
    />
  {/if}

  <!-- Virtual scroll area — custom keyboard-driven widget (arrow keys walk
       lanes, PgUp/Dn jump viewports, Home/End jump to newest/oldest). The
       `role="application"` is correct but svelte-a11y still flags the div
       as non-interactive; the ignores below are intentional. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="scroll-area" role="application" aria-label="Commit graph" tabindex="0"
       bind:this={scrollEl} onscroll={handleScroll} onkeydown={handleGraphKeydown}
       oncontextmenu={handleBgContextMenu}>
    {#if graphStore.isLoading}
      <div class="center-msg">
        <div class="spinner"></div>
        <span>Loading graph…</span>
      </div>
    {:else if data && totalRows > 0}
      <div class="graph-body" style="height: {svgH}px">

        <!-- Left column: SVG lanes (fixed width) -->
        <svg
          class="graph-svg"
          width={svgW}
          height={svgH}
          style="width: {svgW}px; min-width: {svgW}px"
          overflow="visible"
        >
          <defs>
            <!--
              Row-background gradients — one per lane color (10 total).
              gradientUnits="objectBoundingBox" (default) so each rect stretches the
              gradient across its own width automatically.
              Stops: full lane color → transparent, left-to-right.
            -->
            {#each Array.from({length: 10}, (_, i) => i) as ci (ci)}
              <linearGradient id="row-grad-{ci}" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0%"   stop-color={laneColor(ci)} stop-opacity="0.22"/>
                <stop offset="100%" stop-color={laneColor(ci)} stop-opacity="0"/>
              </linearGradient>
            {/each}

            <!--
              Gaussian blur filter for the lane glow pass.
              Applied to a <g> containing ALL visible edges so glows accumulate and
              blend where lanes run close together — the same technique GitKraken uses.
              Large x/y padding prevents clipping on diagonal crossing edges.
            -->
            <!-- clipPaths for avatar images — one per visible non-merge node -->
            {#each visibleNodes as node (node.oid)}
              {#if !node.is_merge}
                <clipPath id="ac-{node.oid}">
                  <circle cx={nodeX(node.lane)} cy={nodeY(node.row)} r={NODE_RADIUS} />
                </clipPath>
              {/if}
            {/each}
          </defs>

          <!--
            ── PASS 0: row background glow ─────────────────────────────────────
            For each visible commit: a gradient rectangle starting at the node's lane
            position and fading to transparent at the right edge of the SVG, plus a
            thin vertical accent line ("bordatura") that caps the gradient on the right.
            Selected commits get a slightly stronger background.
          -->
          {#each visibleNodes as node (node.oid)}
            {@const rx = nodeX(node.lane)}
            {@const ry = nodeY(node.row) - ROW_HEIGHT / 2}
            {@const rw = svgW - rx}
            {@const isSelected = node.oid === graphStore.selectedOid}
            {#if rw > 0}
              <rect
                x={rx} y={ry}
                width={rw} height={ROW_HEIGHT}
                fill="url(#row-grad-{node.color_index})"
                style="opacity: {isSelected ? 1.0 : 'var(--graph-row-bg-opacity, 0.7)'}"
                pointer-events="none"
              />
              <!-- Thin right-edge accent line (the "bordatura") -->
              <line
                x1={svgW - 1} y1={ry + 3}
                x2={svgW - 1} y2={ry + ROW_HEIGHT - 3}
                stroke={laneColor(node.color_index)}
                stroke-width="1.5"
                stroke-linecap="round"
                pointer-events="none"
              />
            {/if}
          {/each}

          <!--
            ── PASS 1: fake-glow layer ─────────────────────────────────────────
            Wide, semi-transparent strokes WITHOUT an SVG filter. A real Gaussian
            blur filter forced the browser to rebuild a group-wide compositor
            layer every frame (Layerize = 46% of frame time on a 15s profile),
            which dwarfed every other cost. The two-stroke approximation below
            (soft wide + slightly narrower) gives a subtle halo without any
            filter overhead — no Layerize, no offscreen buffer, pure stroke.
          -->
          <g pointer-events="none" style="opacity: var(--graph-glow-intensity, 1)">
            {#each visibleEdges as edge (`glow-${edge.from_row}-${edge.from_lane}-${edge.to_row}-${edge.to_lane}`)}
              {#if edge.edge_type !== 'squash_merge'}
                <path
                  d={edgePath(edge)}
                  fill="none"
                  stroke={laneColor(edge.color_index)}
                  stroke-width="5"
                  stroke-linecap="round"
                  opacity="0.18"
                />
              {/if}
            {/each}
          </g>

          <!-- ── PASS 2: crisp edges ───────────────────────────────────────── -->
          {#each visibleEdges as edge (`${edge.from_row}-${edge.from_lane}-${edge.to_row}-${edge.to_lane}`)}
            <GraphEdge {edge} />
          {/each}
          {#each visibleNodes as node (node.oid)}
            <GraphNode
              {node}
              selected={node.oid === graphStore.selectedOid}
              highlighted={graphStore.highlightedOids.has(node.oid)}
              synced={syncedRow >= 0 && node.row >= syncedRow}
              bisectMark={bisectMarks.get(node.oid) ?? null}
              onclick={() => handleSelectCommit(node)}
              oncontextmenu={(e) => handleContextMenu(e, node)}
            />
          {/each}

          <!-- ── Stash markers ─────────────────────────────────────────────
               Anchored to the commit the stash was created from.  The bubble
               sits in the right-side padding lane so it never collides with
               graph edges; the dashed connector keeps it visually tied to
               the parent commit even when that commit is on a far-left lane.
          -->
          {#each visibleNodes as node (node.oid)}
            {@const stashes = stashesByParentOid.get(node.oid)}
            {#if stashes && stashes.length > 0}
              {@const by  = nodeY(node.row)}
              {@const bx  = svgW - 12}
              {@const nx  = nodeX(node.lane) + NODE_RADIUS + 2}
              {@const tip = stashes.length === 1
                ? `${stashes[0].message} — click per aprire`
                : `${stashes.length} stash su questo commit — click per scegliere`}
              <g class="stash-marker" role="button" tabindex="0"
                 onclick={(e) => { e.stopPropagation(); openStashPopupAt(e.clientX, e.clientY, stashes); }}
                 onkeydown={(e) => {
                   if (e.key !== 'Enter' && e.key !== ' ') return;
                   e.preventDefault();
                   const rect = (e.currentTarget as SVGGraphicsElement).getBoundingClientRect();
                   openStashPopupAt(rect.right + 4, rect.top, stashes);
                 }}
              >
                <title>{tip}</title>
                {#if bx - 7 > nx}
                  <line
                    x1={nx} y1={by}
                    x2={bx - 7} y2={by}
                    stroke="var(--color-stash, #c9a227)"
                    stroke-width="1.25"
                    stroke-dasharray="3 3"
                    opacity="0.72"
                    pointer-events="none"
                  />
                {/if}
                <circle
                  cx={bx} cy={by} r="7"
                  fill="var(--bg-elevated)"
                  stroke="var(--color-stash, #c9a227)"
                  stroke-width="1.6"
                />
                {#if stashes.length > 1}
                  <text
                    x={bx} y={by + 3}
                    text-anchor="middle"
                    font-size="9"
                    font-weight="700"
                    fill="var(--color-stash, #c9a227)"
                    pointer-events="none"
                  >{stashes.length}</text>
                {:else}
                  <!-- Mini "archive" glyph: two stacked strokes for the box,
                       one horizontal for the lid latch. -->
                  <rect
                    x={bx - 3.2} y={by - 2.4}
                    width="6.4" height="4.8"
                    rx="0.8"
                    fill="none"
                    stroke="var(--color-stash, #c9a227)"
                    stroke-width="1"
                    pointer-events="none"
                  />
                  <line
                    x1={bx - 3.2} y1={by - 0.4}
                    x2={bx + 3.2} y2={by - 0.4}
                    stroke="var(--color-stash, #c9a227)"
                    stroke-width="1"
                    pointer-events="none"
                  />

                {/if}
              </g>
            {/if}
          {/each}
        </svg>

        <!-- Right column: commit text rows -->
        <div class="rows-panel">
          {#each visibleNodes as node (node.oid)}
            <div
              class="commit-row"
              class:selected={node.oid === graphStore.selectedOid}
              class:is-head={node.is_head}
              class:highlighted={graphStore.highlightedOids.has(node.oid)}
              class:dimmed={searchActive && !graphStore.highlightedOids.has(node.oid)}
              class:synced={syncedRow >= 0 && node.row >= syncedRow}
              style="top: {nodeY(node.row) - ROW_HEIGHT / 2}px; height: {ROW_HEIGHT}px"
              onclick={() => handleSelectCommit(node)}
              oncontextmenu={(e) => handleContextMenu(e, node)}
              role="row"
              tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && handleSelectCommit(node)}
            >
              {#if node.refs.length > 0 || node.is_head}
                <div class="commit-labels">
                  {#if node.is_head}
                    <span class="head-badge">HEAD</span>
                  {/if}
                  {#each node.refs as ref}
                    <BranchLabel {ref} colorIndex={refColorByName.get(ref.name) ?? node.color_index} />
                  {/each}
                </div>
              {/if}

              <span class="commit-summary-wrap">
                <span class="commit-summary">{node.summary}</span>
                {#if notesStore.hasNotes(node.oid)}
                  <button
                    class="note-badge"
                    use:tooltip={`${notesStore.noteCount(node.oid)} note${notesStore.noteCount(node.oid) !== 1 ? 's' : ''}`}
                    onclick={(e) => { e.stopPropagation(); openNotes(node); }}
                  >
                    <StickyNote size={10} />
                    <span>{notesStore.noteCount(node.oid)}</span>
                  </button>
                {/if}
              </span>

              {#if ticketLinksStore.isEnabled()}
                {@const nodeLinks = ticketLinksStore.getLinks(node.oid)}
                <div class="commit-tickets">
                  {#each nodeLinks as link (link.ticket_id)}
                    <TicketChip
                      {link}
                      onclick={handleChipClick}
                      onRemove={link.source === 'manual' && tab
                        ? (l) => ticketLinksStore.removeLink(tab.id, node.oid, l.ticket_id).catch(err => uiStore.showToast(`${err}`, 'error'))
                        : undefined}
                    />
                  {/each}
                </div>
              {/if}

              <span class="commit-meta">
                {node.author.name}
                <span class="commit-date"> · {formatCommitDate(node.timestamp)}</span>
              </span>

              <span class="commit-oid">{node.oid.slice(0, 7)}</span>
            </div>
          {/each}
        </div>

      </div>
    {:else if data}
      <div class="center-msg">Repository is empty</div>
    {/if}
  </div>

  {#if contextMenu}
    {@const cm = contextMenu}
    <GraphContextMenu
      node={cm.node}
      x={cm.x}
      y={cm.y}
      onClose={() => (contextMenu = null)}
      onShowCreateBranch={openCreateBranch}
      onShowCreateTag={openCreateTag}
      onShowLinkTicket={openLinkTicket}
      onShowNotes={openNotes}
    />
  {/if}

  {#if wipContextMenu}
    <ContextMenu
      x={wipContextMenu.x}
      y={wipContextMenu.y}
      items={wipMenuItems}
      onSelect={handleWipMenuSelect}
      onClose={() => (wipContextMenu = null)}
    />
  {/if}

  {#if bgContextMenu}
    <ContextMenu
      x={bgContextMenu.x}
      y={bgContextMenu.y}
      items={bgMenuItems}
      onSelect={handleBgMenuSelect}
      onClose={() => (bgContextMenu = null)}
    />
  {/if}

  {#if stashPopup}
    <ContextMenu
      x={stashPopup.x}
      y={stashPopup.y}
      items={stashPopupItems}
      onSelect={handleStashPopupSelect}
      onClose={() => (stashPopup = null)}
    />
  {/if}
</div>

<!-- ── Modals live here (outside context menu) so they survive its unmount ── -->
{#if modalBranchNode}
  <CreateBranchModal
    node={modalBranchNode}
    tabId={tab?.id}
    onClose={() => (modalBranchNode = null)}
    onCreate={doCreateBranch}
  />
{/if}

{#if modalTagNode}
  <CreateTagModal
    node={modalTagNode}
    onClose={() => (modalTagNode = null)}
    onCreate={doCreateTag}
  />
{/if}

{#if notesNode}
  <NotesModal
    commitOid={notesNode.oid}
    shortOid={notesNode.short_oid}
    onClose={() => (notesNode = null)}
  />
{/if}

{#if showExportModal && tab}
  <FilePickerModal
    mode="save"
    extensions={['svg']}
    title="Export Graph as SVG"
    initialFilename="graph.svg"
    onConfirm={(path) => { showExportModal = false; doExportSvg(path); }}
    onCancel={() => (showExportModal = false)}
  />
{/if}

{#if ticketPickerNode && tab}
  {@const pickerNode = ticketPickerNode}
  <TicketPickerModal
    onClose={() => (ticketPickerNode = null)}
    onSelect={async (issue) => {
      ticketPickerNode = null;
      if (!tab) return;
      try {
        await ticketLinksStore.addLink(tab.id, pickerNode.oid, issue.identifier, 'linear');
        uiStore.showToast(`Linked ${issue.identifier} to ${pickerNode.short_oid}`, 'success');
      } catch (err) {
        uiStore.showToast(`${err}`, 'error');
      }
    }}
  />
{/if}

{#if pendingScrollOid}
  <ConfirmModal
    title="Commit non caricato"
    message="Il commit {pendingScrollOid.slice(0, 7)} è oltre la pagina corrente del grafo."
    detail="Caricare l'intera history per trovarlo? Su repository grandi questa operazione può richiedere qualche secondo."
    variant="info"
    confirmLabel="Carica tutto"
    cancelLabel="Annulla"
    busy={loadingFullForScroll}
    onConfirm={confirmLoadFullForScroll}
    onCancel={() => { pendingScrollOid = null; }}
  />
{/if}

<style>
  .graph-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-base);
  }

  /* Stash anchor bubble — small interactive circle drawn in the SVG right
     gutter. Hover lifts stroke weight and fills the circle with a faint
     stash-tint so it reads as clickable. */
  .stash-marker { cursor: pointer; outline: none; }
  .stash-marker:hover circle {
    stroke-width: 2;
    fill: color-mix(in srgb, var(--color-stash, #c9a227) 14%, var(--bg-elevated));
  }
  .stash-marker:focus-visible circle {
    stroke-width: 2;
    fill: color-mix(in srgb, var(--color-stash, #c9a227) 14%, var(--bg-elevated));
  }

  .graph-toolbar {
    display: flex;
    align-items: center;
    height: 34px;
    /* padding: 0 10px; */
    background: var(--bg-elevated);
    border-left: 4px solid var(--bg-base);
    border-right: 4px solid var(--bg-base);
    /* border-bottom: 1px solid var(--border); */
    flex-shrink: 0;
    gap: 8px;
  }

  .toolbar-spacer { flex: 1; }

  /* ── File filter pill ── */
  .file-filter-pill {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 6px 2px 7px;
    background: rgba(77,120,204,0.18);
    border: 1px solid rgba(77,120,204,0.4);
    border-radius: 999px;
    color: var(--accent);
    font-size: 10px;
    font-family: var(--font-ui-sans);
    max-width: 180px;
    animation: fadeIn 120ms ease;
  }
  .file-filter-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .file-filter-clear {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--accent);
    padding: 0;
    flex-shrink: 0;
    opacity: 0.7;
    transition: opacity var(--transition-fast);
  }
  .file-filter-clear:hover { opacity: 1; }
  @keyframes fadeIn { from { opacity: 0 } to { opacity: 1 } }

  .commit-count {
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .toolbar-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .toolbar-icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* Linked-worktrees badge */
  .link-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    height: 22px;
    border-radius: 11px;
    background: var(--accent-subtle);
    border: 1px solid var(--accent);
    color: var(--accent);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .link-badge:hover { background: var(--accent); color: var(--text-on-accent); }
  .link-badge.disabled { background: var(--bg-overlay); border-color: var(--border); color: var(--text-muted); }
  .link-badge.out-of-sync { border-color: var(--warning); color: var(--warning); background: var(--warning-subtle); }
  .link-badge.out-of-sync:hover { background: var(--warning); color: var(--text-on-accent); }
  .link-badge.syncing { color: var(--accent); }
  .link-badge :global(.spin) { animation: badge-spin 0.9s linear infinite; }
  @keyframes badge-spin { to { transform: rotate(360deg); } }
  .link-name-label { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 6px; height: 6px; border-radius: 50%; }
  .dot-warn { background: var(--warning); box-shadow: 0 0 0 2px color-mix(in srgb, var(--warning) 22%, transparent); }
  .off-pill {
    font-size: 9px; padding: 0 5px; border-radius: 7px;
    background: var(--error-subtle); color: var(--error);
    text-transform: uppercase;
  }

  /* Search bar */
  .search-bar {
    display: flex;
    align-items: center;
    padding: 4px 10px;
    background: var(--bg-elevated);
    border-left: 4px solid var(--bg-base);
    border-right: 4px solid var(--bg-base);
    gap: 6px;
    flex-shrink: 0;
  }


  .scroll-area {
    flex: 1;
    overflow: auto;
    position: relative;
    padding-left: 8px;
    padding-top: .5em;
  }

  /* Hide the default outline (mouse focus) but keep a visible ring when the
     user reaches the graph viewport via Tab or Alt+G — required so the
     keyboard-only nav (Arrows / Page / Home / End) is discoverable. The inset
     offset keeps the ring inside the panel's rounded corners. */
  .scroll-area:focus { outline: none; }
  .scroll-area:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .graph-body {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    min-width: 100%;
  }

  .graph-svg { flex-shrink: 0; display: block; overflow: visible; }

  .rows-panel { flex: 1; position: relative; min-width: 0; }

  .commit-row {
    position: absolute;
    left: 0; right: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), opacity var(--transition-fast);
    overflow: hidden;
  }

  .commit-row:hover    {
    background: var(--bg-hover);
  }
  /* Explicit selectors instead of `:hover *` so style recalc touches ~4
     elements per hover instead of cascading across every descendant. */
  .commit-row:hover .commit-summary,
  .commit-row:hover .commit-meta,
  .commit-row:hover .commit-date,
  .commit-row:hover .commit-oid {
    color: white;
  }
  .commit-row.selected { background: var(--bg-selected); }
  .commit-row.highlighted { background: rgba(77, 120, 204, 0.10); }

  /* Dim non-matching commits during search */
  .commit-row.dimmed {
    opacity: 0.28;
  }
  .commit-row.dimmed .commit-summary {
    color: var(--text-muted);
  }

  .commit-row.is-head {
    background: rgba(77, 120, 204, 0.07);
  }
  .commit-row.is-head::before {
    content: '';
    position: absolute;
    left: 0;
    top: 3px;
    bottom: 3px;
    width: 2px;
    background: var(--accent);
    border-radius: 0 2px 2px 0;
  }
  .commit-row.is-head .commit-summary { font-weight: 600; }

  .head-badge {
    font-family: var(--font-code);
    font-size: 9px;
    font-weight: 700;
    color: var(--accent);
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
    letter-spacing: 0.6px;
    flex-shrink: 0;
    white-space: nowrap;
  }

  /* Pushed commits — slightly dimmed to distinguish from unpushed (ahead) */
  .commit-row.synced { opacity: 0.85; }
  .commit-row.synced .commit-summary { color: var(--text-secondary); }

  .commit-labels {
    display: flex;
    gap: 3px;
    flex-shrink: 1;
    min-width: 0;
    max-width: 50%;
    overflow: hidden;
  }

  .commit-tickets {
    display: flex;
    gap: 3px;
    flex: 0 0 130px;
    justify-content: flex-end;
    overflow: hidden;
  }

  .commit-summary-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }

  .commit-summary {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    min-width: 0;
    transition: color var(--transition-fast);
  }

  .note-badge {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px 1px 4px;
    background: var(--accent-subtle);
    border: 1px solid var(--accent);
    border-radius: 999px;
    color: var(--accent);
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0.85;
    transition: opacity var(--transition-fast), background var(--transition-fast);
  }
  .note-badge:hover { opacity: 1; background: rgba(77,120,204,0.25); }

  .commit-meta {
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    white-space: nowrap;
    flex: 0 0 200px;
    display: flex;
    align-items: center;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-left: 8px;
    padding-left: 10px;
    border-left: 1px solid var(--border-subtle);
  }

  .commit-date { color: var(--text-disabled); }

.commit-oid {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-disabled);
    white-space: nowrap;
    flex-shrink: 0;
    letter-spacing: 0.3px;
    min-width: 54px;
    text-align: right;
    padding-left: 6px;
  }

  .center-msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 200px;
    gap: 10px;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }

  .spinner {
    width: 20px; height: 20px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 700ms linear infinite;
  }

</style>
