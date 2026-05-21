// Shared stash action helpers — apply / pop / drop.
//
// Hand-rolled bring-your-own-tab-id wrappers around the stash IPC calls,
// with the post-action plumbing (status refresh, conflict modal, toast,
// graph refresh) consolidated so consumers don't all reimplement it.
// Used by StashList (sidebar), CommitGraph (hover actions on the bubble)
// and CommitDetailPanel (toolbar in the stash header).
import { stashApply, stashPop, stashDrop } from '$lib/ipc/branch';
import { getStatus }   from '$lib/ipc/stage';
import { uiStore }     from '$lib/stores/ui.svelte';
import { graphStore }  from '$lib/stores/graph.svelte';
import { diffStore }   from '$lib/stores/diff.svelte';
import { repoStore }   from '$lib/stores/repo.svelte';
import { applyPostStashChange } from '$lib/utils/applyPostStashChange';
import type { StashEntry, StashApplyResult } from '$lib/types/git';

async function handleApplyResult(
  tabId: string,
  result: StashApplyResult,
  stash: StashEntry,
  isPop: boolean,
  onRefresh?: () => void,
) {
  if (result.blocking_untracked.length > 0) {
    uiStore.openStashConflictModal(stash, [], result.blocking_untracked, isPop);
    return;
  }
  const s = await getStatus(tabId);
  repoStore.setStatus(s);
  const conflictPaths = result.conflicted_files.length > 0
    ? result.conflicted_files
    : s.conflicted.map(f => f.path);
  if (result.has_conflicts && conflictPaths.length > 0) {
    uiStore.openStashConflictModal(stash, conflictPaths);
    uiStore.showToast(
      `Stash applied with ${conflictPaths.length} conflict${conflictPaths.length === 1 ? '' : 's'} — resolution required`,
      'warning',
    );
  } else {
    // Light refresh — stash op doesn't change graph topology, only the
    // stash list + working dir state.  Skips the costly getGraph.
    await applyPostStashChange(tabId);
    uiStore.setActiveBottomSection('stage');
    if (result.no_changes) {
      uiStore.showToast(
        isPop
          ? 'No changes — working tree already matches the stash. Stash dropped.'
          : 'No changes — working tree already matches the stash.',
        'info',
      );
    } else {
      uiStore.showToast(isPop ? 'Stash applied and dropped' : 'Stash applied', 'success');
    }
    onRefresh?.();
  }
}

export async function applyStashAction(tabId: string, stash: StashEntry, onRefresh?: () => void) {
  try {
    const result = await stashApply(tabId, stash.index);
    await handleApplyResult(tabId, result, stash, false, onRefresh);
  } catch (err) {
    uiStore.showToast(`${err}`, 'error');
  }
}

export async function popStashAction(tabId: string, stash: StashEntry, onRefresh?: () => void) {
  try {
    const result = await stashPop(tabId, stash.index);
    await handleApplyResult(tabId, result, stash, true, onRefresh);
  } catch (err) {
    uiStore.showToast(`${err}`, 'error');
  }
}

export async function dropStashAction(tabId: string, stash: StashEntry, onRefresh?: () => void) {
  try {
    await stashDrop(tabId, stash.index);
    uiStore.showToast('Stash dropped', 'success');
    if (graphStore.selectedStash?.index === stash.index) {
      graphStore.setSelectedStash(null);
      diffStore.clear();
    }
    // Repaint markers + sidebar list without reloading the whole graph.
    await applyPostStashChange(tabId);
    onRefresh?.();
  } catch (err) {
    uiStore.showToast(`${err}`, 'error');
  }
}
