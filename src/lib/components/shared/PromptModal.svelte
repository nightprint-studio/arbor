<script lang="ts">
  /**
   * PromptModal — single-line text prompt, the Arbor replacement for `window.prompt`.
   *
   *   {#if prompt}
   *     <PromptModal title="New field" label="Field name:" initialValue="new_field"
   *                  onConfirm={(v) => { prompt = null; use(v); }}
   *                  onCancel={() => (prompt = null)} />
   *   {/if}
   *
   * Cancelling (Esc, ×, backdrop, Cancel) calls `onCancel` and never `onConfirm`. An empty
   * value is not confirmable — same as `window.prompt` callers that bail on `!value`.
   * `validate` returns an error message (shown inline, confirm blocked) or null.
   */
  import { onMount, tick } from 'svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import ModalFooter from './ModalFooter.svelte';
  import Button from './ui/Button.svelte';
  import Input from './ui/Input.svelte';

  interface Props {
    title:         string;
    /** The question shown above the input. */
    label:         string;
    initialValue?: string;
    placeholder?:  string;
    confirmLabel?: string;
    cancelLabel?:  string;
    /** Return an error message to block confirm, or null when the value is acceptable. */
    validate?:     (value: string) => string | null;
    onConfirm:     (value: string) => void;
    onCancel:      () => void;
    zIndex?:       string;
  }
  let {
    title, label, initialValue = '', placeholder,
    confirmLabel = 'OK', cancelLabel = 'Cancel',
    validate, onConfirm, onCancel, zIndex,
  }: Props = $props();

  // svelte-ignore state_referenced_locally
  let value = $state(initialValue);
  let inputEl = $state<HTMLInputElement | undefined>(undefined);

  const error = $derived(value.length > 0 ? (validate?.(value) ?? null) : null);
  const canConfirm = $derived(value.length > 0 && !error);

  function submit() {
    if (canConfirm) onConfirm(value);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); submit(); }
  }

  onMount(async () => {
    await tick();
    inputEl?.focus();
    inputEl?.select();
  });
</script>

<Modal onClose={onCancel} width="440px" ariaLabel={title} {zIndex}>
  {#snippet header()}
    <ModalHeader {title} onClose={onCancel} />
  {/snippet}

  <div class="prompt-body">
    <label class="prompt-label" for="prompt-modal-input">{label}</label>
    <Input
      id="prompt-modal-input"
      bind:value
      bind:element={inputEl}
      {placeholder}
      {error}
      ariaLabel={label}
      onkeydown={onKey}
    />
    {#if error}<p class="prompt-error">{error}</p>{/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="secondary" onclick={onCancel} type="button">{cancelLabel}</Button>
      <Button variant="primary" onclick={submit} disabled={!canConfirm} type="button">{confirmLabel}</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .prompt-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-family: var(--font-ui-sans);
  }
  .prompt-label {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
  }
  .prompt-error {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--error);
  }
</style>
