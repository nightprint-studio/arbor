<script lang="ts">
  import { GitBranch, AlertTriangle } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { checkoutBranchSafe } from '$lib/ipc/branch';
  import { applyPostCheckout } from '$lib/utils/applyPostCheckout';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import Button from './ui/Button.svelte';

  let loading = $state(false);

  const tabId  = $derived(uiStore.checkoutConflictTabId ?? '');
  const branch = $derived(uiStore.checkoutConflictBranch ?? '');

  function close() {
    uiStore.closeCheckoutConflictModal();
  }

  /** Decide whether the stash-apply error toast should mention "checkout" or
   *  "stash re-apply" — the backend now reuses the same field for both, so
   *  we pattern-match on the prefix it adds. Takes `branchName` as a param
   *  because the caller has snapshotted it; the reactive `branch` may have
   *  already cleared by the time we format the toast. */
  function describeError(msg: string, branchName: string): string {
    if (msg.startsWith('checkout failed')) {
      return `Could not switch to '${branchName}': ${msg.replace(/^checkout failed:\s*/, '')}. Your changes are preserved in the Stash panel.`;
    }
    return `Checked out '${branchName}' — stash re-apply failed: ${msg}. Your changes are safe in the Stash panel.`;
  }

  async function handleStashAndCheckout() {
    if (loading) return;
    loading = true;
    // Snapshot reactive values BEFORE closing the modal.  `tabId` and
    // `branch` are `$derived` from `uiStore.checkoutConflict*`; the close
    // call clears those store fields, so subsequent uses of the deriveds
    // would read an empty string and the IPC would fail with
    // `RepoNotOpen('')` — which the user sees as a confusing "tab not
    // open" toast AND, since applyPostCheckout never reaches the backend,
    // the abnormal-state alert in repoStore.setStatus never fires (the
    // conflict state is "invisible live").
    const localTabId  = tabId;
    const localBranch = branch;
    try {
      const result = await checkoutBranchSafe(localTabId, localBranch);
      uiStore.closeCheckoutConflictModal();
      // Light refresh — checkout only moves HEAD; skip the costly getGraph.
      await applyPostCheckout(localTabId);

      if (result.stash_apply_error) {
        uiStore.showToast(describeError(result.stash_apply_error, localBranch), 'warning', 7000);
      } else if (result.stash_conflicts.length > 0 && result.pre_checkout_stash) {
        uiStore.openStashConflictModal(result.pre_checkout_stash, result.stash_conflicts);
        uiStore.showToast(
          `Checked out '${localBranch}' — stash re-applied with ${result.stash_conflicts.length} conflict${result.stash_conflicts.length === 1 ? '' : 's'}`,
          'warning'
        );
      } else {
        uiStore.showToast(`Checked out '${localBranch}'`, 'success');
      }
    } catch (err) {
      // Even on backend error the operation may have left the workdir in a
      // partial state (e.g. libgit2 wrote some files before bailing, or a
      // hook side-effect kicked in).  Refresh status so the UI surfaces
      // whatever the repo actually looks like now instead of leaving the
      // user blind behind a generic error toast.
      uiStore.closeCheckoutConflictModal();
      await applyPostCheckout(localTabId).catch(() => { /* refresh best-effort */ });
      const conflicted = repoStore.status?.conflicted ?? [];
      if (conflicted.length > 0) {
        // We don't know the exact stash entry that caused the conflicts
        // here — pass index 0 (the most recent stash) which is what
        // checkout_branch_safe would have preserved.
        uiStore.openStashConflictModal(
          { index: 0, message: 'pre-checkout stash', oid: '' },
          conflicted.map(c => c.path),
        );
        uiStore.showToast(
          `Checkout produced ${conflicted.length} conflict${conflicted.length === 1 ? '' : 's'} — resolve in Stage. (${err})`,
          'warning',
          8000,
        );
      } else {
        uiStore.showToast(`Checkout failed: ${err}`, 'error');
      }
    } finally {
      loading = false;
    }
  }

  function handleEnter(e: KeyboardEvent) {
    if (e.key === 'Enter' && !loading) handleStashAndCheckout();
  }
</script>

<svelte:window onkeydown={handleEnter} />

<Modal onClose={close} ariaLabel="Uncommitted changes">
  {#snippet header()}
    <ModalHeader onClose={close}>
      <span class="warn-icon"><AlertTriangle size={14} /></span>
      <span class="modal-title">Uncommitted changes</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    <p>
      Your local changes prevent switching to
      <span class="branch-chip"><GitBranch size={11} />{branch}</span>.
    </p>
    <p class="hint">
      Arbor will stash your changes, check out the branch, then reapply them.
      The stash is always preserved — no changes are discarded.
    </p>
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={close} disabled={loading}>Cancel</Button>
    <Button
      variant="primary"
      onclick={handleStashAndCheckout}
      disabled={loading}
      loading={loading}
    >
      {loading ? 'Switching…' : 'Stash & checkout'}
    </Button>
  {/snippet}
</Modal>

<style>
  .warn-icon {
    display: inline-flex;
    align-items: center;
    color: var(--warning);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  p {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.5;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
  }

  .branch-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--accent);
  }

  .hint {
    color: var(--text-muted);
    font-size: 11px;
  }

</style>
