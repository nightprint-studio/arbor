<script lang="ts">
  /**
   * BennuExternalChangeModal — the decision when a file changed on disk **while you had
   * unsaved edits in it**.
   *
   * Two versions of the same file exist and neither can be thrown away silently, so this is
   * the one moment Bennu has to interrupt. It is not a `ConfirmModal` because there is no
   * "cancel" that resolves anything: the three answers are *keep mine*, *take theirs*, and
   * *not now* — and only the last one leaves the file in the state it is in.
   *
   * The clean-buffer case never reaches here: the project store adopts the new content
   * silently, because there is nothing to lose and nothing to decide (see
   * `checkExternalChanges`).
   *
   * ## Why "not now" is presentation state and not a store flag
   *
   * The conflict is a *fact about the file*, and clearing it in the store would re-arm
   * autosave into exactly the overwrite this whole mechanism exists to prevent. So deferring
   * is remembered **here**: the modal stops offering a path it has already shown, while the
   * tab keeps its badge so the file stays findable. A fresh external change re-stamps the
   * file and the store re-flags it, which clears the deferral by identity — the reason the
   * key is the path *plus* the buffer we saw it with.
   */
  import { FileWarning } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  /** Paths the user chose not to decide about yet. Cleared per path once it resolves. */
  let deferred = $state<Set<string>>(new Set());

  /** The conflict to present: the first flagged path the user hasn't deferred. */
  const path = $derived(projectStore.conflictedPaths.find((p) => !deferred.has(p)) ?? null);
  const name = $derived(path ? (path.split(/[\\/]/).pop() ?? path) : '');

  // A path that stopped being conflicted (resolved here, or the tab closed) must drop out of
  // `deferred`, or a later conflict on the same file would be silently swallowed.
  $effect(() => {
    const live = new Set(projectStore.conflictedPaths);
    if ([...deferred].every((p) => live.has(p))) return;
    deferred = new Set([...deferred].filter((p) => live.has(p)));
  });

  let busy = $state(false);

  async function run(action: (p: string) => Promise<void>) {
    if (!path || busy) return;
    busy = true;
    try {
      await action(path);
    } finally {
      busy = false;
    }
  }

  const takeDisk = () => run((p) => projectStore.resolveTakeDisk(p));
  const keepMine = () => run((p) => projectStore.resolveKeepMine(p));

  function defer() {
    if (!path) return;
    deferred = new Set([...deferred, path]);
  }

  function onKey(e: KeyboardEvent) {
    if (!path) return;
    // No Enter default: both real answers destroy one version, so neither may be the key
    // you hit by reflex to dismiss a dialog. Esc defers, which changes nothing.
    if (e.key === 'Escape') { e.preventDefault(); defer(); }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if path}
  <Modal onClose={defer} width="520px" ariaLabel="File changed on disk">
    {#snippet header()}
      <div class="bx-icon"><FileWarning size={20} /></div>
      <span class="modal-title">File changed on disk</span>
    {/snippet}

    <div class="bx-body">
      <p class="bx-message">
        <code>{name}</code> was changed outside Bennu, and you have unsaved edits in it.
      </p>
      <p class="bx-detail">
        Autosave is paused for this file so neither version is lost. Pick which one to keep —
        the other is discarded.
      </p>
      <p class="bx-path" title={path}>{path}</p>
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <Button variant="ghost" onclick={defer} disabled={busy} type="button">Not now</Button>
        <div class="bx-actions">
          <Button variant="secondary" onclick={takeDisk} disabled={busy} type="button">
            Reload from disk
          </Button>
          <Button
            variant="primary"
            color="var(--warning)"
            onclick={keepMine}
            disabled={busy}
            loading={busy}
            type="button"
          >
            Keep my edits
          </Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .bx-icon {
    display: inline-flex; align-items: center; justify-content: center;
    width: 30px; height: 30px; border-radius: 50%;
    background: var(--warning-subtle); color: var(--warning);
    flex-shrink: 0;
  }
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .bx-body { font-family: var(--font-ui-sans); color: var(--text-primary); }
  .bx-message { margin: 0; font-size: var(--font-size-sm); line-height: 1.5; }
  .bx-message code {
    font-family: var(--font-ui-mono); font-size: var(--font-size-sm);
    color: var(--text-primary);
  }
  .bx-detail {
    margin: 8px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.5;
  }
  .bx-path {
    margin: 12px 0 0; padding: 6px 8px;
    background: var(--bg-input); border-radius: var(--radius-sm);
    font-family: var(--font-ui-mono); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }

  .bx-actions { display: flex; gap: 8px; }
</style>
