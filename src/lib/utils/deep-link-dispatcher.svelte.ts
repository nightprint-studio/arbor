/**
 * Deep-link dispatcher — parses `arbor://…` URLs and routes them to the
 * correct in-app action.  Wired by AppShell once on mount; receives URLs
 * via the `arbor://deep-link` Tauri event (forwarded from
 * `tauri-plugin-deep-link` through the cold-start buffer).
 *
 * Supported URLs:
 *   arbor://repo/open?url=<git-url>
 *   arbor://commit/<sha>?url=<git-url>
 *   arbor://branch/<name>?url=<git-url>&checkout=1
 *   arbor://branch/<name>?url=<git-url>&worktree=1
 *   arbor://mr/open/<number>?url=<git-url>
 *   arbor://pipeline/<run-id>?url=<git-url>
 *
 * `<git-url>` is the **remote** git URL (not a local path) — that's the
 * only piece of information that's safe to share between machines.  The
 * dispatcher uses fuzzy canonical-key matching to find the local clone,
 * applying the user's `cross_workspace_strategy` if the repo lives in a
 * workspace other than the active one.  When no clone exists, the user
 * gets a confirmation modal — refusing it aborts the whole action, no
 * partial state is left behind.
 */

import { openRepoFromUrl } from './openRepoFromUrl';
import { runPipeline, step } from './action-pipeline';
import { applyPostCheckout } from './applyPostCheckout';
import { graphStore } from '$lib/stores/graph.svelte';
import { tabsStore } from '$lib/stores/tabs.svelte';
import { uiStore } from '$lib/stores/ui.svelte';
import { diffStore } from '$lib/stores/diff.svelte';
import { cacheStore } from '$lib/stores/cache.svelte';
import { checkoutBranchSafe } from '$lib/ipc/branch';
import { handleCheckoutResult } from '$lib/utils/checkoutResultHandler';
import { getCommitDiff } from '$lib/ipc/diff';
import { getDeepLinkConfig } from '$lib/ipc/deep-link';
import type { ConfirmConfig, EnableConfig } from '$lib/types/deep-link';

// ---------------------------------------------------------------------------
// Parsed URL shapes
// ---------------------------------------------------------------------------

/** Discriminated union of every supported deep-link action. */
export type DeepLinkAction =
  | { kind: 'repo_open';       url: string }
  | { kind: 'commit_jump';     url: string; sha: string }
  | { kind: 'branch_checkout'; url: string; branch: string }
  | { kind: 'branch_worktree'; url: string; branch: string }
  | { kind: 'mr_open';         url: string; number: number }
  | { kind: 'pipeline_open';   url: string; runId: string };

/** State surface for the clone-confirm modal — drives an `{#if}` block in
 *  AppShell.  Set by the dispatcher when `openRepoFromUrl` returns
 *  `needs_clone`; cleared when the user confirms or cancels. */
export interface PendingClone {
  url:          string;
  description:  string;
  reason:       'unknown' | 'missing_on_disk';
  /** Continuation invoked after a successful clone. */
  resume:       (repoId: string) => Promise<void> | void;
}

/** State surface for the generic action-confirm modal.  Set by `dispatch`
 *  when the per-action confirm setting is on; cleared on accept/cancel. */
export interface PendingActionConfirm {
  /** Short title — e.g. "Open commit abc1234". */
  title:       string;
  /** Optional explanatory line (clipboard preview, side-effect note, …). */
  description: string;
  /** The git URL the action targets.  Shown verbatim in the modal so the
   *  user can sanity-check who's asking. */
  url:         string;
  onAccept:    () => void;
  onReject:    () => void;
}

/** State surface for the "Deep link disabled" notice modal.  Shown when
 *  the dispatcher refuses an action because the feature is off — either
 *  the master kill-switch or the per-action enable toggle. */
export interface PendingDisabled {
  /** Title — "Deep links are disabled" or "<Action kind> is disabled". */
  title:   string;
  /** Body paragraph pointing the user at Settings. */
  message: string;
  /** The deep-link URL that was just blocked. */
  url:     string;
  onClose: () => void;
}

/** Modal callbacks the dispatcher needs from AppShell.  Keeps the
 *  dispatcher decoupled from concrete component imports. */
export interface DeepLinkBindings {
  /** Open the AddWorktree modal pre-filled with `branch`. */
  openWorktreeModal: (tabId: string, branch: string) => void;
  /** Open the MR detail modal for the given MR number on `tabId`. */
  openMrDetail:      (tabId: string, number: number) => void;
  /** Open the CI pipeline detail modal for `runId` on `tabId`. */
  openCiDetail:      (tabId: string, runId: string) => void;
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/**
 * Parse an `arbor://…` URL into a typed action.  Returns `null` on every
 * malformed/unknown URL so the dispatcher can surface a single "Unsupported
 * deep link" toast without exploding.
 */
export function parseDeepLink(raw: string): DeepLinkAction | null {
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    return null;
  }
  if (parsed.protocol !== 'arbor:') return null;

