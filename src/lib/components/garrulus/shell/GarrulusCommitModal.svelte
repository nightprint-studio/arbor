<script lang="ts">
  /**
   * "Commit with a message" — the one dropdown entry that needs a word from the
   * user before it acts.
   *
   * Garrulus writes its own commit messages (`Aggiornate 3 note`), which is the
   * right default for a note vault: a history of *when you were working* reads
   * better than a history of nothing. This is the escape hatch for the change
   * that deserves a sentence, and it runs the same sync the button's main half
   * runs — only the message differs.
   */
  import { MessageSquare } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { garrulusSyncStore } from '$lib/stores/garrulus/sync.svelte';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let message = $state('');
  let field = $state<HTMLInputElement | undefined>();

  const canSubmit = $derived(message.trim().length > 0 && !garrulusSyncStore.busy);

  // No `autofocus` attribute (a11y): the field is focused once it exists.
  $effect(() => {
    field?.focus();
  });

  function submit() {
    if (!canSubmit) return;
    void garrulusSyncStore.syncNow(message.trim());
    onClose();
  }

  /** Enter and Ctrl/Cmd+Enter both submit — one field, so they mean the same
   *  thing, and the modal convention (Ctrl+Enter) has to work regardless. */
  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    submit();
    e.preventDefault();
  }
</script>

<Modal {onClose} width="520px" height="270px" ariaLabel="Commit message">
  {#snippet header()}
    <ModalHeader {onClose}>
      <MessageSquare size={14} />
      <span class="modal-title">Commit with a message</span>
    </ModalHeader>
  {/snippet}

  <div class="gcm-body">
    <FormField
      label="Message"
      hint="Replaces the generated one for this sync only. Enter to go, Esc to cancel."
    >
      <Input
        bind:value={message}
        bind:element={field}
        placeholder="What changed, in one line"
        onkeydown={onKeyDown}
      />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!canSubmit} onclick={submit}>Commit and sync</Button>
  {/snippet}
</Modal>

<style>
  .gcm-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
