<script lang="ts">
  import {
    GitMerge, GitPullRequest, GitPullRequestClosed, ExternalLink,
    CheckCircle, XCircle, Clock, AlertCircle, ChevronDown,
    Loader, ArrowRight, MessageSquare, FileText, GitCommit, FileEdit,
    TriangleAlert, Workflow, RefreshCw, RotateCcw, GitBranch,
    Circle, Ban, Bot, Activity, Tag, UserPlus, Eye, Edit3, Settings,
    Link2, Wand2,
  } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import BrandIcon from '$lib/components/shared/internal/BrandIcon.svelte';
  import { mrStore } from '$lib/stores/mr.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import {
    mergeMr, closeMr, reopenMr, markMrReady, addMrComment, disableMrAutoMerge,
    getMrFiles, getMrCommits, getCommitDiff, mrStartConflictResolution,
    type MrConflictProgress, type MrConflictDone,
  } from '$lib/ipc/mr';
  import { invalidateTabCache } from '$lib/ipc/cache-invalidate';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import ProgressStepper, { type Step as StepDef } from '$lib/components/shared/ui/ProgressStepper.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { fetchRemote } from '$lib/ipc/remote';
  import { deleteBranch, checkoutBranchSafe, listLocalBranches } from '$lib/ipc/branch';
  import { handleCheckoutResult } from '$lib/utils/checkoutResultHandler';
  import { listWorktrees } from '$lib/ipc/worktree';
  import { getStatus } from '$lib/ipc/stage';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { pipelinesStore } from '$lib/stores/pipelines.svelte';
  import { retrigerCiRun, fetchMrCiRuns } from '$lib/ipc/pipeline';
  import CiPipelineDetailModal from '$lib/components/pipeline/CiPipelineDetailModal.svelte';
  import type { MergeRequest, MrState, MrFileDiff, MrCommit, MrComment, MrEvent } from '$lib/types/mr';
  import type { CiRun } from '$lib/types/pipeline';
  import { highlight } from '$lib/utils/diff-formatter';
  import { renderMarkdown } from '$lib/utils/markdown';
  import { installListAwareCopy } from '$lib/utils/html-to-text';
  import { tooltip } from '$lib/actions/tooltip';
  import { replaceEmojiShortcodes } from '$lib/utils/emoji';
  import { getMrConfig } from '$lib/ipc/config';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  let { mr, onClose, onRefresh, onRestoreFromScratch }: {
    mr:        MergeRequest;
    onClose:   () => void;
    onRefresh: () => void;
    onRestoreFromScratch?: () => void | Promise<void>;
  } = $props();

  const tabId = $derived(tabsStore.activeTabId ?? '');

  $effect(() => { if (tabId && mr.number) mrStore.loadDetail(tabId, mr.number); });

  let bodyEl = $state<HTMLElement | null>(null);

  // Reformat Ctrl+C output when the selection contains a list so bullets
  // (CSS pseudo-elements) survive in the clipboard.
  $effect(() => {
    if (!bodyEl) return;
    return installListAwareCopy(bodyEl);
  });

  // ── Merge state ────────────────────────────────────────────────────────
  let acting      = $state(false);
  let mergeMenu   = $state(false);
  let commentTxt  = $state('');
  let commenting  = $state(false);
  // svelte-ignore state_referenced_locally
  let mergeSquash = $state(mr.squash);
  // svelte-ignore state_referenced_locally
  let mergeDelete = $state(mr.deleteBranch);
  let confirmCloseOpen = $state(false);

  // ── Activity filters ───────────────────────────────────────────────────
  // Initial values come from the persistent MR config (Settings → MR/PR)
  // and can be overridden per-modal-session via the chips. The chip toggles
  // are intentionally NOT written back: an open MR's filter state should
  // reflect *what the user wants right now*, not redefine the global
  // defaults. To change defaults the user goes to Settings.
  let showComments = $state(true);
  let showBots     = $state(true);
  let showActivity = $state(true);

  // Load persisted defaults once on mount. Failures are non-fatal — we just
  // keep the in-code defaults above.
  $effect(() => {
    let cancelled = false;
    getMrConfig().then(cfg => {
      if (cancelled) return;
      showComments = cfg.default_show_comments;
      showBots     = cfg.default_show_bots;
      showActivity = cfg.default_show_activity;
    }).catch(() => { /* keep in-code defaults */ });
    return () => { cancelled = true; };
  });

  // ── Tabs ───────────────────────────────────────────────────────────────
  type Tab = 'overview' | 'files' | 'commits' | 'ci';
  let activeTab = $state<Tab>('overview');

  // ── Files tab ──────────────────────────────────────────────────────────
  let files         = $state<MrFileDiff[]>([]);
  let filesLoading  = $state(false);
  let filesError    = $state<string | null>(null);
  let selectedFile  = $state<MrFileDiff | null>(null);

  // ── Commits tab ────────────────────────────────────────────────────────
  let commits            = $state<MrCommit[]>([]);
  let commitsLoading     = $state(false);
  let commitsError       = $state<string | null>(null);
  let selectedCommit     = $state<MrCommit | null>(null);
  let commitFiles        = $state<MrFileDiff[]>([]);
  let commitFilesLoading = $state(false);
  let selectedCommitFile = $state<MrFileDiff | null>(null);

  const detail        = $derived(mrStore.detail);
  const detailLoading = $derived(mrStore.detailLoading);
  const detailMr      = $derived(detail?.mr ?? mr);

  const totalAdd = $derived(files.reduce((a, f) => a + f.additions, 0));
  const totalDel = $derived(files.reduce((a, f) => a + f.deletions, 0));

  // ── CI tab ─────────────────────────────────────────────────────────────
  // pipelinesStore is reused only for provider detection (cached per-tab).
  // Runs themselves come from `fetch_mr_ci_runs`, which queries the
  // MR-specific endpoint required to surface GitLab detached merge-request
  // pipelines (whose `ref` is `refs/merge-requests/{iid}/head` and would be
  // missed by a plain source-branch filter).
  let retriggeringId = $state<string | null>(null);
  let selectedCiRun  = $state<CiRun | null>(null);
  let branchRuns     = $state<CiRun[]>([]);
  let mrCiLoading    = $state(false);
  let mrCiError      = $state<string | null>(null);
  const ciProvider   = $derived(pipelinesStore.ciProvider);
  /** Short HEAD sha (8 chars) for marking the run that built the current PR head. */
  const headShort  = $derived((detailMr.headSha ?? '').slice(0, 8));
  const ciRunCount = $derived(branchRuns.length);

  async function loadMrCi() {
    const mrTabId = tabId;
    const src = detailMr.sourceBranch;
    if (!mrTabId || !src) return;
    if (!ciProvider?.has_token) return;
    mrCiLoading = true;
    mrCiError   = null;
    try {
      branchRuns = await fetchMrCiRuns(mrTabId, mr.number, src, detailMr.headSha || undefined);
    } catch (e: any) {
      mrCiError = String(e?.message ?? e);
      branchRuns = [];
    } finally {
      mrCiLoading = false;
    }
  }

  // Lazy-load provider info + MR-scoped runs when the CI tab opens.
  // Provider info goes through `pipelinesStore.loadCi` (cached per-tab).
  $effect(() => {
    if (activeTab !== 'ci' || !tabId) return;
    pipelinesStore.loadCi(tabId).then(() => {
      // Provider may have just resolved — kick off the MR-scoped fetch.
      loadMrCi();
    });
  });

  // Re-fetch when the MR's source branch changes (e.g. after detail loads).
  $effect(() => {
    if (activeTab === 'ci') {
      detailMr.sourceBranch; // track
      loadMrCi();
    }
  });

  // `mergeable` is null in the summary payload — only the detail response resolves it.
  // Treat as "checking" until detail arrives so the Merge/Resolve button doesn't flip.
  const mergeableChecking = $derived(
    detail === null && detailLoading && (detailMr.mergeable === null || detailMr.mergeable === undefined)
  );
  const hasConflicts     = $derived(detailMr.mergeable === false);
  const mergeBlocked     = $derived(acting || detailMr.isDraft || hasConflicts || mergeableChecking);
  const mergeBlockReason = $derived(
    detailMr.isDraft     ? 'Cannot merge a draft PR' :
    mergeableChecking    ? 'Checking merge status…' :
    hasConflicts         ? 'Merge conflicts must be resolved first' : null
  );

  async function switchTab(t: Tab) {
    activeTab = t;
    if (t === 'ci' && tabId) {
      // Fire-and-forget; the $effect on activeTab also handles this.
      pipelinesStore.loadCi(tabId);
    }
    if (t === 'files' && files.length === 0 && !filesLoading) {
      filesLoading = true; filesError = null;
      try   { files = await getMrFiles(tabId, mr.number); }
      catch (e: any) { filesError = String(e); }
      finally { filesLoading = false; }
    }
    if (t === 'commits' && commits.length === 0 && !commitsLoading) {
      commitsLoading = true; commitsError = null;
      try   { commits = await getMrCommits(tabId, mr.number); }
      catch (e: any) { commitsError = String(e); }
      finally { commitsLoading = false; }
    }
  }

  async function selectCommit(c: MrCommit) {
    if (selectedCommit?.sha === c.sha) return;
    selectedCommit = c;
    selectedCommitFile = null;
    commitFiles = [];
    commitFilesLoading = true;
    try   { commitFiles = await getCommitDiff(tabId, c.sha); }
    catch  {}
    finally { commitFilesLoading = false; }
  }

  // ── Helpers ────────────────────────────────────────────────────────────
  function stateLabel(s: MrState) {
    return s === 'open' ? 'Open' : s === 'merged' ? 'Merged' : 'Closed';
  }
  function timeAgo(iso: string): string {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return '';
    const s = Math.floor((Date.now() - d.getTime()) / 1000);
    if (s < 60)      return `${s}s ago`;
    if (s < 3600)    return `${Math.floor(s/60)}m ago`;
    if (s < 86400)   return `${Math.floor(s/3600)}h ago`;
    if (s < 2592000) return `${Math.floor(s/86400)}d ago`;
    return d.toLocaleDateString();
  }
  function checksColor(s: string) {
    if (s === 'success')                    return 'var(--success)';
    if (s === 'failed')                     return 'var(--error)';
    if (s === 'pending' || s === 'running') return 'var(--warning)';
    return 'var(--text-muted)';
  }
  function initials(n: string) { return (n || '?')[0].toUpperCase(); }

  // ── Activity timeline ─────────────────────────────────────────────────
  // Two item shapes coexist on the timeline; using a discriminated union
  // keeps the {#each} block trivially type-safe.
  type TimelineItem =
    | { kind: 'comment'; key: string; createdAt: string; data: MrComment }
    | { kind: 'event';   key: string; createdAt: string; data: MrEvent };

  /** Comments split by bot heuristic. Memoised so the chip counts and the
   *  rendered timeline can't drift apart. */
  const allComments = $derived<MrComment[]>(detail?.comments ?? []);
  const allEvents   = $derived<MrEvent[]>(detail?.events ?? []);

  const humanComments = $derived(allComments.filter(c => !c.isBot));
  const botComments   = $derived(allComments.filter(c =>  c.isBot));

  /** Chronologically merged & filtered timeline. We intentionally keep
   *  ordering stable by ISO timestamp string compare — both providers
   *  return RFC-3339 strings, so lexical compare is a correct chronological
   *  compare without paying for `Date` parsing on every item. */
  const timeline = $derived<TimelineItem[]>((() => {
    const items: TimelineItem[] = [];
    if (showComments) {
      for (const c of humanComments) {
        items.push({ kind: 'comment', key: `c:${c.id}`, createdAt: c.createdAt, data: c });
      }
    }
    if (showBots) {
      for (const c of botComments) {
        items.push({ kind: 'comment', key: `c:${c.id}`, createdAt: c.createdAt, data: c });
      }
    }
    if (showActivity) {
      for (const e of allEvents) {
        items.push({ kind: 'event', key: `e:${e.id}`, createdAt: e.createdAt, data: e });
      }
    }
    items.sort((a, b) => a.createdAt.localeCompare(b.createdAt));
    return items;
  })());

  /** Map of event kind → Lucide icon component. Centralised so the
   *  template stays simple and adding a kind is a one-liner here. */
  const eventIconMap = {
    state:  Activity,
    label:  Tag,
    assign: UserPlus,
    review: Eye,
    commit: GitCommit,
    rename: Edit3,
    system: Settings,
  } as const;
  function eventIcon(kind: string) {
    return eventIconMap[kind as keyof typeof eventIconMap] ?? Settings;
  }

  /** Render the `summary` field as inline-only markdown — bold (**…**) +
   *  inline code (`…`) — without paragraphs/blocks. Suitable for one-line
   *  event lines.
   *
   *  GitLab system notes may carry an HTML expansion appended to the prose
   *  (e.g. `"added 83 commits\n\n<ul><li>…</li></ul>"`). For the compact
   *  timeline we want the human-readable lede only — everything from the
   *  first HTML tag onward is dropped. As a fallback, when the body opens
   *  *with* a tag, all tags are stripped and the inner text reused.
   */
  function renderEventSummary(s: string): string {
    let cleaned = (s ?? '').split('\n')[0].trim();
    const firstTag = cleaned.search(/<\w/);
    if (firstTag > 0) {
      cleaned = cleaned.slice(0, firstTag).trim();
    } else if (firstTag === 0) {
      cleaned = cleaned.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    }
    cleaned = replaceEmojiShortcodes(cleaned);
    return cleaned
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
      .replace(/`([^`]+)`/g, '<code class="ev-code">$1</code>');
  }

  /** Returns a tone class for the rail node — drives bg/icon color.
   *  `kind === 'state'` is split further by inspecting the summary so
   *  closed/merged/reopened/draft each get their own hue (matches GitLab's
   *  red close-icon convention). */
  function eventTone(ev: MrEvent): string {
    if (ev.kind === 'state') {
      const s = ev.summary.toLowerCase();
      if (s.includes('closed'))                       return 'tone-danger';
      if (s.includes('merged'))                       return 'tone-merged';
      if (s.includes('reopened') || s.includes('ready')) return 'tone-success';
      if (s.includes('draft'))                        return 'tone-muted';
      return 'tone-state';
    }
    return `tone-${ev.kind}`;
  }

  /** "Important" events get a sized icon node on the rail; other events
   *  collapse to a tiny dot. Keeps the timeline scannable when there's a
   *  long tail of minor label/assignment/review changes. */
  function isMajorEvent(kind: string): boolean {
    return kind === 'state' || kind === 'commit' || kind === 'rename';
  }

  // File status
  function fsBadge(s: string)  { return s === 'added' ? 'A' : s === 'removed' ? 'D' : s === 'renamed' ? 'R' : 'M'; }
  function fsCls(s: string)    { return `fs-${s === 'added' ? 'a' : s === 'removed' ? 'd' : s === 'renamed' ? 'r' : 'm'}`; }

  // Diff line styling
  function dlCls(line: string) {
    if (line.startsWith('@@'))  return 'dl-hunk';
    if (line.startsWith('+++') || line.startsWith('---')) return 'dl-fhdr';
    if (line.startsWith('+'))   return 'dl-add';
    if (line.startsWith('-'))   return 'dl-del';
    return 'dl-ctx';
  }

  // Highlighted diff line content — uses Prism for code lines, plain for headers
  function hlLine(line: string, filename: string): string {
    if (line.startsWith('@@') || line.startsWith('+++') || line.startsWith('---')) {
      return line.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
    }
    const content = line.length > 0 ? line.slice(1) : '';
    return highlight(content, filename);
  }

  function checkIcon(status: string) {
    if (status === 'success') return CheckCircle;
    if (status === 'failed' || status === 'cancelled') return XCircle;
    return Clock;
  }
  function checkColor(status: string): string {
    if (status === 'success')   return 'var(--success)';
    if (status === 'failed')    return 'var(--error)';
    if (status === 'cancelled') return 'var(--text-muted)';
    return 'var(--warning)';
  }

  // ── CI helpers ──────────────────────────────────────────────────────────
  function ciStatusIcon(s: string) {
    switch (s) {
      case 'success':   return CheckCircle;
      case 'failed':    return XCircle;
      case 'running':   return RefreshCw;
      case 'cancelled': return Ban;
      default:          return Circle;
    }
  }
  function ciStatusLabel(s: string): string {
    switch (s) {
      case 'success':   return 'Passed';
      case 'failed':    return 'Failed';
      case 'running':   return 'Running';
      case 'cancelled': return 'Cancelled';
      default:          return 'Pending';
    }
  }
  function formatDuration(secs: number | null): string {
    if (!secs) return '';
    if (secs < 60) return `${Math.round(secs)}s`;
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}m ${s.toString().padStart(2, '0')}s`;
  }
  function providerLabel(p: string): string {
    return p === 'github' ? 'GitHub Actions' : p === 'gitlab' ? 'GitLab CI' : p;
  }
  async function refreshCi() {
    // Refresh both: provider cache (in case token state changed) and MR runs.
    const mrTabId = tabId;
    if (mrTabId) await pipelinesStore.refreshCiRuns(mrTabId);
    await loadMrCi();
  }
  async function handleRetriggerForRun(run: CiRun) {
    const mrTabId = tabId;
    if (!mrTabId) return;
    retriggeringId = run.id;
    try {
      await retrigerCiRun(mrTabId, run.id);
      await loadMrCi();
      if (selectedCiRun?.id === run.id) {
        selectedCiRun = branchRuns.find(r => r.id === run.id) ?? selectedCiRun;
      }
    } catch (err) {
      uiStore.showToast(`Failed to re-trigger: ${err}`, 'error');
    } finally {
      retriggeringId = null;
    }
  }

  // ── Actions ────────────────────────────────────────────────────────────
  async function doMerge(method: 'merge' | 'squash' | 'rebase') {
    mergeMenu = false; acting = true;
    // Snapshot the tab so tab-switches during the async flow don't redirect
    // cleanup (fetch / checkout / delete) to the wrong repository.  `tabId`
    // is $derived from `tabsStore.activeTabId` and would otherwise change
    // out from under us.
    const mrTabId     = tabId;
    const sourceBranch = detailMr.sourceBranch;
    const targetBranch = detailMr.targetBranch;
    try {
      await mergeMr(mrTabId, mr.number, { mergeMethod: method, squash: mergeSquash || method === 'squash', deleteBranch: mergeDelete, sourceBranch });
      uiStore.showToast(`#${mr.number} merged successfully`, 'success');
      onClose(); onRefresh();

      // Fetch + refresh the graph so the merge commit shows up locally.
      try { await fetchRemote(mrTabId, 'origin'); } catch { /* ignore fetch errors */ }
      graphStore.refresh();

      // Local cleanup only when the user asked for the source branch to go.
      if (mergeDelete && sourceBranch) {
        await cleanupLocalSourceBranch(mrTabId, sourceBranch, targetBranch);
      }
    } catch (e: any) { uiStore.showToast(String(e), 'error'); }
    finally { acting = false; }
  }

  /** Delete the local copy of the source branch after a remote merge.
   *
   *  `mrTabId` is snapshotted by the caller so the flow keeps operating on
   *  the repo that owns this MR even if the user switches tab mid-flight.
   *
   *  Safety gates:
   *   1. Skip silently if the branch doesn't exist locally.
   *   2. Skip + notify when the branch is checked out in any worktree.
   *   3. Always read the current branch fresh (via `getStatus`) — the cached
   *      `tabsStore.activeTab?.currentBranch` is set at tab-open time and
   *      never refreshed, so it can't be trusted here.  If HEAD is on the
   *      source branch, checkout the target first; if that fails keep the
   *      branch and surface a warning.
   *   4. Finally, delete.  Failures become warnings; nothing is left in a
   *      broken state. */
  async function cleanupLocalSourceBranch(mrTabId: string, source: string, target: string) {
    // 1. Fresh state: branches + actual HEAD.
    let locals: { name: string }[];
    let currentBranch: string | undefined;
    try {
      const [branches, status] = await Promise.all([
        listLocalBranches(mrTabId),
        getStatus(mrTabId),
      ]);
      locals = branches;
      currentBranch = status.current_branch ?? undefined;
      // Keep the in-memory tab cache in sync only when the user is still
      // looking at this repo — otherwise we'd stomp on the active tab's
      // status with another repo's data.
      if (mrTabId === tabsStore.activeTabId) {
        repoStore.setStatus(status);
      }
    } catch {
      // Can't read the repo — give up rather than guess.
      return;
    }
    if (!locals.some(b => b.name === source)) return;

    // 2. Is it checked out in any worktree?
    try {
      const wts = await listWorktrees(mrTabId);
      const busy = wts.find(w => w.branch === source && !w.is_current);
      if (busy) {
        notificationsStore.add(
          `Local branch "${source}" kept`,
          `Checked out in worktree at ${busy.path} — remove the worktree first to delete the branch locally.`,
          'warning',
        );
        return;
      }
    } catch {
      // Best-effort: if we can't list worktrees, don't block the cleanup.
    }

    // 3. If HEAD is on the source branch, switch to the target first.
    if (currentBranch === source) {
      try {
        const result = await checkoutBranchSafe(mrTabId, target);
        // If the safe checkout couldn't settle cleanly (stash conflicts,
        // apply error, or post-merge checkout failure), surface that via the
        // shared handler and bail before the branch-delete step — deleting
        // the source while we're still on it would error out anyway.
        const clean = handleCheckoutResult(result, {
          targetLabel:    target,
          successMessage: `Switched to ${target}`,
        });
        if (!clean) return;
      } catch (e: any) {
        const msg = firstLine(e);
        notificationsStore.add(
          `Local branch "${source}" kept`,
          `Could not switch to "${target}" (${msg}). Switch manually, then delete the branch.`,
          'warning',
        );
        return;
      }
    }

    // 4. Delete.
    try {
      await deleteBranch(mrTabId, source);
    } catch (e: any) {
      notificationsStore.add(
        `Could not delete local branch "${source}"`,
        firstLine(e),
        'warning',
      );
    }
  }

  function firstLine(e: any): string {
    return String(e?.message ?? e).split('\n')[0];
  }

  // ── Manual refresh ─────────────────────────────────────────────────────
  // Spinning state isolates the refresh icon from `acting` (which gates
  // merge/close/comment buttons too — a refresh must never disable those).
  let refreshing = $state(false);

  /** Re-fetch every piece of detail that's already been loaded for this MR.
   *  Tab-content (files, commits) is reloaded only when its tab has been
   *  visited at least once — refreshing eagerly would mask the lazy-load
   *  pattern those tabs rely on. */
  async function doRefresh() {
    if (refreshing) return;
    const mrTabId = tabId;
    if (!mrTabId) return;
    refreshing = true;
    try {
      // Parent list refresh runs in parallel — the user may have just
      // changed something on the remote that affects the listing too.
      const tasks: Promise<unknown>[] = [
        mrStore.loadDetail(mrTabId, mr.number).catch(() => {}),
      ];
      try { onRefresh(); } catch { /* swallow */ }

      if (files.length > 0) {
        tasks.push(
          getMrFiles(mrTabId, mr.number)
            .then(f => { files = f; })
            .catch(() => {}),
        );
      }
      if (commits.length > 0) {
        tasks.push(
          getMrCommits(mrTabId, mr.number)
            .then(c => { commits = c; })
            .catch(() => {}),
        );
      }
      if (activeTab === 'ci') tasks.push(loadMrCi());

      await Promise.all(tasks);
    } finally {
      refreshing = false;
    }
  }

  function requestClose() {
    confirmCloseOpen = true;
  }
  async function confirmCloseMr() {
    confirmCloseOpen = false;
    const mrTabId = tabId;
    acting = true;
    try { await closeMr(mrTabId, mr.number); uiStore.showToast(`#${mr.number} closed`, 'info'); onClose(); onRefresh(); }
    catch (e: any) { uiStore.showToast(String(e), 'error'); }
    finally { acting = false; }
  }
  async function doReopen() {
    const mrTabId = tabId;
    acting = true;
    try { await reopenMr(mrTabId, mr.number); uiStore.showToast(`#${mr.number} reopened`, 'success'); onClose(); onRefresh(); }
    catch (e: any) { uiStore.showToast(String(e), 'error'); }
    finally { acting = false; }
  }
  async function doMarkReady() {
    const mrTabId = tabId;
    acting = true;
    try { await markMrReady(mrTabId, mr.number); uiStore.showToast(`#${mr.number} marked as ready for review`, 'success'); onRefresh(); await mrStore.loadDetail(mrTabId, mr.number); }
    catch (e: any) { uiStore.showToast(String(e), 'error'); }
    finally { acting = false; }
  }
  async function doDisableAutoMerge() {
    const mrTabId = tabId;
    acting = true;
    try {
      await disableMrAutoMerge(mrTabId, mr.number);
      uiStore.showToast(`Auto-merge disabled on #${mr.number}`, 'success');
      onRefresh();
      await mrStore.loadDetail(mrTabId, mr.number);
    } catch (e: any) {
      uiStore.showToast(String(e), 'error');
    } finally {
      acting = false;
    }
  }
  async function doComment() {
    if (!commentTxt.trim()) return;
    const mrTabId = tabId;
    commenting = true;
    try { await addMrComment(mrTabId, mr.number, commentTxt.trim()); commentTxt = ''; await mrStore.loadDetail(mrTabId, mr.number); uiStore.showToast('Comment added', 'success'); }
    catch (e: any) { uiStore.showToast(String(e), 'error'); }
    finally { commenting = false; }
  }

  // ── Conflict-resolution prep flow ──────────────────────────────────────
  // Tracks the live phase of the background prep job so the footer can
  // render a ProgressStepper instead of a static "Preparing…" label.
  let resolveJobId   = $state<string | null>(null);
  let resolveProgress = $state<MrConflictProgress | null>(null);
  let resolveError    = $state<string | null>(null);

  // Ordered to match the backend MrPrepPhase enum.  Detail text comes from
  // the live event payload (activeDetail), so the labels here stay constant.
  const RESOLVE_STEPS: StepDef[] = [
    { key: 'status',   label: 'Checking workdir' },
    { key: 'fetch',    label: 'Fetching from origin' },
    { key: 'checkout', label: 'Switching to source branch' },
    { key: 'merge',    label: 'Merging target' },
  ];

  async function doResolveConflicts() {
    // Snapshot the tab: the resolve flow writes to the working copy, so if
    // the user changes tab mid-flight we must keep operating on the repo
    // that owns this MR.
    const mrTabId = tabId;
    const source = detailMr.sourceBranch;
    const target = detailMr.targetBranch;
    if (!source || !target) {
      uiStore.showToast('Missing source/target branch on this MR', 'error');
      return;
    }

    acting          = true;
    resolveProgress = null;
    resolveError    = null;

    // Subscribe to phase + completion events BEFORE kicking off the job —
    // events from a fast prep flow can arrive before the IPC promise resolves.
    // We don't filter on job_id: the modal allows at most one prep flow at a
    // time (gated by `acting`), so any incoming event belongs to it.  Filtering
    // would introduce a race with the not-yet-known job_id.
    // Initialise as a no-op so TS can't narrow this through the Promise
    // constructor callback to `never` — `setupTauriListeners` runs
    // synchronously inside the executor and overwrites it immediately.
    let stop: () => void = () => {};
    const finished = new Promise<MrConflictDone>((resolve) => {
      stop = setupTauriListeners([
        {
          event: 'arbor://mr-conflict-progress',
          handler: (e: { payload: MrConflictProgress }) => { resolveProgress = e.payload; },
        },
        {
          event: 'arbor://mr-conflict-done',
          handler: (e: { payload: MrConflictDone }) => { resolve(e.payload); },
        },
      ]);
    });

    try {
      resolveJobId = await mrStartConflictResolution(mrTabId, source, target);
      const result = await finished;

      if (result.status === 'clean') {
        invalidateTabCache(mrTabId);
        const s = await getStatus(mrTabId);
        if (mrTabId === tabsStore.activeTabId) repoStore.setStatus(s);
        graphStore.refresh();
        uiStore.showToast(
          `Merged ${target} into ${source} cleanly — push to update the MR.`,
          'success',
        );
        onRefresh();
      } else if (result.status === 'conflicts') {
        invalidateTabCache(mrTabId);
        try {
          const s = await getStatus(mrTabId);
          if (mrTabId === tabsStore.activeTabId) repoStore.setStatus(s);
        } catch {}
        graphStore.refresh();
        onClose();
        // Only prompt the merge modal if the user is still on this repo —
        // otherwise the modal would pop on whatever tab they switched to.
        if (mrTabId === tabsStore.activeTabId) {
          uiStore.openMergeModal();
        } else {
          notificationsStore.add(
            `Conflicts on "${source}" ready to resolve`,
            `Open the repository for this PR to finish the merge.`,
            'info',
          );
        }
      } else {
        // status === 'error'
        resolveError = result.error ?? 'Conflict-resolution prep failed';
        uiStore.showToast(resolveError, 'error');
      }
    } catch (e: any) {
      const msg = String(e?.message ?? e);
      resolveError = msg;
      uiStore.showToast(msg, 'error');
    } finally {
      stop();
      acting          = false;
      resolveJobId    = null;
      resolveProgress = null;
      // Keep `resolveError` set until the next attempt so the user can see
      // what went wrong.
    }
  }

  /** ESC closes the modal — but yields to nested overlays (merge menu,
   *  confirm-close dialog, CI run detail) so a single Escape press doesn't
   *  pop both the inner overlay and the modal at once. We always
   *  stopImmediatePropagation so the host Modal's own Escape handler
   *  doesn't double-fire onClose after we already consumed the event. */
  function handleKeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    if (mergeMenu)        { mergeMenu = false;        e.stopImmediatePropagation(); return; }
    if (confirmCloseOpen) { confirmCloseOpen = false; e.stopImmediatePropagation(); return; }
    if (selectedCiRun)    { selectedCiRun = null;     e.stopImmediatePropagation(); return; }
    e.stopImmediatePropagation();
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if mergeMenu}
  <button type="button" aria-label="Close menu" class="merge-backdrop" onclick={() => mergeMenu = false}></button>
{/if}

