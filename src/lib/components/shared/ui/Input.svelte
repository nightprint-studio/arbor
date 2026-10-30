<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X as XIcon } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type Size = 'sm' | 'md' | 'lg';

  interface Props {
    value: string | number | null | undefined;
    /** Bind the underlying `<input>` element, so a parent can focus / select it
     *  imperatively (e.g. focus the field when a form section opens). */
    element?: HTMLInputElement;
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
    /** Extra attributes spread onto the underlying `<input>` — ARIA wiring a host owns (a
     *  combobox's `role` / `aria-controls`) or a `data-*` hook, without this widget growing a
     *  prop for each. */
    attrs?: Record<string, unknown>;
    /** Leading icon snippet (rendered inside the input on the left). */
    iconStart?: Snippet;
    /** Trailing icon snippet (rendered inside the input on the right, before the clear button). */
    iconEnd?: Snippet;
    /** Leading text affix (e.g. "$", "https://", "@"). Distinct from `iconStart`:
     *  this renders as a muted text span, not a Snippet. Use for short
     *  units / scheme prefixes / sigils. */
    prefix?: string;
    /** Trailing text affix (e.g. "kg", "%", ".com"). Muted text span after the
     *  input, before the clear button / iconEnd. */
    suffix?: string;
    /** A completion drawn in muted text right after what has been typed — the part the host would add. Shown
     *  only while the caret is at the end of the field; Tab or → there takes it, via {@link onacceptghost}. */
    ghost?: string;
    /** The user took the ghost text: the host sets the value (this widget does not guess how the completion joins what was typed). */
    onacceptghost?: () => void;
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
    element = $bindable<HTMLInputElement | undefined>(),
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
    attrs,
    iconStart, iconEnd,
    prefix, suffix,
    ghost, onacceptghost,
    onchange, oninput, onkeydown, onfocus, onblur, onclear,
  }: Props = $props();

  const showClear = $derived(clearable && !disabled && !readonly && value !== '' && value != null);

  $effect(() => { if (autofocus) element?.focus(); });

  /** Tab / → at the very end of the text takes the ghost completion; anywhere else the key does what it always does. */
  function handleKeydown(e: KeyboardEvent) {
    if (ghost && onacceptghost && (e.key === 'Tab' || e.key === 'ArrowRight') && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
      const el = e.currentTarget as HTMLInputElement;
      if (el.selectionStart === el.value.length && el.selectionEnd === el.value.length) {
        e.preventDefault();
        onacceptghost();
        return;
      }
    }
    onkeydown?.(e);
  }

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
  {#if prefix}
    <span class="input-affix input-affix-start">{prefix}</span>
  {/if}

  <span class="input-field">
  {#if ghost}
    <span class="input-ghost" aria-hidden="true"><span class="ghost-typed">{value ?? ''}</span><span class="ghost-text">{ghost}</span></span>
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
    bind:this={element}
    aria-label={ariaLabel}
    aria-invalid={error ? true : undefined}
    {...attrs}
    onchange={(e) => onchange?.((e.target as HTMLInputElement).value)}
    oninput={(e) => oninput?.((e.target as HTMLInputElement).value)}
    onkeydown={handleKeydown}
    {onfocus}
    {onblur}
  />
  </span>

  {#if suffix}
    <span class="input-affix input-affix-end">{suffix}</span>
  {/if}
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

  /* The field is a positioning context so the ghost layer can sit exactly over the input's text box. */
  .input-field { position: relative; display: flex; flex: 1; min-width: 0; }
  .input-ghost {
    position: absolute; inset: 0;
    display: flex; align-items: center;
    font-family: var(--font-ui-sans);
    white-space: pre; overflow: hidden; pointer-events: none;
  }
  .ghost-typed { visibility: hidden; }
  .ghost-text { color: var(--text-disabled); }

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
  .sz-sm .input-ghost { padding: 3px 0; font-size: var(--font-size-xs); }
  .sz-md { padding: 0 8px; }
  .sz-md .text-input { padding: 5px 0; font-size: var(--font-size-sm); }
  .sz-md .input-ghost { padding: 5px 0; font-size: var(--font-size-sm); }
  .sz-lg { padding: 0 12px; }
  .sz-lg .text-input { padding: 8px 0; font-size: var(--font-size-md); }
  .sz-lg .input-ghost { padding: 8px 0; font-size: var(--font-size-md); }

  /* ---- Affordances ---- */
  .input-icon-start, .input-icon-end {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  /* Text affixes — kept visually lighter than the typed value so the user
     reads them as units / sigils, not as part of the editable text. */
  .input-affix {
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    flex-shrink: 0;
    user-select: none;
    pointer-events: none;
  }
  .sz-sm .input-affix { font-size: var(--font-size-xs); }
  .sz-md .input-affix { font-size: var(--font-size-sm); }
  .sz-lg .input-affix { font-size: var(--font-size-md); }
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
