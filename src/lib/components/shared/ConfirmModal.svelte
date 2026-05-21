<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { AlertTriangle, Trash2, CheckCircle2, Info } from 'lucide-svelte';
  import Modal from './Modal.svelte';
  import Button from './ui/Button.svelte';

  interface Props {
    title:        string;
    message:      string;
    /** Extra description shown below the message in smaller text. */
    detail?:      string;
    /** Visual intent.  'danger' uses the error palette + Trash icon. */
    variant?:     'default' | 'danger' | 'warning' | 'info';
    confirmLabel?: string;
    cancelLabel?:  string;
    onConfirm:    () => void;
    onCancel:     () => void;
    /** When set, the confirm button is disabled and the component shows a
     *  subtle loading state.  Useful while the parent awaits an async op. */
    busy?:        boolean;
  }
  let {
    title, message, detail,
    variant = 'default',
    confirmLabel = 'Confirm',
    cancelLabel  = 'Cancel',
    onConfirm, onCancel, busy = false,
  }: Props = $props();

  const Icon = $derived(
    variant === 'danger'  ? Trash2 :
    variant === 'warning' ? AlertTriangle :
    variant === 'info'    ? Info :
                            CheckCircle2,
  );

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !busy) { e.preventDefault(); onConfirm(); }
  }

  let confirmBtn = $state<HTMLButtonElement | undefined>(undefined);
  onMount(async () => {
    await tick();
    confirmBtn?.focus();
  });
</script>

<svelte:window onkeydown={onKey} />

<Modal onClose={onCancel} width="440px" ariaLabel={title}>
  {#snippet header()}
    <div class="confirm-icon" class:danger={variant === 'danger'} class:warning={variant === 'warning'} class:info={variant === 'info'}>
      <Icon size={20} />
    </div>
    <span class="modal-title">{title}</span>
  {/snippet}

  <div class="confirm-body">
    <p class="message">{message}</p>
    {#if detail}<p class="detail">{detail}</p>{/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onCancel} type="button">{cancelLabel}</Button>
    <Button
      variant={variant === 'danger' ? 'danger' : 'primary'}
      color={variant === 'warning' ? 'var(--warning)' : undefined}
      onclick={onConfirm}
      disabled={busy}
      loading={busy}
      bind:element={confirmBtn}
      type="button"
    >
      {confirmLabel}
    </Button>
  {/snippet}
</Modal>

<style>
  .confirm-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--accent-subtle);
    color: var(--accent);
    flex-shrink: 0;
  }
  .confirm-icon.danger  { background: var(--error-subtle);   color: var(--error); }
  .confirm-icon.warning { background: var(--warning-subtle); color: var(--warning); }
  .confirm-icon.info    { background: var(--info-subtle);    color: var(--info); }

  .confirm-body {
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .message { font-size: var(--font-size-sm); line-height: 1.5; margin: 0; }
  .detail  {
    margin: 8px 0 0;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }

</style>
