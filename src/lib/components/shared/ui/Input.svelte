<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X as XIcon } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type Size = 'sm' | 'md' | 'lg';

  interface Props {
    value: string | number | null | undefined;
    type?: 'text' | 'number' | 'password' | 'email' | 'search' | 'url' | 'tel';
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    /** Constrain width to a single value-friendly column (~80px). */
    narrow?: boolean;
    /** Render full-width (default). */
    block?: boolean;
    size?: Size;
    /** Show an error border + tooltip. */
    error?: string | null;
    /** Show a clear (×) button when the value is non-empty. */
    clearable?: boolean;
    autofocus?: boolean;
    min?: number | string;
    max?: number | string;
    step?: number | string;
    name?: string;
    id?: string;
    ariaLabel?: string;
    /** Leading icon snippet (rendered inside the input on the left). */
    iconStart?: Snippet;
    /** Trailing icon snippet (rendered inside the input on the right, before the clear button). */
    iconEnd?: Snippet;
    onchange?: (value: string) => void;
    oninput?: (value: string) => void;
    onkeydown?: (e: KeyboardEvent) => void;
    onfocus?: (e: FocusEvent) => void;
    onblur?: (e: FocusEvent) => void;
    /** Fired when the clear button is clicked. */
    onclear?: () => void;
  }

  let {
    value = $bindable(),
    type        = 'text',
    placeholder,
    disabled    = false,
    readonly    = false,
    narrow      = false,
    block       = true,
    size        = 'md',
    error       = null,
    clearable   = false,
    autofocus   = false,
    min, max, step,
    name, id,
    ariaLabel,
    iconStart, iconEnd,
    onchange, oninput, onkeydown, onfocus, onblur, onclear,
  }: Props = $props();

  const showClear = $derived(clearable && !disabled && !readonly && value !== '' && value != null);

  let inputEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (autofocus) inputEl?.focus(); });

  function handleClear() {
    value = '';
    oninput?.('');
    onclear?.();
  }
</script>

<span
  class="input-wrap sz-{size}"
  class:narrow
  class:block={block && !narrow}
  class:disabled
  class:readonly
  class:has-error={!!error}
  class:has-icon-start={!!iconStart}
  class:has-icon-end={!!iconEnd || showClear}
  use:tooltip={error ?? ''}
>
  {#if iconStart}
    <span class="input-icon-start">{@render iconStart()}</span>
  {/if}

  <input
    class="text-input"
    {type}
    bind:value
    {placeholder}
    {disabled}
    {readonly}
    {min}
    {max}
    {step}
    {name}
    {id}
    bind:this={inputEl}
    aria-label={ariaLabel}
    aria-invalid={error ? true : undefined}
    onchange={(e) => onchange?.((e.target as HTMLInputElement).value)}
    oninput={(e) => oninput?.((e.target as HTMLInputElement).value)}
    {onkeydown}
    {onfocus}
    {onblur}
  />

  {#if showClear}
    <button
      type="button"
      class="input-clear"
      tabindex="-1"
      aria-label="Clear"
      onclick={handleClear}
    >
      <XIcon size={12} />
    </button>
  {:else if iconEnd}
    <span class="input-icon-end">{@render iconEnd()}</span>
  {/if}
</span>

<style>
  .input-wrap {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    position: relative;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }
  .input-wrap.block { width: 100%; }
  .input-wrap.narrow { width: 80px; }
  .input-wrap:focus-within { border-color: var(--border-focus); }
  .input-wrap.has-error { border-color: var(--error); }
  .input-wrap.disabled { opacity: 0.45; cursor: not-allowed; }
  .input-wrap.readonly { background: var(--bg-elevated); }

  .text-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    outline: none;
    padding: 0;
  }
  .text-input::placeholder { color: var(--text-disabled); }
  .text-input:disabled { cursor: not-allowed; }

  /* ---- Sizes ---- */
  .sz-sm { padding: 0 7px; }
  .sz-sm .text-input { padding: 3px 0; font-size: var(--font-size-xs); }
  .sz-md { padding: 0 8px; }
  .sz-md .text-input { padding: 5px 0; font-size: var(--font-size-sm); }
  .sz-lg { padding: 0 12px; }
  .sz-lg .text-input { padding: 8px 0; font-size: var(--font-size-md); }

  /* ---- Affordances ---- */
  .input-icon-start, .input-icon-end {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .input-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 50%;
    padding: 2px;
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .input-clear:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