  const url = parsed.searchParams.get('url')?.trim();
  if (!url) return null;

  // For `arbor://repo/open?url=…`:
  //   protocol = "arbor:", host = "repo", pathname = "/open"
  // For `arbor://commit/<sha>?url=…`:
  //   host = "commit", pathname = "/<sha>"
  const host    = parsed.host || parsed.hostname;
  const segs    = parsed.pathname.split('/').filter(Boolean);
  const flag    = (k: string) => parsed.searchParams.get(k);

  switch (host) {
    case 'repo':
      if (segs[0] === 'open') return { kind: 'repo_open', url };
      return null;

    case 'commit': {
      const sha = segs[0];
      if (!sha) return null;
      return { kind: 'commit_jump', url, sha };
    }

    case 'branch': {
      // segs[0] is the branch name (URL-encoded).  Slashes inside branch
      // names (e.g. `feature/foo`) should be percent-encoded by the link
      // author — we accept the pathname as a single segment and join back
      // anything past it for resilience to `feature/foo` written raw.
      if (segs.length === 0) return null;
      const branch = segs.map(decodeURIComponent).join('/');
      if (flag('worktree') === '1') return { kind: 'branch_worktree', url, branch };
      if (flag('checkout') === '1') return { kind: 'branch_checkout', url, branch };
      return null;
    }

    case 'mr': {
      // arbor://mr/open/<number>?url=…
      if (segs[0] !== 'open' || !segs[1]) return null;
      const n = Number(segs[1]);
      if (!Number.isFinite(n) || n <= 0) return null;
      return { kind: 'mr_open', url, number: n };
    }

    case 'pipeline': {
      const runId = segs[0];
      if (!runId) return null;
      return { kind: 'pipeline_open', url, runId };
    }

    default:
      return null;
  }
}

/** Short user-facing label for the action — fed into the clone-confirm
 *  modal's body so the user sees what they're about to consent to. */
