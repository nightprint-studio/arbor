<script lang="ts">
  /**
   * NamePromptModal — the one "type a name" dialog of the project tree: rename, new folder, new
   * file. The first field is focused with the base name (without extension) pre-selected, Enter
   * submits, Esc cancels (Modal). It validates only what no filesystem accepts — empty, a path
   * separator — and reports the rest (a clash, a permission) as an inline error from `onSubmit`.
   */
  import { untrack } from 'svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  interface Props {
    title: string;
    label?: string;
    hint?: string;
    initial?: string;
    confirmLabel?: string;
    /** Resolves with an error message to keep the dialog open, or nothing to close it. */
    onSubmit: (name: string) => Promise<string | void> | string | void;
    onClose: () => void;
  }

  let { title, label = 'Name', hint, initial = '', confirmLabel = 'OK', onSubmit, onClose }: Props = $props();

  let name = $state(untrack(() => initial));
  let busy = $state(false);
  let error = $state<string | null>(null);
  let inputEl = $state<HTMLInputElement | undefined>();

  const valid = $derived(name.trim().length > 0 && !/[\\/]/.test(name) && name.trim() !== untrack(() => initial));

  // Focus with the base name selected (the extension stays out of the way of a retype).
  $effect(() => {
    const el = inputEl;
    if (!el) return;
    el.focus();
    const dot = initial.lastIndexOf('.');
    el.setSelectionRange(0, dot > 0 ? dot : initial.length);
  });

  async function submit() {
    if (!valid || busy) return;
    busy = true;
    error = null;
    try {
      const problem = await onSubmit(name.trim());
      if (problem) error = problem; else onClose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {onClose} width="520px" height="260px" ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}><span class="npm-title">{title}</span></ModalHeader>
  {/snippet}

  <div class="npm-body">
    <FormField {label} {hint} {error}>
      <Input
        bind:value={name}
        bind:element={inputEl}
        ariaLabel={label}
        onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); void submit(); } }}
      />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!valid || busy} onclick={submit}>{confirmLabel}</Button>
  {/snippet}
</Modal>

<style>
  .npm-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .npm-body { padding: 16px 4px 4px; }
</style>