<Modal
  {onClose}
  width="min(940px, 95vw)" height="min(700px, 92vh)"
  padBody={false}
  ariaLabel="Merge request"
  minimizable
  parkId={`mr-${mr.number}`}
  parkTitle={`#${mr.number} · ${detailMr.title}`}
  parkIcon={GitPullRequest}
  {onRestoreFromScratch}
>
  {#snippet header()}
    <ModalHeader {onClose}>
      <span class="state-badge state-{detailMr.state}">
        {#if detailMr.state === 'merged'}<GitMerge size={11} />
        {:else if detailMr.state === 'closed'}<GitPullRequestClosed size={11} />
        {:else}<GitPullRequest size={11} />{/if}
        {stateLabel(detailMr.state)}
      </span>
      {#if detailMr.isDraft}<span class="draft-badge">Draft</span>{/if}
      <button
        type="button"
        class="modal-title modal-title-btn"
        use:tooltip={'Click to copy title'}
        onclick={() => copyToClipboard(detailMr.title, { successToast: 'Title copied', errorToast: true })}
      >{detailMr.title}</button>
      <button
        type="button"
        class="modal-num modal-num-btn"
        use:tooltip={'Click to copy'}
        onclick={() => copyToClipboard(`#${mr.number}`, { successToast: `Copied #${mr.number}`, errorToast: true })}
      >#{mr.number}</button>

      {#snippet actions()}
        <button
          class="icon-btn"
          onclick={doRefresh}
          disabled={refreshing}
          use:tooltip={'Refresh detail'}
          aria-label="Refresh detail"
        >
          {#if refreshing}
            <Loader size={13} class="spin" />
          {:else}
            <RefreshCw size={13} />
          {/if}
        </button>
        <button
          class="icon-btn"
          onclick={() => tabId && copyDeepLink({ kind: 'mr_open', number: mr.number }, tabId)}
          use:tooltip={'Copy arbor:// link to this MR'}
          aria-label="Copy arbor:// link to this MR"
        >
          <Link2 size={13} />
        </button>
        <button class="icon-btn" type="button" onclick={() => openUrl(mr.webUrl).catch(() => {})} use:tooltip={'Open in browser'} aria-label="Open in browser">
          <ExternalLink size={13} />
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="body-stack" bind:this={bodyEl}>
  <!-- ── Meta ────────────────────────────────────────────────────────────── -->
  <div class="modal-meta">
    <span class="bp src">{detailMr.sourceBranch || '…'}</span>
    <ArrowRight size={11} class="meta-arr" />
    <span class="bp tgt">{detailMr.targetBranch || '…'}</span>
    <span class="dot">·</span>
    {#if detailMr.author.avatarUrl}
      <img src={detailMr.author.avatarUrl} alt={detailMr.author.login} class="au-av" />
    {:else}
      <span class="au-init">{initials(detailMr.author.displayName || detailMr.author.login)}</span>
    {/if}
    <span class="au-name">{detailMr.author.displayName || detailMr.author.login || '—'}</span>
    <span class="dot">·</span>
    <span class="meta-time">{timeAgo(detailMr.updatedAt)}</span>
    {#if detailMr.checksStatus !== 'none'}
      <span class="dot">·</span>
      <span style="font-size:11px;font-weight:500;color:{checksColor(detailMr.checksStatus)}">● {detailMr.checksStatus}</span>
    {/if}
    {#if detailMr.labels.length > 0}
      <span class="dot">·</span>
      {#each detailMr.labels as lbl}
        <span class="lbl" style="background:#{lbl.color}18;color:#{lbl.color};border-color:#{lbl.color}44">{lbl.name}</span>
      {/each}
    {/if}
  </div>

  <!-- ── Tabs ────────────────────────────────────────────────────────────── -->
  <div class="tab-bar" role="tablist">
    <button class="tab" class:ta={activeTab==='overview'} onclick={() => switchTab('overview')}>
      <FileText size={12} /> Overview
    </button>
    <button class="tab" class:ta={activeTab==='ci'} onclick={() => switchTab('ci')}>
      <Workflow size={12} /> CI
      {#if ciRunCount > 0}<span class="tc">{ciRunCount}</span>{/if}
    </button>
    <button class="tab" class:ta={activeTab==='files'} onclick={() => switchTab('files')}>
      <FileEdit size={12} /> Files
      {#if files.length > 0}<span class="tc">{files.length}</span>{/if}
    </button>
    <button class="tab" class:ta={activeTab==='commits'} onclick={() => switchTab('commits')}>
      <GitCommit size={12} /> Commits
      {#if commits.length > 0}<span class="tc">{commits.length}</span>{/if}
    </button>
  </div>

  <!-- ── Content (fixed height, no resize) ──────────────────────────────── -->
  <div class="content-area">

    <!-- OVERVIEW ──────────────────────────────────────────────────────────── -->
    {#if activeTab === 'overview'}
      <div class="overview-pane">

        {#if detailMr.description?.trim()}
          <div class="ov-section">
            <div class="sec-lbl">
              Description
              <span class="sec-copy">
                <CopyButton
                  value={detailMr.description}
                  variant="icon"
                  title="Copy description"
                  toastSuccess="Description copied"
                />
              </span>
            </div>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            <div class="desc-text md-body">{@html renderMarkdown(detailMr.description)}</div>
          </div>
        {/if}

        {#if detailMr.assignees.length > 0 || detailMr.reviewers.length > 0}
          <div class="ov-section ov-people">
            {#if detailMr.assignees.length > 0}
              <div class="people-col">
                <div class="sec-lbl">Assignees</div>
                {#each detailMr.assignees as u}
                  <span class="person">
                    {#if u.avatarUrl}<img src={u.avatarUrl} alt={u.login} class="av-xs" />
                    {:else}<span class="av-xs av-ph">{u.login[0]?.toUpperCase()}</span>{/if}
                    {u.displayName}
                  </span>
                {/each}
              </div>
            {/if}
            {#if detailMr.reviewers.length > 0}
              <div class="people-col">
                <div class="sec-lbl">Reviewers</div>
                {#each detailMr.reviewers as u}
                  <span class="person">
                    {#if u.avatarUrl}<img src={u.avatarUrl} alt={u.login} class="av-xs" />
                    {:else}<span class="av-xs av-ph">{u.login[0]?.toUpperCase()}</span>{/if}
                    {u.displayName}
                  </span>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        {#if detail?.checks && detail.checks.length > 0}
          <div class="ov-section">
            <div class="sec-lbl">CI Checks</div>
            <ul class="checks-list">
              {#each detail.checks as ck}
                {@const Ic = checkIcon(ck.status)}
                <li class="ck-item">
                  <span style="color:{checkColor(ck.status)};display:flex"><Ic size={12}/></span>
                  <span class="ck-name">{ck.name}</span>
                  {#if ck.url}<button class="ck-link" type="button" onclick={() => openUrl(ck.url!).catch(() => {})} aria-label="Open check in browser"><ExternalLink size={10}/></button>{/if}
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        <div class="ov-section">
          <div class="sec-lbl">
            <MessageSquare size={12} /> Activity
            {#if mrStore.detailLoading}<Loader size={11} class="spin" />{/if}
          </div>

          <!-- Filter chips: each toggles a class of timeline item.
               Hidden when there's nothing to filter (no comments, no bots, no events). -->
          {#if humanComments.length + botComments.length + allEvents.length > 0}
            <div class="filter-chips">
              {#if humanComments.length > 0}
                <button
                  class="chip"
                  class:chip-on={showComments}
                  onclick={() => showComments = !showComments}
                  use:tooltip={showComments ? 'Hide comments' : 'Show comments'}
                >
                  <MessageSquare size={11} />
                  <span>Comments</span>
                  <span class="chip-cnt">{humanComments.length}</span>
                </button>
              {/if}
              {#if botComments.length > 0}
                <button
                  class="chip chip-bots"
                  class:chip-on={showBots}
                  onclick={() => showBots = !showBots}
                  use:tooltip={showBots ? 'Hide bot comments' : 'Show bot comments'}
                >
                  <Bot size={11} />
                  <span>Bots</span>
                  <span class="chip-cnt">{botComments.length}</span>
                </button>
              {/if}
              {#if allEvents.length > 0}
                <button
                  class="chip chip-activity"
                  class:chip-on={showActivity}
                  onclick={() => showActivity = !showActivity}
                  use:tooltip={showActivity ? 'Hide system activity' : 'Show system activity'}
                >
                  <Activity size={11} />
                  <span>Activity</span>
                  <span class="chip-cnt">{allEvents.length}</span>
                </button>
              {/if}
            </div>
          {/if}

          {#if timeline.length > 0}
            <ul class="tl-list">
              {#each timeline as item (item.key)}
                {#if item.kind === 'comment'}
                  {@const c = item.data}
                  <li class="tl-item tl-comment" class:tl-bot={c.isBot}>
                    <span class="tl-node tl-node-lg" class:tl-node-bot={c.isBot}>
                      {#if c.author.avatarUrl}
                        <img src={c.author.avatarUrl} alt={c.author.login} />
                      {:else if c.isBot}
                        <Bot size={12} />
                      {:else}
                        <span class="tl-node-ph">{c.author.login[0]?.toUpperCase()}</span>
                      {/if}
                    </span>
                    <div class="cmt" class:cmt-bot={c.isBot}>
                      <div class="cmt-hd">
                        <strong class="cmt-au">{c.author.displayName}</strong>
                        {#if c.isBot}<span class="bot-badge"><Bot size={9} />bot</span>{/if}
                        <span class="cmt-time">{timeAgo(c.createdAt)}</span>
                        <span class="cmt-copy">
                          <CopyButton
                            value={c.body}
                            variant="icon"
                            title="Copy comment"
                            toastSuccess="Comment copied"
                          />
                        </span>
                      </div>
                      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                      <div class="cmt-body md-body">{@html renderMarkdown(c.body)}</div>
                    </div>
                  </li>
                {:else}
                  {@const ev = item.data}
                  {@const Ic = eventIcon(ev.kind)}
                  {@const tone = eventTone(ev)}
                  {@const major = isMajorEvent(ev.kind)}
                  <li class="tl-item tl-event {tone}" class:tl-minor={!major}>
                    {#if major}
                      <span class="tl-node tl-node-md tone-bg">
                        <Ic size={11} />
                      </span>
                    {:else}
                      <span class="tl-node tl-node-dot tone-bg"></span>
                    {/if}
                    <div class="ev-row">
                      {#if ev.actor.avatarUrl}<img src={ev.actor.avatarUrl} alt={ev.actor.login} class="av-xxs" />
                      {:else}<span class="av-xxs av-ph">{ev.actor.login[0]?.toUpperCase()}</span>{/if}
                      <strong class="ev-actor">{ev.actor.displayName}</strong>
                      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                      <span class="ev-summary">{@html renderEventSummary(ev.summary)}</span>
                      <span class="ev-time">{timeAgo(ev.createdAt)}</span>
                    </div>
                  </li>
                {/if}
              {/each}
            </ul>
          {:else if !mrStore.detailLoading}
            <p class="no-cmt">
              {#if humanComments.length + botComments.length + allEvents.length === 0}
                No activity yet.
              {:else}
                Nothing to show — adjust the filters above.
              {/if}
            </p>
          {/if}

          <div class="add-cmt">
            <textarea class="cmt-input" placeholder="Leave a comment…" rows="3"
              bind:value={commentTxt}
              onkeydown={(e) => { if (e.key==='Enter'&&(e.ctrlKey||e.metaKey)) doComment(); }}
            ></textarea>
            <div class="cmt-foot">
              <span class="cmt-hint">Ctrl+Enter to submit</span>
              <button class="btn btn-accent" onclick={doComment} disabled={commenting||!commentTxt.trim()}>
                {commenting ? 'Posting…' : 'Comment'}
              </button>
            </div>
          </div>
        </div>

      </div>

    <!-- FILES ─────────────────────────────────────────────────────────────── -->
    {:else if activeTab === 'files'}
      <div class="split-pane">

        <!-- Left: file list -->
        <div class="split-left">
          {#if filesLoading}
            <div class="pane-state"><Loader size={18} class="spin" /></div>
          {:else if filesError}
            <div class="pane-state err">{filesError}</div>
          {:else if files.length === 0}
            <div class="pane-state muted">No files</div>
          {:else}
            <div class="fl-summary">
              <span>{files.length} file{files.length!==1?'s':''}</span>
              <span class="add-txt">+{totalAdd}</span>
              <span class="del-txt">−{totalDel}</span>
            </div>
            <ul class="fl-list">
              {#each files as f}
                <li>
                  <button
                    class="fl-item"
                    class:fl-sel={selectedFile?.filename === f.filename}
                    onclick={() => selectedFile = f}
                    use:tooltip={f.filename}
                  >
                    <span class="fs-badge {fsCls(f.status)}">{fsBadge(f.status)}</span>
                    <span class="fl-name">{f.filename.split('/').pop()}</span>
                    <span class="fl-stats">
                      {#if f.additions}<span class="add-txt">+{f.additions}</span>{/if}
                      {#if f.deletions}<span class="del-txt">−{f.deletions}</span>{/if}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <!-- Right: diff viewer -->
        <div class="split-right">
          {#if !selectedFile}
            <div class="pane-state muted">
              <FileText size={28} style="opacity:0.25;margin-bottom:8px" />
              Select a file to view its diff
            </div>
          {:else if !selectedFile.patch}
            <div class="pane-state muted">No diff available for this file.</div>
          {:else}
            <div class="diff-hdr">
              <span class="fs-badge {fsCls(selectedFile.status)}">{fsBadge(selectedFile.status)}</span>
              <span class="diff-fname">{selectedFile.filename}</span>
              <span class="add-txt">+{selectedFile.additions}</span>
              <span class="del-txt">−{selectedFile.deletions}</span>
            </div>
            <div class="patch-wrap">
              {#each selectedFile.patch.split('\n') as line}
                <div class="dl {dlCls(line)}">
                  <span class="dl-gutter">{line[0] === '+' || line[0] === '-' ? line[0] : ' '}</span>
                  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                  <span class="dl-text">{@html hlLine(line, selectedFile.filename)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>

      </div>

    <!-- COMMITS ───────────────────────────────────────────────────────────── -->
    {:else if activeTab === 'commits'}
      <div class="split-pane">

        <!-- Left: commit list -->
        <div class="split-left commits-left">
          <div class="cm-header">
            <span>Commits</span>
            {#if commits.length > 0}
              <span class="cm-count">{commits.length}</span>
            {/if}
          </div>
          {#if commitsLoading}
            <div class="pane-state"><Loader size={18} class="spin" /></div>
          {:else if commitsError}
            <div class="pane-state err">{commitsError}</div>
          {:else if commits.length === 0}
            <div class="pane-state muted">No commits</div>
          {:else}
            <ul class="cm-list">
              {#each commits as c}
                <li>
                  <button
                    class="cm-item"
                    class:cm-sel={selectedCommit?.sha === c.sha}
                    onclick={() => selectCommit(c)}
                    use:tooltip={c.message}
                  >
                    <span class="cm-sha">{c.sha.slice(0, 7)}</span>
                    <div class="cm-body">
                      <span class="cm-msg">{c.message}</span>
                      <span class="cm-meta">{c.author} · {timeAgo(c.date)}</span>
                    </div>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <!-- Right: commit detail (files + diff) -->
        <div class="split-right commit-right">
          {#if !selectedCommit}
            <div class="pane-state muted">
              <GitCommit size={28} style="opacity:0.25;margin-bottom:8px" />
              Select a commit
            </div>
          {:else}
            <!-- Commit files list (top, fixed height) -->
            <div class="cmt-files-panel">
              {#if commitFilesLoading}
                <div class="pane-state small"><Loader size={16} class="spin" /></div>
              {:else if commitFiles.length === 0}
                <div class="pane-state small muted">No file changes</div>
              {:else}
                <div class="fl-summary small">
                  <span class="cm-sha-lg">{selectedCommit.sha.slice(0,7)}</span>
                  <span class="cm-msg-sm">{selectedCommit.message}</span>
                  <span style="margin-left:auto;display:flex;gap:6px">
                    <span class="add-txt">+{commitFiles.reduce((a,f)=>a+f.additions,0)}</span>
                    <span class="del-txt">−{commitFiles.reduce((a,f)=>a+f.deletions,0)}</span>
                  </span>
                </div>
                <ul class="fl-list">
                  {#each commitFiles as f}
                    <li>
                      <button
                        class="fl-item"
                        class:fl-sel={selectedCommitFile?.filename === f.filename}
                        onclick={() => selectedCommitFile = f}
                        use:tooltip={f.filename}
                      >
                        <span class="fs-badge {fsCls(f.status)}">{fsBadge(f.status)}</span>
                        <span class="fl-name">{f.filename.split('/').pop()}</span>
                        <span class="fl-path">{f.filename.includes('/') ? f.filename.split('/').slice(0,-1).join('/') : ''}</span>
                        <span class="fl-stats">
                          {#if f.additions}<span class="add-txt">+{f.additions}</span>{/if}
                          {#if f.deletions}<span class="del-txt">−{f.deletions}</span>{/if}
                        </span>
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>

            <!-- Diff (bottom, flex 1) -->
            <div class="cmt-diff-panel">
              {#if !selectedCommitFile}
                <div class="pane-state muted small">Select a file to view diff</div>
              {:else if !selectedCommitFile.patch}
                <div class="pane-state muted small">No diff available.</div>
              {:else}
                <div class="diff-hdr">
                  <span class="fs-badge {fsCls(selectedCommitFile.status)}">{fsBadge(selectedCommitFile.status)}</span>
                  <span class="diff-fname">{selectedCommitFile.filename}</span>
                  <span class="add-txt">+{selectedCommitFile.additions}</span>
                  <span class="del-txt">−{selectedCommitFile.deletions}</span>
                </div>
                <div class="patch-wrap">
                  {#each selectedCommitFile.patch.split('\n') as line}
                    <div class="dl {dlCls(line)}">
                      <span class="dl-gutter">{line[0] === '+' || line[0] === '-' ? line[0] : ' '}</span>
                      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                      <span class="dl-text">{@html hlLine(line, selectedCommitFile.filename)}</span>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>

      </div>

    <!-- CI ────────────────────────────────────────────────────────────────── -->
    {:else if activeTab === 'ci'}
      <div class="ci-pane">
        {#if ciProvider}
          <div class="ci-hdr">
            <span class="ci-pbadge ci-pbadge-{ciProvider.provider}">
              <BrandIcon brand={ciProvider.provider} size={11} />
              {providerLabel(ciProvider.provider)}
            </span>
            <span class="ci-branch-hint">
              <GitBranch size={10} /> {detailMr.sourceBranch}
            </span>
            <span class="ci-spacer"></span>
            {#if ciProvider.has_token}
              <button class="ci-mini-btn" use:tooltip={'Refresh runs'} onclick={refreshCi} disabled={mrCiLoading}>
                <RefreshCw size={11} class={mrCiLoading ? 'spin' : ''} />
              </button>
            {/if}
          </div>
        {/if}

        {#if !ciProvider && !pipelinesStore.ciLoading}
          <div class="ci-state">
            <AlertCircle size={26} class="ci-state-icon" />
            <p class="ci-state-title">No CI/CD remote detected</p>
            <p class="ci-state-hint">This repository has no GitHub or GitLab remote.</p>
          </div>

        {:else if ciProvider && !ciProvider.has_token}
          <div class="ci-state">
            <AlertCircle size={26} class="ci-state-icon ci-state-warn" />
            <p class="ci-state-title">{providerLabel(ciProvider.provider)} detected</p>
            <p class="ci-state-hint">
              Connect your account in <strong>Settings → Authentication</strong> to view runs.
            </p>
          </div>

        {:else if mrCiLoading && branchRuns.length === 0}
          <div class="ci-state">
            <Loader size={22} class="spin" />
            <p class="ci-state-hint">Loading runs…</p>
          </div>

        {:else if mrCiError}
          <div class="ci-state">
            <AlertCircle size={26} class="ci-state-icon ci-state-warn" />
            <p class="ci-state-title">Failed to load runs</p>
            <p class="ci-state-hint ci-err">{mrCiError}</p>
            <button class="ci-mini-btn" onclick={refreshCi}><RefreshCw size={11}/> Retry</button>
          </div>

        {:else if branchRuns.length === 0}
          <div class="ci-state">
            <Circle size={26} class="ci-state-icon" />
            <p class="ci-state-title">No runs found for this branch</p>
            <p class="ci-state-hint">
              Push a commit to <code>{detailMr.sourceBranch}</code> to trigger a run.
            </p>
          </div>

        {:else}
          <ul class="ci-runs">
            {#each branchRuns as run (run.id)}
              {@const isHead = run.commit_sha === headShort}
              {@const CiStatusIcon = ciStatusIcon(run.status)}
              <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
              <li>
                <div
                  class="ci-card"
                  class:ci-card-head={isHead}
                  role="button"
                  tabindex="0"
                  onclick={() => { selectedCiRun = run; }}
                  onkeydown={(e) => e.key === 'Enter' && (selectedCiRun = run)}
                >
                  <div class="ci-card-l">
                    <span class="ci-status ci-st-{run.status}">
                      <CiStatusIcon
                        size={11}
                        class={run.status === 'running' ? 'spin' : ''}
                      />
                      {ciStatusLabel(run.status)}
                    </span>
                    {#if run.duration_secs}
                      <span class="ci-dur"><Clock size={9}/> {formatDuration(run.duration_secs)}</span>
                    {/if}
                  </div>

                  <div class="ci-card-c">
                    <div class="ci-card-title">
                      <span class="ci-name">{run.name}</span>
                      <span class="ci-id">#{run.id}</span>
                      {#if isHead}
                        <span class="ci-head-pill" use:tooltip={'Built the current PR head'}>PR HEAD</span>
                      {/if}
                    </div>
                    <div class="ci-chips">
                      <span class="ci-chip ci-sha">{run.commit_sha}</span>
                      <span class="ci-chip ci-time">{timeAgo(run.created_at)}</span>
                    </div>
                  </div>

                  <div class="ci-card-r" role="toolbar" tabindex="-1" aria-label="CI run actions" onclick={(e) => e.stopPropagation()}>
                    <button
                      class="ci-mini-btn"
                      use:tooltip={'Re-trigger'}
                      disabled={retriggeringId === run.id || run.status === 'running'}
                      onclick={() => handleRetriggerForRun(run)}
                    >
                      {#if retriggeringId === run.id}
                        <Loader size={11} class="spin" />
                      {:else}
                        <RotateCcw size={11} />
                      {/if}
                    </button>
                    <button class="ci-mini-btn" type="button" onclick={() => openUrl(run.web_url).catch(() => {})} use:tooltip={'Open in browser'} aria-label="Open in browser">
                      <ExternalLink size={11} />
                    </button>
                  </div>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

  </div>
  </div>

  {#snippet footer()}
    <div class="ftr-left">
      {#if detailMr.state === 'open' && detailMr.autoMergeEnabled}
        <span class="ftr-armed" use:tooltip={detailMr.provider === 'gitlab'
            ? 'Will merge automatically once the pipeline succeeds.'
            : 'Will merge automatically once required checks pass.'}>
          <Wand2 size={12}/> Auto-merge armed
        </span>
      {:else if detailMr.state === 'open' && detailMr.isDraft}
        <span class="ftr-warn"><AlertCircle size={12}/> Draft — cannot merge</span>
      {:else if detailMr.state === 'open' && mergeableChecking}
        <span class="ftr-warn ftr-checking"><Loader size={12} class="spin"/> Checking merge status…</span>
      {:else if detailMr.state === 'open' && detailMr.mergeable === false}
        <span class="ftr-warn"><AlertCircle size={12}/> Merge conflicts</span>
      {/if}
    </div>
    <div class="ftr-right">
      {#if detailMr.state === 'open' && !detailMr.autoMergeEnabled && !hasConflicts && !mergeableChecking}
        <label class="ftr-check" use:tooltip={'Squash all commits into one before merging'}>
          <input type="checkbox" bind:checked={mergeSquash} />
          <span>Squash</span>
        </label>
        <label class="ftr-check" use:tooltip={{ content: 'Delete source branch after merge', description: 'Removes both remote and local copy (keeps it when a worktree still uses it, or when switching to the target fails).' }}>
          <input type="checkbox" bind:checked={mergeDelete} />
          <span>Delete branch</span>
        </label>
        <span class="ftr-div"></span>
      {/if}
      {#if detailMr.state === 'closed'}
        <Button variant="secondary" onclick={doReopen} disabled={acting}>{acting ? 'Reopening…' : 'Reopen'}</Button>
      {:else if detailMr.state === 'open' && detailMr.autoMergeEnabled}
        <Button variant="danger" onclick={requestClose} disabled={acting} title={`Close this ${detailMr.provider === 'gitlab' ? 'merge request' : 'pull request'} without merging`}>{acting ? 'Closing…' : 'Close'}</Button>
        <Button variant="secondary" onclick={doDisableAutoMerge} disabled={acting} title="Cancel the armed auto-merge so the MR stays open until you merge it manually">
          {acting ? 'Disabling…' : 'Disable auto-merge'}
        </Button>
      {:else if detailMr.state === 'open'}
        <Button variant="danger" onclick={requestClose} disabled={acting} title={`Close this ${detailMr.provider === 'gitlab' ? 'merge request' : 'pull request'} without merging`}>{acting ? 'Closing…' : 'Close'}</Button>
        {#if detailMr.isDraft}
          <button class="btn btn-ready" onclick={doMarkReady} disabled={acting} use:tooltip={'Remove draft status and mark as ready for review'}>
            {acting ? 'Updating…' : 'Mark as ready'}
          </button>
        {/if}
        {#if mergeableChecking}
          <div class="merge-split merge-dis" use:tooltip={'Checking merge status…'}>
            <button class="ms-main" disabled>
              <Loader size={12} class="spin" />
              Checking…
            </button>
            <button class="ms-caret" disabled>
              <ChevronDown size={11}/>
            </button>
          </div>
        {:else if hasConflicts}
          {#if acting}
            <div class="resolve-progress" role="status" aria-live="polite">
              <ProgressStepper
                steps={RESOLVE_STEPS}
                current={resolveProgress?.phase ?? 'status'}
                activeDetail={resolveProgress?.detail ?? null}
                layout="horizontal"
                size="sm"
                error={resolveError}
              />
            </div>
          {:else}
            <button
              class="btn btn-resolve"
              onclick={doResolveConflicts}
              use:tooltip={{ content: `Fetch, checkout ${detailMr.sourceBranch}`, description: `Merges ${detailMr.targetBranch} into it so conflicts can be resolved locally` }}
            >
              <TriangleAlert size={12}/>
              Resolve Conflicts
            </button>
          {/if}
        {:else}
          <div class="merge-split" class:merge-dis={mergeBlocked} use:tooltip={mergeBlockReason ?? ''}>
            <button class="ms-main" onclick={() => doMerge('merge')} disabled={mergeBlocked}>
              {#if acting}<Loader size={12} class="spin" />{:else}<GitMerge size={12}/>{/if}
              {acting ? 'Merging…' : 'Merge'}
            </button>
            <button class="ms-caret" onclick={(e)=>{e.stopPropagation();mergeMenu=!mergeMenu;}}
              disabled={mergeBlocked} use:tooltip={mergeBlockReason ?? 'More merge options'}>
              <ChevronDown size={11}/>
            </button>
          </div>
          {#if mergeMenu}
            <div class="merge-dd">
              <button class="md-opt" onclick={() => doMerge('merge')}>
                <strong>Merge commit</strong><span>Preserve all commits</span>
              </button>
              <button class="md-opt" onclick={() => doMerge('squash')}>
                <strong>Squash and merge</strong><span>Combine into one commit</span>
              </button>
              {#if mr.provider === 'github'}
                <button class="md-opt" onclick={() => doMerge('rebase')}>
                  <strong>Rebase and merge</strong><span>Rebase onto base branch</span>
                </button>
              {/if}
            </div>
          {/if}
        {/if}
      {/if}
    </div>
  {/snippet}

</Modal>

{#if selectedCiRun}
  <CiPipelineDetailModal
    run={selectedCiRun}
    {tabId}
    onClose={() => { selectedCiRun = null; }}
    onRetrigger={() => handleRetriggerForRun(selectedCiRun!)}
  />
{/if}

{#if confirmCloseOpen}
  <ConfirmModal
    title="Close {detailMr.provider === 'gitlab' ? 'merge request' : 'pull request'} #{mr.number}?"
    message="This will close “{detailMr.title}” without merging."
    detail="You can reopen it later, but its CI runs and reviews may be affected."
    variant="danger"
    confirmLabel={acting ? 'Closing…' : `Close #${mr.number}`}
    cancelLabel="Cancel"
    busy={acting}
    onConfirm={confirmCloseMr}
    onCancel={() => { confirmCloseOpen = false; }}
  />
{/if}

<style>
  .merge-backdrop { position:fixed;inset:0;z-index:calc(var(--z-modal) + 1); background:transparent;border:none;padding:0;cursor:default; }
  @keyframes fadeIn { from{opacity:0} to{opacity:1} }

  .state-badge {
    display:inline-flex;align-items:center;gap:4px;
    font-size:11px;font-weight:600;padding:3px 9px;border-radius:99px;flex-shrink:0;
  }
  .state-open   { background:color-mix(in srgb, var(--success) 20%, var(--bg-base));   color:var(--success); }
  .state-merged { background:color-mix(in srgb, var(--color-tag) 20%, var(--bg-base)); color:var(--color-tag); }
  .state-closed { background:var(--bg-base);color:var(--text-muted);border:1px solid var(--border); }
  .draft-badge  { font-size:11px;font-weight:500;color:var(--text-muted);border:1px solid var(--border);border-radius: var(--radius-sm);padding:2px 7px;flex-shrink:0; }
  /* Title typography is injected via the global `.modal-title` recipe by
     ModalHeader; we just reset margins / button chrome. */
  :global(h2.modal-title)  { margin:0; flex:1; min-width:0; }
  .modal-title-btn {
    margin:0; flex:1; min-width:0;
    background:transparent; border:1px solid transparent;
    padding:2px 6px; border-radius:var(--radius-sm);
    color:inherit; text-align:left; cursor:pointer;
    font:inherit;
    transition:background var(--transition-fast),border-color var(--transition-fast);
  }
  .modal-title-btn:hover {
    background:var(--bg-hover);
    border-color:var(--border-subtle);
  }
  .modal-num    { font-size:12px;color:var(--text-disabled);font-family:var(--font-code);flex-shrink:0; }
  .modal-num-btn {
    background:transparent; border:1px solid transparent;
    padding:1px 6px; border-radius:var(--radius-sm);
    cursor:pointer; font-family:var(--font-code);
    transition:background var(--transition-fast),color var(--transition-fast),border-color var(--transition-fast);
  }
  .modal-num-btn:hover {
    background:var(--bg-hover);
    color:var(--text-secondary);
    border-color:var(--border-subtle);
  }
  .icon-btn     { display:flex;align-items:center;justify-content:center;width:22px;height:22px;border:none;background:transparent;color:var(--text-muted);border-radius:var(--radius-sm);cursor:pointer;text-decoration:none;transition:background var(--transition-fast),color var(--transition-fast); }
  .icon-btn:hover:not(:disabled) { background:var(--bg-hover);color:var(--text-secondary); }
  .icon-btn:disabled { opacity:.55;cursor:default; }

  .body-stack {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Meta ───────────────────────────────────────────────────────────── */
  .modal-meta {
    display:flex;align-items:center;flex-wrap:wrap;gap:5px;
    padding: 8px 14px;
    font-size:11px;color:var(--text-muted);
    flex-shrink:0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .bp { font-family:var(--font-code);font-size:10px;padding:2px 7px;border-radius: var(--radius-sm);white-space:nowrap;max-width:150px;overflow:hidden;text-overflow:ellipsis; }
  .bp.src { background:color-mix(in srgb,var(--accent) 12%,transparent);color:var(--accent);border:1px solid color-mix(in srgb,var(--accent) 25%,transparent); }
  .bp.tgt { background:var(--bg-overlay);color:var(--text-secondary);border:1px solid var(--border-subtle); }
  :global(.meta-arr) { color:var(--text-disabled);flex-shrink:0; }
  .dot  { color:var(--text-disabled); }
  .au-av   { width:16px;height:16px;border-radius:50%;object-fit:cover;flex-shrink:0; }
  .au-init { width:16px;height:16px;border-radius:50%;background:var(--accent-subtle);color:var(--accent);font-size:9px;font-weight:700;display:inline-flex;align-items:center;justify-content:center;flex-shrink:0; }
  .au-name { color:var(--text-secondary);font-weight:500; }
  .meta-time { color:var(--text-muted); }
  .lbl { font-size:10px;font-weight:600;padding:2px 7px;border-radius:99px;border:1px solid;white-space:nowrap; }

  /* ── Tab bar ──────────────────────────────────────────────────────────── */
  .tab-bar {
    display:flex;padding:0 12px;
    flex-shrink:0;background:transparent;gap:2px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .tab {
    display:inline-flex;align-items:center;gap:5px;padding:7px 12px;
    font-size:12px;font-weight:500;color:var(--text-muted);
    background:transparent;border:none;border-bottom:2px solid transparent;
    cursor:pointer;font-family:var(--font-ui-sans);
    transition:color var(--transition-fast),border-color var(--transition-fast);white-space:nowrap;
  }
  .tab:hover { color:var(--text-secondary); }
  .ta { color:var(--accent)!important;border-bottom-color:var(--accent); }
  .tc {
    display:inline-flex;align-items:center;justify-content:center;
    min-width:17px;height:15px;padding:0 4px;
    background:var(--bg-elevated);border:1px solid var(--border-subtle);
    border-radius:99px;font-size:10px;font-weight:600;color:var(--text-muted);
  }
  .ta .tc { background:color-mix(in srgb,var(--accent) 15%,transparent);border-color:color-mix(in srgb,var(--accent) 30%,transparent);color:var(--accent); }

  /* ── Content area (fills remaining space, never resizes modal) ────────── */
  .content-area {
    flex:1;
    overflow:hidden;
    display:flex;flex-direction:column;
  }

  /* ── Overview pane ────────────────────────────────────────────────────── */
  .overview-pane { flex:1;overflow-y:auto;overflow-x:hidden; }
  .ov-section { padding:12px 16px;border-bottom:1px solid var(--border-subtle); }
  .ov-section:last-child { border-bottom:none; }
  .ov-people  { display:flex;gap:24px; }
  .sec-lbl    { display:flex;align-items:center;gap:5px;font-size:11px;font-weight:600;color:var(--text-muted);margin-bottom:6px;letter-spacing:.02em; }
  .sec-copy   { margin-left:auto;opacity:0;transition:opacity var(--transition-fast); }
  .sec-copy, .sec-copy :global(*) { user-select:none; }
  .ov-section:hover .sec-copy,
  .ov-section:focus-within .sec-copy { opacity:1; }
  .desc-text  { font-size:13px;color:var(--text-secondary);word-break:break-word;line-height:1.6;user-select:text; }
  .desc-text :global(*) { user-select:text; }

  .people-col { flex:1;min-width:0;display:flex;flex-direction:column;gap:5px; }
  .person     { display:inline-flex;align-items:center;gap:6px;font-size:12px;color:var(--text-secondary); }
  .av-xs      { width:18px;height:18px;border-radius:50%;object-fit:cover;flex-shrink:0; }
  .av-ph      { background:var(--accent-subtle);color:var(--accent);font-size:9px;font-weight:700;display:inline-flex;align-items:center;justify-content:center; }

  .checks-list { list-style:none;padding:0;margin:0;display:flex;flex-direction:column;gap:4px; }
  .ck-item     { display:flex;align-items:center;gap:6px;font-size:12px;color:var(--text-secondary); }
  .ck-name     { flex:1; }
  .ck-link     { color:var(--text-muted);display:flex;text-decoration:none;background:none;border:none;padding:0;cursor:pointer; }
  .ck-link:hover { color:var(--accent); }

  .cnt-badge  { display:inline-flex;align-items:center;justify-content:center;min-width:18px;height:15px;padding:0 4px;background:var(--bg-overlay);border-radius:99px;font-size:10px;font-weight:600;color:var(--text-muted);border:1px solid var(--border-subtle); }

  /* ── Activity timeline filter chips ───────────────────────────────────── */
  .filter-chips { display:flex;flex-wrap:wrap;gap:6px;margin-bottom:10px; }
  .chip {
    display:inline-flex;align-items:center;gap:5px;
    padding:3px 8px 3px 7px;
    background:transparent;
    border:1px solid var(--border-subtle);
    border-radius:99px;
    font-size:11px;font-weight:500;
    color:var(--text-muted);
    cursor:pointer;
    transition:background var(--transition-fast),border-color var(--transition-fast),color var(--transition-fast);
    user-select:none;
  }
  .chip:hover { background:var(--bg-hover);color:var(--text-secondary); }
  .chip-on {
    background:var(--accent-subtle);
    border-color:color-mix(in srgb,var(--accent) 35%,transparent);
    color:var(--accent);
  }
  .chip-on:hover { background:color-mix(in srgb,var(--accent) 18%,transparent);color:var(--accent); }
  .chip-cnt {
    display:inline-flex;align-items:center;justify-content:center;
    min-width:14px;height:14px;padding:0 4px;
    background:var(--bg-overlay);
    border-radius:99px;
    font-size:9.5px;font-weight:600;
    color:inherit;opacity:.85;
  }
  .chip-on .chip-cnt { background:color-mix(in srgb,var(--accent) 22%,var(--bg-elevated)); }

  /* Bots chip uses the same yellow accent as bot comment cards. The count
     pill is always yellow (so the cue lands even when the chip is off);
     the rest of the chip only goes yellow when toggled on, matching how
     the other chips switch hue on activation. */
  .chip-bots .chip-cnt {
    background: color-mix(in srgb, var(--warning) 22%, var(--bg-elevated));
    color: var(--warning); opacity: 1; font-weight: 700;
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
  }
  .chip-bots.chip-on {
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    border-color: color-mix(in srgb, var(--warning) 45%, transparent);
    color: var(--warning);
  }
  .chip-bots.chip-on:hover { background: color-mix(in srgb, var(--warning) 22%, transparent); color: var(--warning); }
  .chip-bots.chip-on .chip-cnt { background: color-mix(in srgb, var(--warning) 28%, var(--bg-elevated)); }

  /* Activity chip — purple, distinct from comments (blue) and bots (yellow).
     Same recipe as bots: count pill always tinted; full chip flips when on. */
  .chip-activity .chip-cnt {
    background: color-mix(in srgb, var(--color-tag) 22%, var(--bg-elevated));
    color: var(--color-tag); opacity: 1; font-weight: 700;
    border: 1px solid color-mix(in srgb, var(--color-tag) 30%, transparent);
  }
  .chip-activity.chip-on {
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-tag) 45%, transparent);
    color: var(--color-tag);
  }
  .chip-activity.chip-on:hover { background: color-mix(in srgb, var(--color-tag) 22%, transparent); color: var(--color-tag); }
  .chip-activity.chip-on .chip-cnt { background: color-mix(in srgb, var(--color-tag) 28%, var(--bg-elevated)); }

  /* ── Activity timeline list (comments + events interleaved) ─────────────
     GitLab-style vertical rail with nodes. The rail is a 2px line drawn by
     a ::before pseudo-element on the <ul>; each <li> hangs a circular node
     to the left, its background matching the surrounding section so the
     line "punches through" cleanly. */
  .tl-list {
    list-style:none;
    padding:4px 0 4px 32px;
    margin:0 0 10px;
    position:relative;
    display:flex;flex-direction:column;gap:6px;
  }
  .tl-list::before {
    content:'';
    position:absolute;
    left:12px; top:6px; bottom:6px;
    width:2px;
    /* Use --border (vs --border-subtle): Ayu Dark's border-subtle is only
       a few percent lighter than bg-base, which renders the rail nearly
       invisible on the dark canvas. Stepping up to --border keeps it
       subtle on the stock dark theme while staying visible everywhere. */
    background:var(--border);
    border-radius:2px;
  }
  .tl-item { position:relative; margin:0; }

  .tl-node {
    position:absolute;
    border-radius:50%;
    display:inline-flex;align-items:center;justify-content:center;
    background:var(--bg-base);
    border:2px solid var(--border-subtle);
    color:var(--text-secondary);
    z-index:1;
    overflow:hidden;
    box-sizing:content-box;
  }
  /* Avatar-sized node — anchors comment items */
  .tl-node-lg { width:24px;height:24px;left:-32px;top:2px;padding:0; }
  .tl-node-lg img { width:100%;height:100%;object-fit:cover;border-radius:50%; }
  .tl-node-lg .tl-node-ph {
    width:100%;height:100%;display:inline-flex;align-items:center;justify-content:center;
    background:var(--accent-subtle);color:var(--accent);
    font-size:11px;font-weight:700;font-family:var(--font-ui-sans);
  }
  /* Bot rail-node — warning-tint border matches the bot comment card so
     the visual cue is consistent on both sides of the rail. */
  .tl-node-bot {
    border-color: color-mix(in srgb, var(--warning) 55%, transparent);
    background: color-mix(in srgb, var(--warning) 18%, var(--bg-base));
    color: var(--warning);
  }
  .tl-node-bot .tl-node-ph { background: transparent; color: var(--warning); }

  /* Icon-sized node — anchors major timeline events (state/commit/rename) */
  .tl-node-md { width:20px;height:20px;left:-30px;top:5px; }

  /* Dot — anchors minor events (label/assign/review/system) */
  .tl-node-dot {
    width:8px;height:8px;left:-24px;top:11px;
    border-width:2px;
    background:var(--bg-base);
  }

  /* ── Tone classes (per-event color) ────────────────────────────────────
     Applied to the <li>; the .tone-bg helper picks up the variables and
     paints the node bg/icon. Keeping the colors on CSS variables means
     swapping to a light theme later just needs a media query. */
  .tone-danger  { --tone-bg:color-mix(in srgb,var(--error) 16%,var(--bg-base));   --tone-fg:var(--error); --tone-border:color-mix(in srgb,var(--error) 60%,transparent); }
  .tone-merged  { --tone-bg:color-mix(in srgb,var(--color-tag) 16%,var(--bg-base));   --tone-fg:var(--color-tag); --tone-border:color-mix(in srgb,var(--color-tag) 60%,transparent); }
  .tone-success { --tone-bg:color-mix(in srgb,var(--success) 16%,var(--bg-base));   --tone-fg:var(--success); --tone-border:color-mix(in srgb,var(--success) 60%,transparent); }
  .tone-muted   { --tone-bg:var(--bg-overlay);                               --tone-fg:var(--text-muted); --tone-border:var(--border-subtle); }
  .tone-state   { --tone-bg:color-mix(in srgb,#d2a8ff 16%,var(--bg-base));   --tone-fg:#d2a8ff; --tone-border:color-mix(in srgb,#d2a8ff 50%,transparent); }
  .tone-label   { --tone-bg:color-mix(in srgb,#79c0ff 16%,var(--bg-base));   --tone-fg:#79c0ff; --tone-border:color-mix(in srgb,#79c0ff 50%,transparent); }
  .tone-assign  { --tone-bg:color-mix(in srgb,#7ee787 16%,var(--bg-base));   --tone-fg:#7ee787; --tone-border:color-mix(in srgb,#7ee787 50%,transparent); }
  .tone-review  { --tone-bg:color-mix(in srgb,#ffa657 16%,var(--bg-base));   --tone-fg:#ffa657; --tone-border:color-mix(in srgb,#ffa657 50%,transparent); }
  .tone-commit  { --tone-bg:color-mix(in srgb,var(--color-tag) 16%,var(--bg-base));   --tone-fg:var(--color-tag); --tone-border:color-mix(in srgb,var(--color-tag) 50%,transparent); }
  .tone-rename  { --tone-bg:color-mix(in srgb,var(--warning) 16%,var(--bg-base));   --tone-fg:var(--warning); --tone-border:color-mix(in srgb,var(--warning) 50%,transparent); }
  .tone-system  { --tone-bg:var(--bg-overlay);                               --tone-fg:var(--text-muted); --tone-border:var(--border-subtle); }

  .tl-event .tone-bg { background:var(--tone-bg);border-color:var(--tone-border);color:var(--tone-fg); }
  .tl-event.tl-minor .tone-bg { background:var(--tone-fg);border-color:var(--tone-border); }

  .cmt-list   { list-style:none;padding:0;margin:0 0 10px;display:flex;flex-direction:column;gap:7px; }
  /* Regular comment card — soft blue tint lifts the card off the section
     bg so the body text isn't grey-on-grey. Same structural recipe as
     bot cards (tinted bg + accent strip + tinted border) but in the
     neutral accent (blue) so the "is this a bot?" cue still lands cleanly. */
  .cmt {
    position: relative;
    background: color-mix(in srgb, var(--accent) 6%, var(--bg-elevated));
    border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
    border-radius: var(--radius-md);
    padding: 9px 11px 9px 13px;
  }
  .cmt::before {
    content: '';
    position: absolute;
    left: 0; top: 0; bottom: 0;
    width: 3px;
    background: var(--accent);
    border-radius: var(--radius-md) 0 0 var(--radius-md);
  }
  .cmt-hd     { display:flex;align-items:center;gap:6px;margin-bottom:5px; }
  .cmt-au     { font-size:12px;font-weight:600;color:var(--text-primary); }
  .cmt-time   { font-size:10px;color:var(--text-muted);margin-left:auto; }
  .cmt-copy   { opacity:0;transition:opacity var(--transition-fast); }
  .cmt-copy, .cmt-copy :global(*) { user-select:none; }
  .cmt:hover .cmt-copy,
  .cmt:focus-within .cmt-copy { opacity:1; }
  .cmt-body   { font-size:12px;color:var(--text-primary);word-break:break-word;line-height:1.55;user-select:text; }
  .cmt-body :global(*) { user-select:text; }

  /* ── Markdown renderer styles (matches IssueDetailModal) ──────────────── */
  :global(.md-body .md-pre) {
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); padding: 10px 12px; margin: 8px 0;
    overflow-x: auto; font-family: var(--font-code); font-size: 11px;
    line-height: 1.6;
  }
  :global(.md-body .md-code) {
    font-family: var(--font-code); font-size: 11px; background: none; padding: 0;
  }
  :global(.md-body .md-inline-code) {
    font-family: var(--font-code); font-size: 11px;
    background: var(--bg-overlay); padding: 1px 5px; border-radius: var(--radius-sm);
    color: var(--accent); border: 1px solid var(--border-subtle);
  }
  :global(.md-body strong) { color: var(--text-primary); font-weight: 600; }
  :global(.md-body em)     { color: var(--text-secondary); }
  :global(.md-body .md-h1) { font-size: 15px; font-weight: 700; color: var(--text-primary); margin: 12px 0 6px; display: block; }
  :global(.md-body .md-h2) { font-size: 13px; font-weight: 600; color: var(--text-primary); margin: 10px 0 4px; display: block; border-bottom: 1px solid var(--border-subtle); padding-bottom: 3px; }
  :global(.md-body .md-h3) { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 8px 0 4px; display: block; }
  :global(.md-body .md-p)  { margin: 2px 0; }
  :global(.md-body .md-spacer) { height: 6px; }
  :global(.md-body .md-bq) {
    border-left: 3px solid var(--accent-subtle); padding: 4px 10px;
    color: var(--text-muted); font-style: italic; margin: 6px 0;
    background: var(--bg-overlay); border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
  }
  :global(.md-body .md-ul),
  :global(.md-body .md-ol) { padding-left: 18px; margin: 4px 0; }
  :global(.md-body .md-ul li),
  :global(.md-body .md-ol li) { margin: 2px 0; }
  :global(.md-body .md-hr) { border: none; border-top: 1px solid var(--border-subtle); margin: 10px 0; }
  :global(.md-body .md-link) { color: var(--accent); cursor: pointer; }

  /* ── Raw-HTML passthrough (GitHub PR bodies / Dependabot) ──────────────
     The renderer keeps a safelist of HTML tags as-is (see markdown.ts).
     Style them so they sit next to the md-* output without falling back to
     ugly browser defaults. */
  :global(.md-body p) { margin: 2px 0; }
  :global(.md-body h1) { font-size: 15px; font-weight: 700; color: var(--text-primary); margin: 12px 0 6px; }
  :global(.md-body h2) { font-size: 13px; font-weight: 600; color: var(--text-primary); margin: 10px 0 4px; border-bottom: 1px solid var(--border-subtle); padding-bottom: 3px; }
  :global(.md-body h3) { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 8px 0 4px; }
  :global(.md-body h4),
  :global(.md-body h5),
  :global(.md-body h6) { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin: 6px 0 3px; }
  :global(.md-body ul:not(.md-ul)),
  :global(.md-body ol:not(.md-ol)) { padding-left: 22px; margin: 6px 0; }
  :global(.md-body ul:not(.md-ul) li),
  :global(.md-body ol:not(.md-ol) li) { margin: 3px 0; padding-left: 2px; }
  :global(.md-body blockquote:not(.md-bq)) {
    border-left: 3px solid var(--accent-subtle); padding: 4px 12px;
    margin: 6px 0 6px 4px; background: var(--bg-overlay);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0; color: var(--text-secondary);
  }
  :global(.md-body code) {
    font-family: var(--font-code); font-size: 11px;
    background: var(--bg-overlay); padding: 1px 5px; border-radius: var(--radius-sm);
    color: var(--accent); border: 1px solid var(--border-subtle);
  }
  :global(.md-body pre code) { background: none; border: none; padding: 0; color: inherit; }

  /* Collapsible <details>: a bordered card. The summary is the header (always
     visible), the body sits inside the same border so the chevron + label
     "close" the box visually. */
  :global(.md-body details) {
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    margin: 8px 0; background: var(--bg-overlay);
    overflow: hidden;
  }
  :global(.md-body details > summary) {
    cursor: pointer; padding: 6px 10px;
    font-weight: 600; color: var(--text-primary);
    list-style: none; display: flex; align-items: center; gap: 6px;
    user-select: none;
  }
  :global(.md-body details > summary::-webkit-details-marker) { display: none; }
  :global(.md-body details > summary::before) {
    content: '▶'; font-size: 9px; color: var(--text-muted);
    transition: transform 120ms ease; display: inline-block;
  }
  :global(.md-body details[open] > summary::before) { transform: rotate(90deg); }
  :global(.md-body details[open] > summary) {
    border-bottom: 1px solid var(--border-subtle);
    background: color-mix(in srgb, var(--bg-elevated) 50%, transparent);
  }
  :global(.md-body details > *:not(summary)) { padding: 0 12px; }
  :global(.md-body details > *:not(summary):first-of-type) { padding-top: 8px; }
  :global(.md-body details > *:not(summary):last-child) { padding-bottom: 8px; }
  /* nested details: tighter, no double-border noise */
  :global(.md-body details details) { margin: 6px 0; }

  .no-cmt     { margin:0 0 10px;font-size:12px;color:var(--text-disabled);font-style:italic; }

  /* Bot comments — soft warning tint (yellow) lifts them off the dark
     background while signalling "automated content". The body text stays
     full-strength so it's readable; only the chrome is tinted. */
  .cmt-bot {
    background: color-mix(in srgb, var(--warning) 7%, var(--bg-elevated));
    border-color: color-mix(in srgb, var(--warning) 32%, transparent);
    border-style: solid;
    position: relative;
  }
  /* Left-edge accent strip — strong cue, takes up no extra vertical space */
  .cmt-bot::before {
    content:''; position:absolute;
    left:0; top:0; bottom:0;
    width:3px;
    background:var(--warning);
    border-radius:var(--radius-md) 0 0 var(--radius-md);
  }
  .cmt-bot .cmt-au { color: var(--text-primary); }
  .cmt-bot .cmt-body { color: var(--text-primary); }
  .bot-badge {
    display:inline-flex;align-items:center;gap:3px;
    padding:1px 7px 1px 6px;
    background: color-mix(in srgb, var(--warning) 18%, var(--bg-elevated));
    border:1px solid color-mix(in srgb, var(--warning) 40%, transparent);
    border-radius:99px;
    font-size:9px;font-weight:700;
    color:var(--warning);
    text-transform:uppercase;letter-spacing:.05em;
  }

  /* ── Timeline event row (sits next to a rail node) ─────────────────── */
  .ev-row {
    display:flex;align-items:center;gap:6px;
    padding:3px 0 3px 2px;
    font-size:11.5px;color:var(--text-secondary);
    line-height:1.5;min-width:0;
  }
  .tl-event { padding:1px 0; }
  .tl-event.tl-minor .ev-row { padding:1px 0 1px 2px; font-size:11px; color:var(--text-muted); }
  .av-xxs       { width:14px;height:14px;border-radius:50%;object-fit:cover;flex-shrink:0; }
  .av-xxs.av-ph { background:var(--accent-subtle);color:var(--accent);font-size:8px;font-weight:700;display:inline-flex;align-items:center;justify-content:center; }
  .ev-actor     { font-weight:600;color:var(--text-primary);font-size:11.5px;flex-shrink:0; }
  .tl-minor .ev-actor { font-weight:500;color:var(--text-secondary); }
  .ev-summary   {
    flex:1;min-width:0;
    overflow:hidden;text-overflow:ellipsis;white-space:nowrap;
  }
  :global(.ev-summary strong) { color:var(--text-primary);font-weight:600; }
  :global(.ev-summary .ev-code) {
    font-family:var(--font-code);font-size:10.5px;
    background:var(--bg-overlay);padding:0 4px;border-radius: var(--radius-sm);color:var(--accent);
  }
  .ev-time      { font-size:10px;color:var(--text-muted);flex-shrink:0; }
  .add-cmt    { display:flex;flex-direction:column;gap:6px; }
  .cmt-input  { width:100%;min-height:60px;resize:vertical;background:var(--bg-elevated);border:1px solid var(--border);border-radius:var(--radius-md);color:var(--text-primary);font-family:var(--font-ui-sans);font-size:12px;padding:8px 10px;box-sizing:border-box;outline:none;transition:border-color var(--transition-fast),box-shadow var(--transition-fast);line-height:1.5; }
  .cmt-input:focus { border-color:var(--accent);box-shadow:0 0 0 2px color-mix(in srgb,var(--accent) 20%,transparent); }
  .cmt-foot   { display:flex;align-items:center;justify-content:space-between; }
  .cmt-hint   { font-size:10px;color:var(--text-disabled); }

  /* ── Split pane (Files + Commits tabs) ────────────────────────────────── */
  .split-pane {
    display:flex;flex:1;overflow:hidden;height:100%;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }

  .split-left {
    width: 240px;
    flex-shrink: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .commits-left { width: 270px; }

  .split-right {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  /* ── Pane states ──────────────────────────────────────────────────────── */
  .pane-state {
    display:flex;flex-direction:column;align-items:center;justify-content:center;
    flex:1;gap:6px;padding:24px 16px;color:var(--text-muted);font-size:12px;text-align:center;
  }
  .pane-state.muted { color:var(--text-disabled); }
  .pane-state.err   { color:var(--status-error, var(--error));font-size:11px; }
  .pane-state.small { padding:12px; }

  /* ── File list (card-list pattern) ─────────────────────────────────────── */
  .fl-summary {
    display:flex;align-items:center;gap:6px;padding:10px 12px;
    font-size:10px;font-weight:600;letter-spacing:0.06em;text-transform:uppercase;
    font-family:var(--font-ui-sans);
    color:var(--text-muted);border-bottom:1px solid var(--border-subtle);
    flex-shrink:0;
  }
  .fl-summary.small { font-size:10px; }
  .fl-list    {
    list-style:none;margin:0;overflow-y:auto;flex:1;
    padding:6px 8px;
    display:flex;flex-direction:column;gap:4px;
  }
  .fl-item {
    display:flex;align-items:center;gap:7px;width:100%;padding:7px 10px;
    background:var(--bg-elevated);
    border:1px solid var(--border-subtle);
    border-radius:var(--radius-md);
    text-align:left;cursor:pointer;
    font-family:var(--font-ui-sans);font-size:11px;color:var(--text-secondary);
    transition:background var(--transition-fast),border-color var(--transition-fast),
               box-shadow var(--transition-fast),color var(--transition-fast);
  }
  .fl-item:hover {
    background:var(--bg-overlay);
    border-color:var(--border);
    box-shadow:0 1px 4px rgba(0,0,0,0.15);
    color:var(--text-primary);
  }
  .fl-sel {
    background:var(--accent-subtle)!important;
    border-color:color-mix(in srgb,var(--accent) 55%,transparent)!important;
    color:var(--accent)!important;
  }
  .fl-name { flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--text-secondary); }
  .fl-path { font-size:10px;color:var(--text-disabled);max-width:70px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  .fl-stats { display:flex;gap:4px;flex-shrink:0; }

  /* File status badge */
  .fs-badge { display:inline-flex;align-items:center;justify-content:center;width:16px;height:16px;border-radius: var(--radius-sm);font-size:9px;font-weight:800;flex-shrink:0; }
  .fs-a { background:color-mix(in srgb, var(--success) 20%, var(--bg-base)); color:var(--success); }
  .fs-d { background:color-mix(in srgb, var(--error) 20%, var(--bg-base));   color:var(--error); }
  .fs-r { background:color-mix(in srgb, var(--warning) 20%, var(--bg-base)); color:var(--warning); }
  .fs-m { background:color-mix(in srgb,var(--accent) 15%,transparent);color:var(--accent); }

  /* add/del text colors */
  .add-txt { color:var(--success);font-weight:600;font-size:11px;font-family:var(--font-code); }
  .del-txt { color:var(--error);font-weight:600;font-size:11px;font-family:var(--font-code); }

  /* ── Diff viewer ──────────────────────────────────────────────────────── */
  .diff-hdr {
    display:flex;align-items:center;gap:8px;padding:6px 12px;
    border-bottom:1px solid var(--border-subtle);flex-shrink:0;
    background:var(--bg-overlay);font-size:11px;
  }
  .diff-fname { flex:1;min-width:0;font-family:var(--font-code);font-size:11px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }

  .patch-wrap {
    flex:1;overflow:auto;
    font-family:var(--font-code);font-size:11px;line-height:1.5;
  }
  .dl { display:flex;min-width:max-content; }
  .dl-gutter { width:14px;min-width:14px;text-align:center;color:var(--text-disabled);user-select:none; }
  .dl-text   { flex:1;padding:0 6px;white-space:pre; }

  .dl-add  { background:color-mix(in srgb,var(--success) 10%,transparent); }
  .dl-add .dl-gutter { color:var(--success); }
  .dl-add .dl-text   { color:var(--success); }
  .dl-del  { background:color-mix(in srgb,var(--error) 10%,transparent); }
  .dl-del .dl-gutter { color:var(--error); }
  .dl-del .dl-text   { color:var(--error); }
  .dl-hunk { background:color-mix(in srgb,var(--accent) 8%,transparent); }
  .dl-hunk .dl-text  { color:var(--accent);font-style:italic; }
  .dl-hunk .dl-gutter { color:transparent; }
  .dl-fhdr .dl-text  { color:var(--text-muted); }
  .dl-ctx .dl-text   { color:var(--text-secondary); }

  /* ── Commits tab specifics ────────────────────────────────────────────── */
  .cm-header {
    display:flex;align-items:center;gap:6px;padding:10px 12px;
    font-size:10px;font-weight:600;letter-spacing:0.06em;text-transform:uppercase;
    font-family:var(--font-ui-sans);color:var(--text-muted);
    border-bottom:1px solid var(--border-subtle);flex-shrink:0;
  }
  .cm-count {
    margin-left:auto;
    font-size:var(--font-size-xs);color:var(--text-disabled);
    background:var(--bg-overlay);border-radius:999px;padding:0 5px;
    line-height:16px;letter-spacing:0;text-transform:none;font-weight:500;
  }
  .cm-list {
    list-style:none;margin:0;overflow-y:auto;flex:1;
    padding:6px 8px;display:flex;flex-direction:column;gap:4px;
  }
  .cm-item {
    display:flex;align-items:flex-start;gap:8px;width:100%;padding:7px 10px;
    background:var(--bg-elevated);
    border:1px solid var(--border-subtle);
    border-radius:var(--radius-md);
    text-align:left;cursor:pointer;
    font-family:var(--font-ui-sans);
    transition:background var(--transition-fast),border-color var(--transition-fast),
               box-shadow var(--transition-fast),color var(--transition-fast);
  }
  .cm-item:hover {
    background:var(--bg-overlay);
    border-color:var(--border);
    box-shadow:0 1px 4px rgba(0,0,0,0.15);
  }
  .cm-sel {
    background:var(--accent-subtle)!important;
    border-color:color-mix(in srgb,var(--accent) 55%,transparent)!important;
  }
  .cm-sha { font-family:var(--font-code);font-size:10px;color:var(--accent);flex-shrink:0;background:color-mix(in srgb,var(--accent) 10%,transparent);padding:2px 5px;border-radius: var(--radius-sm);margin-top:1px; }
  .cm-body { flex:1;min-width:0;display:flex;flex-direction:column;gap:2px; }
  .cm-msg  { font-size:12px;color:var(--text-primary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis; }
  .cm-meta { font-size:10px;color:var(--text-muted); }

  .commit-right  { display:flex;flex-direction:column;overflow:hidden; }
  .cmt-files-panel { flex-shrink:0;max-height:220px;display:flex;flex-direction:column;overflow:hidden;border-bottom:1px solid var(--border-subtle); }
  .cmt-diff-panel  { flex:1;overflow:hidden;display:flex;flex-direction:column; }

  .cm-sha-lg  { font-family:var(--font-code);font-size:10px;color:var(--accent);background:color-mix(in srgb,var(--accent) 10%,transparent);padding:2px 5px;border-radius: var(--radius-sm);flex-shrink:0; }
  .cm-msg-sm  { flex:1;min-width:0;font-size:11px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }

  /* ── Footer ───────────────────────────────────────────────────────────── */
  .ftr-left  { display:flex;align-items:center;margin-right:auto; }
  .ftr-right { display:flex;align-items:center;gap:7px;position:relative; }
  .ftr-warn  { display:inline-flex;align-items:center;gap:5px;font-size:11px;font-weight:500;color:var(--warning); }
  .ftr-checking { color:var(--text-muted); }
  /* Auto-merge armed indicator — calmer than .ftr-warn (it's a status,
     not a blocker). Uses the accent colour to match the Wand2 brand
     used in the Create modal. */
  .ftr-armed {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    padding: 3px 8px;
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
    border-radius: 999px;
  }

  .ftr-check { display:flex;align-items:center;gap:5px;font-size:11px;color:var(--text-muted);cursor:pointer;user-select:none;white-space:nowrap; }
  .ftr-check:hover { color:var(--text-secondary); }
  .ftr-check input { width:13px;height:13px;accent-color:var(--accent);cursor:pointer;flex-shrink:0; }
  .ftr-div   { width:1px;height:18px;background:var(--border-subtle); }

  /* ── Buttons ──────────────────────────────────────────────────────────── */
  .btn { display:inline-flex;align-items:center;gap:5px;padding:5px 12px;font-size:12px;font-weight:500;border-radius:var(--radius-md);border:1px solid transparent;cursor:pointer;font-family:var(--font-ui-sans);transition:background var(--transition-fast),opacity var(--transition-fast);white-space:nowrap; }
  .btn:disabled { opacity:.45;cursor:default; }
  .btn-accent { background:var(--accent);color:var(--text-on-accent); }
  .btn-accent:hover:not(:disabled) { background:var(--accent-hover); }
  .btn-accent:disabled { opacity:.4; }
  .btn-ready  { background:transparent;border-color:color-mix(in srgb,var(--success) 40%,transparent);color:var(--success); }
  .btn-ready:hover:not(:disabled) { background:color-mix(in srgb,var(--success) 12%,transparent); }
  .btn-resolve {
    background:var(--warning);color:var(--text-on-accent);border-color:var(--warning);
    font-weight:600;padding:5px 14px;
  }
  .btn-resolve:hover:not(:disabled) {
    /* Slightly brightened warning hue — keeps the "amber action" feel
       across themes without baking in a literal hex. */
    background:color-mix(in srgb, var(--warning) 80%, white);
    border-color:color-mix(in srgb, var(--warning) 80%, white);
  }
  .btn-resolve:disabled { opacity:.55; }

  /* Conflict-resolution prep: live phase stepper rendered in place of the
     "Resolve Conflicts" button while the background job runs. Constrained
     width so the stepper doesn't elbow the merge buttons off the footer. */
  .resolve-progress {
    display:flex;
    align-items:center;
    min-width:0;
    /* Cap so a long phase detail (e.g. a long branch name) doesn't push
       the rest of the footer out of view; the inner labels truncate. */
    max-width:520px;
    padding:4px 10px;
    border:1px solid var(--border-subtle);
    border-radius:var(--radius-md);
    background:var(--bg-elevated);
  }

  /* ── Merge split button ───────────────────────────────────────────────── */
  .merge-split { display:flex;border-radius:var(--radius-md);overflow:hidden;border:1px solid var(--accent); }
  .merge-dis   { opacity:.5;pointer-events:none; }
  .ms-main { display:inline-flex;align-items:center;gap:5px;padding:5px 11px;font-size:12px;font-weight:500;background:var(--accent);color:var(--text-on-accent);border:none;cursor:pointer;font-family:var(--font-ui-sans);transition:background var(--transition-fast);white-space:nowrap; }
  .ms-main:hover:not(:disabled) { background:var(--accent-hover); }
  .ms-caret { display:inline-flex;align-items:center;justify-content:center;padding:5px 7px;background:var(--accent);color:rgba(255,255,255,.85);border:none;border-left:1px solid rgba(255,255,255,.2);cursor:pointer;transition:background var(--transition-fast); }
  .ms-caret:hover:not(:disabled) { background:var(--accent-hover); }

  .merge-dd { position:absolute;bottom:calc(100% + 6px);right:0;z-index:calc(var(--z-modal) + 2);background:var(--bg-overlay);border:1px solid var(--border);border-radius:var(--radius-md);padding:4px;min-width:220px;box-shadow:0 12px 40px rgba(0,0,0,.5);animation:dropUp 120ms cubic-bezier(.16,1,.3,1); }
  @keyframes dropUp { from{opacity:0;transform:translateY(6px)} to{opacity:1;transform:translateY(0)} }
  .md-opt { display:flex;flex-direction:column;align-items:flex-start;width:100%;padding:8px 11px;background:transparent;border:none;border-radius:var(--radius-sm);cursor:pointer;font-family:var(--font-ui-sans);transition:background var(--transition-fast);text-align:left;gap:2px; }
  .md-opt:hover { background:var(--bg-hover); }
  .md-opt strong { font-size:12px;color:var(--text-primary);font-weight:600; }
  .md-opt span   { font-size:11px;color:var(--text-muted); }


  /* ── CI tab ─────────────────────────────────────────────────────────────── */
  .ci-pane {
    flex:1;overflow-y:auto;overflow-x:hidden;
    display:flex;flex-direction:column;gap:8px;padding:10px 12px 14px;
  }

  .ci-hdr {
    display:flex;align-items:center;gap:8px;
    padding:6px 10px;background:var(--bg-elevated);
    border:1px solid var(--border-subtle);border-radius:var(--radius-md);
    flex-shrink:0;
  }
  .ci-pbadge {
    display:inline-flex;align-items:center;gap:4px;
    font-size:11px;font-weight:600;padding:3px 8px;border-radius:99px;
  }
  .ci-pbadge-github { background:#24292f1a;color:var(--text-primary);border:1px solid #24292f33; }
  .ci-pbadge-gitlab { background:#fc6d2614;color:#fc6d26;border:1px solid #fc6d2640; }
  .ci-branch-hint {
    display:inline-flex;align-items:center;gap:4px;
    font-size:11px;color:var(--accent);font-family:var(--font-code);
    background:var(--accent-subtle);padding:2px 7px;border-radius: var(--radius-sm);
  }
  .ci-spacer { flex:1; }

  .ci-mini-btn {
    display:inline-flex;align-items:center;gap:4px;
    padding:4px 8px;background:var(--bg-overlay);
    border:1px solid var(--border-subtle);border-radius:var(--radius-sm);
    color:var(--text-secondary);font-size:11px;font-weight:500;cursor:pointer;
    text-decoration:none;font-family:var(--font-ui-sans);
    transition:background var(--transition-fast),color var(--transition-fast),border-color var(--transition-fast);
  }
  .ci-mini-btn:hover:not(:disabled) {
    background:var(--bg-hover);color:var(--text-primary);border-color:var(--border);
  }
  .ci-mini-btn:disabled { opacity:.45;cursor:default; }

  .ci-state {
    flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;
    gap:8px;padding:32px 16px;text-align:center;color:var(--text-muted);
  }
  .ci-state-title { margin:0;font-size:13px;font-weight:600;color:var(--text-primary); }
  .ci-state-hint  { margin:0;font-size:12px;color:var(--text-muted);line-height:1.5;max-width:340px; }
  .ci-state-hint code {
    font-family:var(--font-code);font-size:11px;background:var(--bg-overlay);
    padding:1px 5px;border-radius: var(--radius-sm);color:var(--text-secondary);
  }
  .ci-err { color:var(--error);font-family:var(--font-code);font-size:11px;word-break:break-word; }
  :global(.ci-state-icon)      { color:var(--text-muted); }
  :global(.ci-state-icon.ci-state-warn) { color:var(--error); }

  .ci-runs {
    list-style:none;padding:0;margin:0;
    display:flex;flex-direction:column;gap:6px;
  }

  .ci-card {
    display:flex;align-items:center;gap:10px;
    padding:9px 12px;background:var(--bg-elevated);
    border:1px solid var(--border-subtle);border-radius:var(--radius-md);
    cursor:pointer;
    transition:background var(--transition-fast),border-color var(--transition-fast),
               box-shadow var(--transition-fast);
  }
  .ci-card:hover {
    background:var(--bg-overlay);border-color:var(--border);
    box-shadow:0 1px 4px rgba(0,0,0,.18);
  }
  .ci-card-head { border-color:color-mix(in srgb,var(--accent) 55%,transparent); }
  .ci-card-head:hover { border-color:var(--accent); }

  .ci-card-l { display:flex;flex-direction:column;align-items:flex-start;gap:4px;flex-shrink:0;min-width:90px; }
  .ci-card-c { flex:1;min-width:0;display:flex;flex-direction:column;gap:3px; }
  .ci-card-r { display:flex;align-items:center;gap:4px;flex-shrink:0; }

  .ci-status {
    display:inline-flex;align-items:center;gap:4px;
    font-size:10px;font-weight:600;padding:2px 8px;border-radius:99px;white-space:nowrap;
  }
  .ci-st-success   { background:rgba(74,222,128,.15);color:var(--success);border:1px solid rgba(74,222,128,.3); }
  .ci-st-failed    { background:rgba(248,113,113,.15);color:var(--error);border:1px solid rgba(248,113,113,.3); }
  .ci-st-running   { background:var(--accent-subtle);color:var(--accent);border:1px solid var(--accent); }
  .ci-st-cancelled { background:rgba(120,120,120,.1);color:var(--text-muted);border:1px solid var(--border); }
  .ci-st-pending   { background:rgba(120,120,120,.08);color:var(--text-disabled);border:1px solid var(--border-subtle); }

  .ci-dur {
    display:inline-flex;align-items:center;gap:3px;
    font-size:10px;color:var(--text-muted);font-family:var(--font-code);
  }

  .ci-card-title { display:flex;align-items:center;gap:6px;min-width:0; }
  .ci-name {
    font-size:12px;font-weight:600;color:var(--text-primary);
    white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0;flex-shrink:1;
  }
  .ci-id { font-size:10px;font-family:var(--font-code);color:var(--text-disabled);flex-shrink:0; }
  .ci-head-pill {
    font-size:9px;font-weight:700;letter-spacing:.04em;
    padding:1px 6px;border-radius:99px;
    background:var(--accent);color:var(--text-on-accent);
    flex-shrink:0;
  }

  .ci-chips { display:flex;align-items:center;gap:5px;flex-wrap:wrap; }
  .ci-chip {
    display:inline-flex;align-items:center;gap:3px;
    font-size:10px;padding:1px 6px;border-radius: var(--radius-sm);
    background:var(--bg-overlay);color:var(--text-muted);
  }
  .ci-sha  { font-family:var(--font-code); }
  .ci-time { color:var(--text-disabled); }
</style>