export function describeAction(a: DeepLinkAction): string {
  switch (a.kind) {
    case 'repo_open':       return 'Open repository';
    case 'commit_jump':     return `Open commit ${a.sha.slice(0, 8)}`;
    case 'branch_checkout': return `Check out branch "${a.branch}"`;
    case 'branch_worktree': return `Create a worktree on "${a.branch}"`;
    case 'mr_open':         return `Open merge request !${a.number}`;
    case 'pipeline_open':   return `Open CI pipeline ${a.runId}`;
  }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/** Map an action to its confirm-config field name. */
function confirmKey(a: DeepLinkAction): keyof ConfirmConfig {
  switch (a.kind) {
    case 'repo_open':       return 'repo_open';
    case 'commit_jump':     return 'commit_jump';
    case 'branch_checkout': return 'branch_checkout';
    case 'branch_worktree': return 'branch_worktree';
    case 'mr_open':         return 'mr_open';
    case 'pipeline_open':   return 'pipeline_open';
  }
}

/** Map an action to its enable-config field name (same shape as confirm,
 *  but kept distinct so future per-feature divergence is trivial). */
function enableKey(a: DeepLinkAction): keyof EnableConfig {
  return confirmKey(a) as keyof EnableConfig;
}

/** Human label for an action kind — used in the "<kind> is disabled" notice. */
function actionKindLabel(a: DeepLinkAction): string {
  switch (a.kind) {
    case 'repo_open':       return 'Open-repository';
    case 'commit_jump':     return 'Jump-to-commit';
    case 'branch_checkout': return 'Branch-checkout';
    case 'branch_worktree': return 'Branch-worktree';
    case 'mr_open':         return 'Open-MR';
    case 'pipeline_open':   return 'Open-pipeline';
  }
}

/** Static description used in the action-confirm modal — a longer, more
 *  consequence-oriented sentence than `describeAction` (which is the title). */
function explainAction(a: DeepLinkAction): string {
  switch (a.kind) {
    case 'repo_open':
      return 'Arbor will open this repository, cloning it first if it isn\'t in your library.';
    case 'commit_jump':
      return 'Arbor will switch to the matching repository tab and scroll the graph to this commit.';
    case 'branch_checkout':
      return 'Arbor will run a stash-safe checkout of this branch on the matching repository — your uncommitted changes are stashed and re-applied automatically. Any conflicts surface in the Stage area.';
    case 'branch_worktree':
      return 'Arbor will open the "Add worktree" dialog pre-filled with this branch — you choose the destination folder before anything is created on disk.';
    case 'mr_open':
      return 'Arbor will fetch the merge request detail and open it in the MR modal.';
    case 'pipeline_open':
      return 'Arbor will open the CI pipeline detail modal for this run.';
  }
}

function createDispatcher() {
  let bindings: DeepLinkBindings | null = null;
  let pendingClone         = $state<PendingClone | null>(null);
  let pendingActionConfirm = $state<PendingActionConfirm | null>(null);
  let pendingDisabled      = $state<PendingDisabled | null>(null);

  function wire(b: DeepLinkBindings) { bindings = b; }

  /** Parse + route a single URL.  Logs and toasts on malformed input. */
  async function dispatch(rawUrl: string): Promise<void> {
    const parsed = parseDeepLink(rawUrl);
    if (!parsed) {
      console.warn('[deep-link] unrecognised URL:', rawUrl);
      uiStore.showToast(`Unsupported deep link: ${rawUrl}`, 'warning');
      return;
    }

    // Pull settings up-front so we can apply the worktree-rewrite transform
    // and gate enable / confirm off a single config snapshot.
    let cfg;
    try {
      cfg = await getDeepLinkConfig();
    } catch {
      // Config IPC failed (very rare).  Treat as default-disabled to err on
      // the side of safety — the user should never have a deep-link silently
      // execute because we couldn't read the config.
      pendingDisabled = {
        title:   'Deep links unavailable',
        message: 'Arbor couldn\'t read the deep-link configuration. Open Settings → Tools → Deep Links to verify the feature is set up.',
        url:     parsed.url,
        onClose: () => { pendingDisabled = null; },
      };
      return;
    }

    // Optionally rewrite checkout → worktree before any user-facing step,
    // so the confirm dialog describes what's actually going to happen.
    let action: DeepLinkAction = parsed;
    if (cfg.checkout_as_worktree && parsed.kind === 'branch_checkout') {
      action = { kind: 'branch_worktree', url: parsed.url, branch: parsed.branch };
    }

    // Master kill-switch — always wins.
    if (!cfg.enabled) {
      pendingDisabled = {
        title:   'Deep links are disabled',
        message: 'Arbor received an arbor:// link but the deep-link feature is turned off. Enable it in Settings → Tools → Deep Links to allow incoming links.',
        url:     action.url,
        onClose: () => { pendingDisabled = null; },
      };
      return;
    }

    // Per-action enable.  Even with the master on, each action kind is opt-in
    // — sharing a link should never silently mutate a workspace.
    if (!cfg.enable[enableKey(action)]) {
      pendingDisabled = {
        title:   `${actionKindLabel(action)} links are disabled`,
        message: `Arbor received a ${actionKindLabel(action).toLowerCase()} deep link but this action kind is turned off. Enable it in Settings → Tools → Deep Links → Enabled actions.`,
        url:     action.url,
        onClose: () => { pendingDisabled = null; },
      };
      return;
    }

    const needsConfirm = cfg.confirm[confirmKey(action)];
    if (!needsConfirm) {
      await runAction(action);
      return;
    }

    pendingActionConfirm = {
      title:       describeAction(action),
      description: explainAction(action),
      url:         action.url,
      onAccept: () => {
        pendingActionConfirm = null;
        void runAction(action);
      },
      onReject: () => {
        pendingActionConfirm = null;
      },
    };
  }

  async function runAction(action: DeepLinkAction): Promise<void> {
    const outcome = await openRepoFromUrl(action.url);

    if (outcome.kind === 'error') {
      uiStore.showToast(outcome.message, 'error');
      return;
    }

    if (outcome.kind === 'needs_clone') {
      // Stash the action behind the confirm modal — the resume continuation
      // re-enters runFollowUp() with the freshly-cloned repoId.
      pendingClone = {
        url:         action.url,
        description: describeAction(action),
        reason:      outcome.reason,
        resume:      async (repoId) => {
          // After a successful clone the new tab is already open & active
          // (CloneModal's submit path calls addTab + setActive).  Wait one
          // microtask so subscribers (graph etc.) have caught up before we
          // dispatch the follow-up action.
          await Promise.resolve();
          await runFollowUp(action, repoId);
        },
      };
      return;
    }

    // outcome.kind === 'opened' — the tab is active, run the follow-up.
    await runFollowUp(action, outcome.repoId);
  }

  /** Per-action follow-up assuming a tab for `repoId` is already active. */
  async function runFollowUp(action: DeepLinkAction, repoId: string): Promise<void> {
    if (!bindings) {
      console.warn('[deep-link] dispatcher not wired — follow-up dropped');
      return;
    }

    switch (action.kind) {
      case 'repo_open':
        // Nothing to do — opening + activating the tab IS the action.
        return;

      case 'commit_jump': {
        // Fast-path when the section is already what we want — Svelte's
        // $state skips the reactive update for same-value primitives, so
        // the ready effect would never fire and the awaiter would only
        // resolve via timeout.  Skip the whole subscribe/wait dance.
        const alreadyOnDetail = uiStore.activeBottomSection === 'detail';
        // The bottom panel pushes a "ready" signal to uiStore via Svelte
        // transition events (introend) AND a fallback effect that fires on
        // section swaps with no transition.  Subscribe BEFORE setting the
        // section so we never miss the signal.
        const ready = alreadyOnDetail
          ? Promise.resolve()
          : uiStore.awaitBottomPanelReady();
        // Likewise for the graph data: when the deep-link switched tab the
        // graph for the new tab loads asynchronously and `scrollToCommit`
        // would either fire against `data === null`, race the auto-scroll-
        // to-HEAD effect, or — most insidiously — see the PREVIOUS tab's
        // graphData (still in memory, not yet replaced) and let the scroll-
        // to-commit effect "fail to find" + clear the target.  Subscribe
        // BEFORE the pipeline starts; `awaitGraphLoaded` is tab-aware via
        // graphDataTabId so it only fast-resolves when the data really
        // belongs to `repoId`.
        const graphReady = graphStore.awaitGraphLoaded(repoId);
        await runPipeline([
          step('open-bottom-detail', () => uiStore.setActiveBottomSection('detail')),
          step('await-panel-ready',  () => ready),
          step('await-graph-loaded', () => graphReady),
          step('scroll-to-commit',   () => graphStore.scrollToCommit(action.sha)),
          // Mirror what `CommitGraph.handleSelectCommit` does after a click:
          // load the commit detail + file diff so the panel actually swaps
          // its content over.  Errors here surface as a toast — they don't
          // unwind the rest of the flow (the user is already on the commit).
          step('load-commit-detail', async () => {
            try {
              const [detail, files] = await Promise.all([
                cacheStore.loadCommitDetail(repoId, action.sha),
                getCommitDiff(repoId, action.sha),
              ]);
              graphStore.setDetail(detail);
              diffStore.setFiles(files);
            } catch (e) {
              uiStore.showToast(`Failed to load commit ${action.sha.slice(0, 8)}: ${e}`, 'error');
            }
          }),
        ]);
        return;
      }

      case 'branch_checkout': {
        // No-op when we're already on the target branch — checkoutBranchSafe
        // would otherwise stash + immediately re-apply the workdir for a
        // pointless side-trip, masking real changes the user is in the
        // middle of and (in dev mode) potentially racing the auto-reload
        // that follows any modified .rs files.
        const currentBranch = tabsStore.tabs.find(t => t.id === repoId)?.currentBranch;
        if (currentBranch === action.branch) {
          uiStore.showToast(`Already on ${action.branch}`, 'info');
          // Even when we don't checkout, focus on the branch is what the
          // user clicked the deep-link for.
          graphStore.scrollToBranch(action.branch);
          return;
        }
        try {
          const r = await checkoutBranchSafe(repoId, action.branch);
          await runPipeline([
            // Light post-checkout refresh — moves HEAD on the existing
            // graph nodes + reloads sidebar lists, no `getGraph` lane
            // re-assignment.  Awaited so the focus step below runs after
            // the new HEAD has landed in graphData (otherwise the badge
            // would still highlight the old HEAD's row briefly).
            step('apply-post-checkout', () => applyPostCheckout(repoId)),
            // The branch ref already lives on its commit node (existing
            // local branch checkout), so scroll-to-branch finds it
            // immediately.
            step('focus-branch',       () => graphStore.scrollToBranch(action.branch)),
          ]);
          handleCheckoutResult(r, {
            targetLabel:    action.branch,
            successMessage: `Checked out ${action.branch}`,
          });
        } catch (e) {
          uiStore.showToast(`Checkout failed: ${e}`, 'error');
        }
        return;
      }

      case 'branch_worktree':
        bindings.openWorktreeModal(repoId, action.branch);
        return;

      case 'mr_open':
        // The binding is responsible for loading detail + opening the modal —
        // the dispatcher doesn't know what shape of MR object the modal needs.
        bindings.openMrDetail(repoId, action.number);
        return;

      case 'pipeline_open':
        bindings.openCiDetail(repoId, action.runId);
        return;
    }
  }

  /** Called by the confirm modal after a successful clone. */
  function confirmClone(repoId: string): void {
    const p = pendingClone;
    pendingClone = null;
    if (!p) return;
    void p.resume(repoId);
  }

  function cancelClone(): void {
    pendingClone = null;
  }

  return {
    wire,
    dispatch,
    confirmClone,
    cancelClone,
    get pendingClone()         { return pendingClone; },
    get pendingActionConfirm() { return pendingActionConfirm; },
    get pendingDisabled()      { return pendingDisabled; },
  };
}

export const deepLinkDispatcher = createDispatcher();
