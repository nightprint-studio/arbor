// pullResultHandler.ts — single source of truth for reacting to a PullResult.
//
// The backend returns `Ok(PullResult)` even when the pull itself failed as
// long as there's useful recovery context (pre-pull stash, conflicts). That
// shape is good for the detailed UI in RepoActions — which opens the stash
// conflict modal — but it's a footgun for any caller that treats a resolved
// promise as success. CommandPalette was doing exactly that and greeting the
// user with a "Pulled" toast over a failed pull.
//
// Every caller should route the result through `handlePullResult`. The
// helper pushes persistent notifications / opens the conflict modal as
// needed, and returns `true` only on a fully clean pull.

import type { PullResult, RepoStatus, StashEntry } from '$lib/types/git';
import { notificationsStore } from '$lib/stores/notifications.svelte';
import { uiStore } from '$lib/stores/ui.svelte';

export interface PullResultContext {
  /** Human-readable source label for toasts, e.g. `origin` or `upstream`. */
  remoteLabel?: string;
  /** Files currently conflicted in the workdir, as a fallback when
   *  `pre_pull_stash` is present but `stash_conflicts` is empty (some
   *  edge cases). Typically `status.conflicted.map(f => f.path)`. */
  workdirConflicts?: string[];
  /** Repo status fetched right after the pull.  When present, a `pull_error`
   *  produced while a merge / cherry-pick / rebase is in progress is routed
   *  to the conflict resolution modal instead of a plain toast. */
  status?: RepoStatus;
}

/**
 * Interpret a PullResult, routing the user to the right recovery UI.
 * Returns true on a clean pull — caller typically shows a success toast.
 */
export function handlePullResult(
  result: PullResult,
  ctx: PullResultContext = {},
): boolean {
  // ── stash re-apply failed for a non-conflict reason (file lock, antivirus) ──
  if (result.stash_apply_error && result.pre_pull_stash) {
    notificationsStore.add(
      'Pull — stash not re-applied automatically',
      `The pull completed, but your pre-pull stash is still present (stash@{0}): ${result.stash_apply_error}. ` +
      `Your changes are safe — apply them manually from the Stash panel.`,
      'error',
    );
    if (result.pull_error) {
      notificationsStore.add('Pull failed', result.pull_error, 'error');
    }
    return false;
  }

  // ── stash re-apply produced conflicts ────────────────────────────────────
  const conflictPaths = result.stash_conflicts.length > 0
    ? result.stash_conflicts
    : (ctx.workdirConflicts ?? []);
  if (result.pre_pull_stash && conflictPaths.length > 0) {
    const stash: StashEntry = result.pre_pull_stash;
    uiStore.openStashConflictModal(stash, conflictPaths);
    notificationsStore.add(
      'Pull — stash conflicts',
      `Pull completed. Resolve ${conflictPaths.length} stash conflict${conflictPaths.length === 1 ? '' : 's'}. ` +
      `Your changes remain safe in stash@{0} until you close the resolution.`,
      'warning',
    );
    if (result.pull_error) {
      notificationsStore.add('Pull failed', result.pull_error, 'error');
    }
    return false;
  }

  // ── pull itself failed (stash, if any, was cleanly restored) ─────────────
  if (result.pull_error) {
    // If the repo is currently mid-op (merge / cherry-pick / rebase / orphan
    // conflict files), the pull error is almost certainly that blockage
    // talking.  A plain toast leaves the user stranded — push them into the
    // resolution modal where they can finish or abort the operation.
    const st = ctx.status;
    const opInProgress =
      !!st && (st.is_merging || st.is_cherry_picking || st.is_rebasing || st.is_reverting);
    const hasConflictFiles = (st?.conflicted.length ?? 0) > 0;
    if (opInProgress || hasConflictFiles) {
      const label =
        st?.is_merging        ? 'merge' :
        st?.is_cherry_picking ? 'cherry-pick' :
        st?.is_rebasing       ? 'rebase' :
        st?.is_reverting      ? 'revert' :
                                'resolution';
      uiStore.openMergeModal();
      notificationsStore.add(
        'Pull blocked by in-progress operation',
        `The repo has a ${label} in progress. ` +
        `Complete or abort the resolution from the window just opened, then retry the pull.\n\n` +
        result.pull_error,
        'warning',
      );
      return false;
    }
    const suffix = ctx.remoteLabel ? ` (${ctx.remoteLabel})` : '';
    uiStore.showToast(`Pull failed${suffix}: ${result.pull_error}`, 'error');
    return false;
  }

  return true;
}

/**
 * Fallback for the `catch` branch of a pull flow.  The backend rejects the
 * IPC call (throwing on the JS side) when a pull fails with no stash
 * context — which means `handlePullResult` never runs and the user lands
 * on a toast they can't act on.  This helper probes the repo status and,
 * if a merge-like op is in progress, pushes the user into the conflict
 * modal instead.  Returns true when the error has been routed (caller
 * should skip the default toast); false otherwise.
 */
export function handlePullThrown(
  error: unknown,
  status: RepoStatus | null,
  ctx: Pick<PullResultContext, 'remoteLabel'> = {},
): boolean {
  if (!status) return false;
  const opInProgress =
    status.is_merging || status.is_cherry_picking || status.is_rebasing || status.is_reverting;
  const hasConflictFiles = status.conflicted.length > 0;
  if (!opInProgress && !hasConflictFiles) return false;

  const label =
    status.is_merging        ? 'merge' :
    status.is_cherry_picking ? 'cherry-pick' :
    status.is_rebasing       ? 'rebase' :
    status.is_reverting      ? 'revert' :
                               'resolution';
  const suffix = ctx.remoteLabel ? ` (${ctx.remoteLabel})` : '';
  uiStore.openMergeModal();
  notificationsStore.add(
    `Pull blocked by in-progress ${label}${suffix}`,
    `Complete or abort the resolution from the window just opened, then retry the pull.\n\n${error}`,
    'warning',
  );
  return true;
}
