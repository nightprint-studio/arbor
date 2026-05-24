<script lang="ts">
  /**
   * "Stash changes" modal — a small message-input dialog used from both the
   * stage area and the repo actions bar. Composes the shared `<Modal>` shell
   * so it gets the standard backdrop, ESC handling, focus trap and animation
   * for free — no hand-rolled `role="dialog"` divs.
   *
   * The widget owns:
   *   - the input value and Enter-to-submit handling
   *   - the busy state visual (confirm button label flips, both buttons
   *     get disabled while `busy` is true)
   *
   * The host owns:
   *   - whether the modal is mounted (open/close state)
   *   - the actual `stashSave` call and any post-stash refresh
   */
  import { untrack } from 'svelte';
  import { Check, X } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  interface Props {
    /** Branch name shown inside the placeholder hint ("WIP on <branch>…"). */
    branchName?: string;
    /** When true, both buttons are disabled and the confirm button shows
     *  the busy label. */
    busy?: boolean;
    /** Initial message (rarely needed — defaults to empty). */
    initialMessage?: string;
    /** Label on the confirm button at rest. Default: "Stash All". */
    confirmLabel?: string;
    /** Label on the confirm button while `busy` is true. Default: "Stashing…". */
    busyLabel?: string;
    /** Submit handler. Receives the trimmed message (empty string means
     *  the user didn't enter a custom message). */
    onConfirm: (message: string) => void;
    /** Dismiss handler — fired by Cancel button and the Modal's own ESC /
     *  backdrop dismiss. */
    onCancel: () => void;
  }

  let {
    branchName,
    busy           = false,
    initialMessage = '',
    confirmLabel   = 'Stash All',
    busyLabel      = 'Stashing…',
    onConfirm,
    onCancel,
  }: Props = $props();

  let message = $state(untrack(() => initialMessage));

  function submit() {
    if (busy) return;
    onConfirm(message.trim());
  }

  function onKeydown(e: KeyboardEvent) {
    // Enter submits when the input has focus. Modal already handles Escape
    // and backdrop dismiss via the shared shell, so we don't intercept those.
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }
</script>

<Modal onClose={onCancel} width="480px" ariaLabel="Stash changes">
  {#snippet header()}
    <ModalHeader title="Stash changes" onClose={onCancel} />
  {/snippet}

  <div class="stash-body">
    <label class="stash-label" for="stash-message-input">
      Stash message <span class="stash-opt">(optional)</span>
    </label>
    <input
      id="stash-message-input"
      class="stash-input"
      type="text"
      placeholder={`WIP on ${branchName ?? 'branch'}…`}
      bind:value={message}
      onkeydown={onKeydown}
    />
  </div>

  {#snippet footer()}
    <Button variant="ghost" size="sm" onclick={onCancel} disabled={busy}>
      {#snippet iconStart()}<X size={12} />{/snippet}
      Cancel
    </Button>
    <Button
      variant="primary"
      color="var(--success)"
      size="sm"
      onclick={submit}
      loading={busy}
    >
      {#snippet iconStart()}<Check size={12} />{/snippet}
      {busy ? busyLabel : confirmLabel}
    </Button>
  {/snippet}
</Modal>

<style>
  .stash-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .stash-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }
  .stash-opt {
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-muted);
  }

  .stash-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 6px 8px;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .stash-input:focus { border-color: var(--accent); }
</style>
