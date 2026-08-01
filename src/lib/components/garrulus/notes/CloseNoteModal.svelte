<script lang="ts">
  /**
   * "This note has changes that are not on disk" — asked when a tab is closed
   * with unsaved bytes.
   *
   * Not `ConfirmModal`, and the reason is the third button: the honest answer set
   * here is *save* / *discard* / *keep editing*, and a yes-no dialog has to drop
   * one of the two that matter. Made from `Modal` + `ModalHeader` +
   * `ModalFooter`, which is what CLAUDE.md says a custom dialog is made of.
   *
   * Save is the default and holds the focus: the destructive answer should cost a
   * deliberate move, and `Esc` means "I did not decide", which is *keep editing*.
   */
  import { onMount, tick } from 'svelte';
  import { AlertTriangle } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  interface Props {
    title: string;
    /** Vault-relative path, so two notes of the same name are told apart. */
    path: string;
    busy?: boolean;
    onSave: () => void;
    onDiscard: () => void;
    onCancel: () => void;
  }

  let { title, path, busy = false, onSave, onDiscard, onCancel }: Props = $props();

  let saveBtn = $state<HTMLButtonElement | undefined>(undefined);

  onMount(async () => {
    await tick();
    saveBtn?.focus();
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !busy) {
      e.preventDefault();
      onSave();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal onClose={onCancel} width="460px" ariaLabel="Close {title}">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <span class="cnm-icon"><AlertTriangle size={18} /></span>
      <span class="modal-title">Save before closing?</span>
    </ModalHeader>
  {/snippet}

  <div class="cnm-body">
    <p class="cnm-message">“{title}” has changes that are not on disk yet.</p>
    <p class="cnm-path">{path}</p>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={onDiscard} disabled={busy}>
        Close without saving
      </Button>
      <span class="cnm-right">
        <Button variant="secondary" onclick={onCancel} disabled={busy}>Keep editing</Button>
        <Button
          variant="primary"
          color="var(--warning)"
          onclick={onSave}
          loading={busy}
          disabled={busy}
          bind:element={saveBtn}
        >
          Save and close
        </Button>
      </span>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .cnm-icon {
    display: flex;
    color: var(--warning);
  }

  .cnm-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cnm-message {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: 1.5;
    color: var(--text-primary);
  }

  .cnm-path {
    margin: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }

  .cnm-right {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
</style>
