<script lang="ts">
  /**
   * DeepLinkDisabledModal — informational notice shown when the dispatcher
   * refuses an `arbor://` URL because the feature is off (master kill-switch
   * OR per-action enable toggle).  No action options — just acknowledge and
   * close.  Mirrors the visual rhythm of the other deep-link modals so users
   * see it as part of the same family.
   *
   * The modal embeds the URL verbatim via `<UrlBlock>` so the user can sanity-
   * check what was just blocked (and copy it elsewhere if they want to act on
   * it manually after enabling the feature).
   */
  import { ShieldOff } from 'lucide-svelte';
  import Alert from './ui/Alert.svelte';
  import Button from './ui/Button.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import UrlBlock from './ui/UrlBlock.svelte';

  let {
    title,
    message,
    url,
    onClose,
  }: {
    title:   string;
    message: string;
    url:     string;
    onClose: () => void;
  } = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === 'Escape') { e.preventDefault(); onClose(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Modal {onClose} width="460px" ariaLabel="Deep Link Disabled">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldOff size={15} strokeWidth={2} />
      <span class="modal-title">Deep Link Blocked</span>
    </ModalHeader>
  {/snippet}

  <div class="dld-body">
    <Alert variant="warning" {title} text={message} />
    <UrlBlock label="Blocked URL" value={url} copyable />
  </div>

  {#snippet footer()}
    <Button variant="primary" onclick={onClose}>OK</Button>
  {/snippet}
</Modal>

<style>
  .dld-body {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
</style>
