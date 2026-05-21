<script lang="ts">
  /**
   * DeepLinkActionConfirmModal — generic "Are you sure?" gate that fires
   * BEFORE the dispatcher does any work for an `arbor://…` URL.
   *
   * Distinct from `DeepLinkConfirmModal`:
   *   * THIS modal asks *whether to proceed at all* and is opt-out per-action
   *     in Settings → Tools → Deep Links → Confirmations.
   *   * `DeepLinkConfirmModal` only appears when a clone is also required
   *     (and is mandatory regardless of these toggles, since it needs the
   *     destination folder).
   *
   * On confirm the dispatcher resumes the action (which may then trigger
   * the clone-confirm modal if the local copy is missing).
   */
  import { Link2 } from 'lucide-svelte';
  import Alert from './ui/Alert.svelte';
  import Button from './ui/Button.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import UrlBlock from './ui/UrlBlock.svelte';

  let {
    title,
    description,
    url,
    onConfirm,
    onCancel,
  }: {
    /** One-line summary of the action ("Open commit abc1234"). */
    title:       string;
    /** Optional longer paragraph explaining what will happen. */
    description?: string | null;
    /** The git remote URL the link references. */
    url:         string;
    onConfirm:   () => void;
    onCancel:    () => void;
  } = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); onConfirm(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal onClose={onCancel} width="460px" ariaLabel="Confirm Deep Link Action">
  {#snippet header()}
    <ModalHeader onClose={onCancel}>
      <Link2 size={15} strokeWidth={2} />
      <span class="modal-title">Confirm Action</span>
    </ModalHeader>
  {/snippet}

  <div class="dlc-body">
    <Alert variant="info" {title} text={description ?? undefined} />
    <UrlBlock label="Repository" value={url} />
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onCancel}>Cancel</Button>
    <Button variant="primary" onclick={onConfirm}>Continue</Button>
  {/snippet}
</Modal>

<style>
  .dlc-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
</style>
