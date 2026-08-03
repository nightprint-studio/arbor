<script lang="ts">
  /**
   * The one thing standing between an open transaction and its silent loss.
   *
   * Two ways out of a Picus session end a transaction the user never committed:
   * closing the window, and disconnecting. Both roll it back — the server does that
   * when the socket goes, whatever the client intended — so the only question is
   * whether the user finds out before or afterwards. This component makes it before.
   *
   * ## Why the window hook lives here and not in the shell
   *
   * `onCloseRequested` is registered once, next to the confirmation it raises and
   * next to the store that knows what is open. A close request is vetoed, the
   * question is asked, and the close is re-issued only once the rollbacks have run —
   * with `allowClose` so the second pass does not ask again.
   *
   * The disconnect path does not go through here: it is `connectionsStore.disconnect`
   * awaiting `txStore.confirmRelease`, which raises the same dialog. One dialog, two
   * callers, one wording rule — the alternative was two confirmations that would
   * drift apart the first time either was edited.
   */
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { txStore } from '$lib/stores/picus/tx.svelte';

  const guard = $derived(txStore.guard);

  /** "Payroll (failed)" — the name the sidebar shows, plus the state if it is not
   *  the ordinary one. */
  const affected = $derived(
    (guard?.ids ?? []).map((id) => {
      const name = connectionsStore.byId(id)?.name ?? id;
      return txStore.snapshot(id).state === 'failed' ? `${name} — failed transaction` : name;
    }),
  );

  const title = $derived(
    guard?.scope === 'window'
      ? 'Close Picus with an open transaction?'
      : 'Disconnect with an open transaction?',
  );

  const message = $derived(
    affected.length === 1
      ? 'This connection has uncommitted work. It will be rolled back — every statement in '
        + 'the transaction is undone, and nothing of it reaches the database.'
      : `${affected.length} connections have uncommitted work. All of it will be rolled back — `
        + 'every statement in those transactions is undone.',
  );

  /** The names, one per line — `ConfirmModal`'s detail preserves the breaks. */
  const detail = $derived(
    [...affected, '', 'Cancel to go back and commit first.'].join('\n'),
  );

  /**
   * Set for the close we issue ourselves, so the re-issued close is not vetoed a
   * second time and the user is not asked the same question twice.
   */
  let allowClose = false;

  $effect(() => {
    const win = getCurrentWindow();
    const registered = win.onCloseRequested(async (event) => {
      try {
        if (allowClose) return;
        const open = txStore.openConnectionIds;
        if (!open.length) return;
        // Vetoed first, asked second: the question is worth nothing if the window has
        // already gone by the time it is on screen.
        event.preventDefault();
        if (!(await txStore.confirmRelease(open, 'window'))) return;
        allowClose = true;
        await win.close();
      } catch (e) {
        // Whatever went wrong, the window must still close.
        //
        // Tauri awaits this handler and only closes if it settles without a veto,
        // so a rejection here does not merely skip the question — it leaves a
        // window that cannot be closed at all, by any means short of killing the
        // process. Losing an unasked-about transaction is bad; trapping the user in
        // the window is worse, and the server rolls that transaction back anyway.
        console.error('picus: the transaction guard failed — closing anyway', e);
        allowClose = true;
        void win.close();
      }
    });
    return () => void registered.then((unlisten) => unlisten()).catch(() => {});
  });
</script>

{#if guard}
  <ConfirmModal
    {title}
    {message}
    {detail}
    variant="warning"
    confirmLabel={guard.scope === 'window' ? 'Roll back and close' : 'Roll back and disconnect'}
    cancelLabel="Keep the transaction"
    busy={txStore.releasing}
    onConfirm={() => void txStore.confirmGuard()}
    onCancel={() => txStore.cancelGuard()}
  />
{/if}
