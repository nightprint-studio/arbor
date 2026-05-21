<script lang="ts">
  /**
   * DeepLinkLoadingModal — placeholder shown while a deep-link follow-up
   * fetches the data needed to render its real modal (MR detail, CI run, …).
   *
   * Two states:
   *   * `loading` — spinner + caption. Stays open until the fetch resolves.
   *   * `error`   — Alert with the error/not-found message + a Close button.
   *
   * The intent is to give the user instant feedback that the click was
   * registered, and to surface failures (404, network) explicitly instead of
   * leaving them with a silent toast.
   */
  import { Loader2, AlertTriangle } from 'lucide-svelte';
  import Alert from './ui/Alert.svelte';
  import Button from './ui/Button.svelte';
  import Spinner from './ui/Spinner.svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';

  let {
    title,
    status,
    message,
    onClose,
  }: {
    /** Header text — keep tight (e.g. "Opening merge request !123"). */
    title:    string;
    status:   'loading' | 'error';
    /** Body text for the `error` state.  Ignored when `status === 'loading'`. */
    message?: string;
    onClose:  () => void;
  } = $props();
</script>

<Modal onClose={onClose} width="420px" ariaLabel={title}>
  {#snippet header()}
    <ModalHeader onClose={onClose}>
      {#if status === 'loading'}
        <Loader2 size={15} strokeWidth={2} class="dll-spin" />
      {:else}
        <AlertTriangle size={15} strokeWidth={2} class="dll-warn" />
      {/if}
      <span class="modal-title">{title}</span>
    </ModalHeader>
  {/snippet}

  <div class="dll-body">
    {#if status === 'loading'}
      <div class="dll-loading">
        <Spinner size="xl" />
        <span class="dll-caption">Fetching from the remote provider…</span>
      </div>
    {:else}
      <Alert variant="error" text={message ?? 'Something went wrong.'} />
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose}>
      {status === 'loading' ? 'Cancel' : 'Close'}
    </Button>
  {/snippet}
</Modal>

<style>
  .dll-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dll-loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding: 28px 8px 18px;
  }

  .dll-caption {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  /* lucide-svelte forwards `class` to the SVG, so the rule lives in :global. */
  :global(.dll-spin) { animation: dll-rotate 1s linear infinite; }
  :global(.dll-warn) { color: var(--warning); }
  @keyframes -global-dll-rotate {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
