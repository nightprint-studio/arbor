<script lang="ts">
  import { Check, X } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type Size = 'sm' | 'md';

  interface Props {
    value: string;
    placeholder?: string;
    size?: Size;
    maxlength?: number;
    /** Optional sync validator. Return null/undefined for valid, or an error message. */
    validate?: (value: string) => string | null | undefined;
    /** Disable confirm if value is empty (default true). */
    requireValue?: boolean;
    onconfirm: (value: string) => void;
    oncancel: () => void;
  }

  let {
    value = $bindable(),
    placeholder,
    size = 'sm',
    maxlength,
    validate,
    requireValue = true,
    onconfirm,
    oncancel,
  }: Props = $props();

  const trimmed   = $derived(value.trim());
  const error     = $derived(validate?.(trimmed) ?? null);
  const canSubmit = $derived(!error && (!requireValue || trimmed.length > 0));

  let inputEl: HTMLInputElement | undefined = $state();
  $effect(() => { inputEl?.focus(); inputEl?.select(); });

  function confirm() {
    if (!canSubmit) return;
    onconfirm(trimmed);
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter')  { e.preventDefault(); confirm(); }
    if (e.key === 'Escape') { e.preventDefault(); oncancel(); }
  }
</script>

<div class="inline-edit sz-{size}" class:has-error={!!error}>
  <input
    class="edit-input"
    bind:value
    {placeholder}
    {maxlength}
    onkeydown={keydown}
    aria-invalid={error ? true : undefined}
    bind:this={inputEl}
  />
  <button
    type="button"
    class="edit-btn confirm"
    use:tooltip={error ?? 'Confirm'}
    aria-label="Confirm"
    disabled={!canSubmit}
    onclick={confirm}
  >
    <Check size={size === 'md' ? 13 : 11} />
  </button>
  <button
    type="button"
    class="edit-btn cancel"
    use:tooltip={'Cancel'}
    aria-label="Cancel"
    onclick={oncancel}
  >
    <X size={size === 'md' ? 13 : 11} />
  </button>
</div>

{#if error}
  <div class="edit-error" role="alert">{error}</div>
{/if}

<style>
  .inline-edit {
    display: flex;
    align-items: center;
    gap: 3px;
    flex: 1;
    min-width: 0;
  }

  .edit-input {
    flex: 1;
    min-width: 0;
    background: var(--bg-input);
    border: 1px solid var(--border-focus);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    outline: none;
  }
  .inline-edit.sz-sm .edit-input { padding: 2px 5px; font-size: var(--font-size-xs); }
  .inline-edit.sz-md .edit-input { padding: 4px 8px; font-size: var(--font-size-sm); }

  .inline-edit.has-error .edit-input { border-color: var(--error); }

  .edit-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    padding: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .edit-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .inline-edit.sz-sm .edit-btn { width: 18px; height: 18px; }
  .inline-edit.sz-md .edit-btn { width: 22px; height: 22px; }

  .edit-btn.confirm { color: var(--success); }
  .edit-btn.confirm:hover:not(:disabled) { background: var(--success-subtle); }
  .edit-btn.cancel  { color: var(--text-muted); }
  .edit-btn.cancel:hover  { background: var(--bg-overlay); color: var(--text-primary); }

  .edit-error {
    margin-top: 3px;
    font-size: var(--font-size-xs);
    color: var(--error);
  }
</style>
