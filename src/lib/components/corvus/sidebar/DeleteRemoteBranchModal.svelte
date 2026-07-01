<script lang="ts">
  import { AlertTriangle } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  let {
    branchName,
    onConfirm,
    onCancel,
  }: {
    branchName: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

<Modal onClose={onCancel} ariaLabel="Delete Remote Branch">
  {#snippet header()}
    <ModalHeader title="Delete Remote Branch" onClose={onCancel} />
  {/snippet}
  <div class="body">
    <div class="warning-icon">
      <AlertTriangle size={32} />
    </div>
    <p class="message">
      You are about to delete <strong>{branchName}</strong> from the remote.
    </p>
    <p class="irreversible">
      This action is irreversible — the branch will be removed from the remote
      and cannot be recovered unless someone still has it locally.
    </p>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onCancel}>Cancel</Button>
    <Button variant="danger" onclick={onConfirm}>Delete from remote</Button>
  {/snippet}
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    text-align: center;
    padding: 8px 4px 4px;
  }

  .warning-icon {
    color: var(--warning);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .message {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    margin: 0;
    line-height: 1.5;
  }

  .message strong {
    font-weight: 600;
    font-family: var(--font-code);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
  }

  .irreversible {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
    max-width: 320px;
  }
</style>
