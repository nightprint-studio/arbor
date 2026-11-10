<script lang="ts">
  /**
   * Imber › Actions — everything Imber changed in this repository, newest first, and what can be taken back.
   *
   * Local changes can be undone, newest first: a commit (HEAD moves back, the changes stay staged), a branch
   * created, switched to or deleted, a stage, a merge or pull (a hard reset, after a recovery snapshot). What
   * left the machine cannot: a push, a merge request, a plan run are listed with the reason, so the list is
   * honest about what undo does not cover.
   *
   * Keyboard: `U` undoes the newest reversible action, `Esc` closes.
   */
  import { Undo2 } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { imberStore as store } from '$lib/stores/corvus/imber.svelte';
  import { relativeTime } from '$lib/utils/imber-schedule';

  let { onClose }: { onClose: () => void } = $props();

  const actions = $derived(store.actions);
  const newest = $derived(actions.find((a) => a.undo !== null && a.undone_ms === null) ?? null);
  const now = Date.now();
  let busy = $state(false);

  onMount(() => { void store.loadActions(); });

  async function undo(id?: string) {
    if (busy) return;
    busy = true;
    try { await store.undo(id); } finally { busy = false; }
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.key === 'u' || e.key === 'U') && !e.ctrlKey && !e.metaKey && !e.altKey && newest) { e.preventDefault(); void undo(newest.id); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal {onClose} width="640px" height="min(520px, 86vh)" ariaLabel="Imber actions">
  {#snippet header()}
    <ModalHeader onClose={onClose}>
      <Undo2 size={14} class="header-icon" />
      <span class="modal-title">Imber › Actions</span>
    </ModalHeader>
  {/snippet}

  {#if actions.length === 0}
    <EmptyState message="Imber has not changed anything here yet" description="Commits, branches, merges and pushes it makes are listed here, with what can be undone." />
  {:else}
    <div class="list">
      {#each actions as a (a.id)}
        <div class="row" class:undone={a.undone_ms !== null}>
          <div class="main">
            <span class="summary">{a.summary}</span>
            <span class="when">{relativeTime(a.at_ms, now)}</span>
            {#if a.undone_ms !== null}
              <Badge variant="tone" tone="neutral" size="sm" label="undone" />
            {:else if a.undo !== null}
              <Badge variant="tone" tone="success" size="sm" label="can be undone" />
            {:else}
              <Badge variant="tone" tone="warning" size="sm" label="cannot be undone" tooltip={a.irreversible ?? ''} />
            {/if}
          </div>
          {#if a.undone_ms === null && a.undo === null && a.irreversible}
            <p class="why">{a.irreversible}</p>
          {/if}
          {#if a.undo !== null && a.undone_ms === null}
            <Button
              variant="outline"
              size="xs"
              disabled={busy || newest?.id !== a.id}
              tooltip={newest?.id === a.id ? 'Undo this action (U)' : 'A later action has to be undone first'}
              onclick={() => void undo(a.id)}
            ><Undo2 size={11} /> Undo</Button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter hint="Only local changes can be undone · a push, a merge request or a plan run cannot">
      <Button variant="ghost" onclick={onClose}>Close</Button>
      <Button variant="primary" disabled={!newest || busy} loading={busy} onclick={() => void undo()}><Undo2 size={12} /> Undo last action</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .list { display: flex; flex-direction: column; gap: 6px; }
  .row {
    display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; padding: 8px 10px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-overlay);
  }
  .row.undone .summary { text-decoration: line-through; color: var(--text-muted); }
  .main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .summary { font-size: var(--font-size-sm); overflow-wrap: anywhere; }
  .when { font-size: var(--font-size-xs); color: var(--text-muted); }
  .why { flex-basis: 100%; margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
