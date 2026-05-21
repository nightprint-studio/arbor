<!--
  Reusable search bar for tree-style panels (DocsPanel, SettingsPanel, …).

  Renders: search-icon + input + optional regex toggle (.*) + match counter
  + previous/next/clear buttons. Handles Enter / Shift+Enter / Escape so
  consumers don't have to.

  Match count + navigation are driven by props so the consumer owns the
  match list (e.g. DOM `<mark>` refs).
-->
<script lang="ts">
  import { Search, ChevronUp, ChevronDown } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Bound search query. */
    query: string;
    /** Bound regex-mode flag. Hidden if `showRegex` is false. */
    regex?: boolean;
    /** Show the `.*` regex toggle. Default true. */
    showRegex?: boolean;
    /** Marks regex pattern as invalid → red border on the toggle. */
    regexInvalid?: boolean;
    /** 1-based index of the highlighted match. 0 = none. */
    current?: number;
    /** Total match count. */
    total?: number;
    /** Show match counter + prev/next when query is non-empty. Default true. */
    showCounter?: boolean;
    placeholder?: string;
    autofocus?: boolean;
    ariaLabel?: string;

    onNext?: () => void;
    onPrev?: () => void;
    onClear?: () => void;
    /** Emitted on every keystroke (raw value). */
    oninput?: (value: string) => void;
  }

  let {
    query        = $bindable(''),
    regex        = $bindable(false),
    showRegex    = true,
    regexInvalid = false,
    current      = 0,
    total        = 0,
    showCounter  = true,
    placeholder  = 'Search…',
    autofocus    = false,
    ariaLabel    = 'Search',
    onNext, onPrev, onClear, oninput,
  }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => { if (autofocus) inputEl?.focus(); });

  export function focus() { inputEl?.focus(); inputEl?.select(); }

  function handleInput(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    query = v;
    oninput?.(v);
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (query) { e.preventDefault(); doClear(); }
      return;
    }
    if (e.key === 'Enter') {
      if (!query) return;
      e.preventDefault();
      if (e.shiftKey) onPrev?.();
      else            onNext?.();
    }
  }

  function doClear() {
    query = '';
    oninput?.('');
    onClear?.();
    inputEl?.focus();
  }

  const hasQuery = $derived(query.trim().length > 0);
  const counterLabel = $derived(total > 0 ? `${current}/${total}` : 'No matches');
</script>

<div class="search-bar" class:has-query={hasQuery}>
  <Search size={12} class="sb-icon" />

  <input
    bind:this={inputEl}
    class="sb-input"
    type="text"
    {placeholder}
    value={query}
    oninput={handleInput}
    onkeydown={handleKey}
    aria-label={ariaLabel}
  />

  {#if showRegex}
    <button
      type="button"
      class="sb-flag"
      class:active={regex}
      class:invalid={regex && regexInvalid}
      onclick={() => { regex = !regex; }}
      use:tooltip={regex ? 'Regex mode (on) — click to disable' : 'Regex mode (off) — click to enable'}
      aria-label="Toggle regex search"
      aria-pressed={regex}
      tabindex="-1"
    >.*</button>
  {/if}

  {#if hasQuery && showCounter}
    <span class="sb-counter" class:none={total === 0}>{counterLabel}</span>
    <button
      type="button"
      class="sb-nav"
      onclick={() => onPrev?.()}
      use:tooltip={{ content: 'Previous match', shortcut: 'Shift+Enter' }}
      aria-label="Previous match"
      disabled={total === 0}
      tabindex="-1"
    >
      <ChevronUp size={12} />
    </button>
    <button
      type="button"
      class="sb-nav"
      onclick={() => onNext?.()}
      use:tooltip={{ content: 'Next match', shortcut: 'Enter' }}
      aria-label="Next match"
      disabled={total === 0}
      tabindex="-1"
    >
      <ChevronDown size={12} />
    </button>
  {/if}

  {#if hasQuery}
    <button
      type="button"
      class="sb-clear"
      onclick={doClear}
      use:tooltip={{ content: 'Clear search', shortcut: 'Esc' }}
      aria-label="Clear search"
      tabindex="-1"
    >×</button>
  {/if}
</div>

<style>
  .search-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm, 4px);
    transition: border-color var(--transition-fast);
    min-width: 0;
  }
  .search-bar:focus-within { border-color: var(--accent); }

  :global(.sb-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .sb-input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
  }
  .sb-input::placeholder { color: var(--text-muted); }

  .sb-flag {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 18px;
    min-width: 22px;
    padding: 0 4px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .sb-flag:hover { color: var(--text-secondary); border-color: var(--border); }
  .sb-flag.active {
    color: var(--accent);
    background: var(--accent-subtle);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .sb-flag.invalid {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
    border-color: color-mix(in srgb, var(--error) 50%, transparent);
  }

  .sb-counter {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
    min-width: 28px;
    text-align: center;
    user-select: none;
  }
  .sb-counter.none { color: var(--text-disabled); }

  .sb-nav {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .sb-nav:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .sb-nav:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .sb-clear {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1;
    padding: 0 1px;
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
    transition: color var(--transition-fast);
    flex-shrink: 0;
  }
  .sb-clear:hover { color: var(--text-secondary); }
</style>
