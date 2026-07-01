<script lang="ts">
  import { AlertTriangle } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';

  let {
    target,
    onConfirm,
    onCancel,
  }: {
    /** Human-readable label: e.g. "config.toml" or "all 5 files" */
    target: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

<Modal onClose={onCancel} ariaLabel="Discard Changes">
  {#snippet header()}
    <ModalHeader title="Discard Changes" onClose={onCancel} />
  {/snippet}
  <div class="body">
    <div class="warning-icon">
      <AlertTriangle size={32} />
    </div>
    <p class="message">
      You are about to discard changes to <strong>{target}</strong>.
    </p>
    <p class="irreversible">This action is irreversible — discarded changes cannot be recovered.</p>
    <div class="actions">
      <button class="btn cancel" onclick={onCancel}>Cancel</button>
      <button class="btn discard" onclick={onConfirm}>Discard</button>
    </div>
  </div>
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
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
    justify-content: center;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 18px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }

  .btn.cancel {
    background: transparent;
    color: var(--text-secondary);
  }
  .btn.cancel:hover { background: var(--bg-hover); color: var(--text-primary); }

  .btn.discard {
    background: var(--error-subtle);
    color: var(--error);
    border-color: color-mix(in srgb, var(--error) 35%, transparent);
  }
  .btn.discard:hover { background: color-mix(in srgb, var(--error) 28%, transparent); border-color: color-mix(in srgb, var(--error) 55%, transparent); }
</style>
