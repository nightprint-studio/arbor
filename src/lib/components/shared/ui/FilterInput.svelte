<!--
  FilterInput — compact "narrow this list" field: lens icon, input, and a clear
  button that shows while there is text. Esc empties the field first (and only
  then lets the key bubble), so a panel that closes on Esc isn't closed by the
  gesture that meant "drop the filter".

  Not SearchBar: no match counter / next-prev / regex. This is for live list
  filters in toolbars (Problems, Console, …).
-->
<script lang="ts">
  import { Search, X } from 'lucide-svelte';

  interface Props {
    /** Bound filter text. */
    value: string;
    /** Bound `<input>`, so a host can focus / select it (e.g. Ctrl+F). */
    element?: HTMLInputElement | null;
    placeholder?: string;
    ariaLabel?: string;
  }

  let {
    value = $bindable(''),
    element = $bindable<HTMLInputElement | null>(null),
    placeholder = 'Search…',
    ariaLabel,
  }: Props = $props();

  function clear() { value = ''; element?.focus(); }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && value) { e.preventDefault(); e.stopPropagation(); clear(); }
  }
</script>

<div class="filter-input">
  <Search size={12} />
  <input
    bind:this={element}
    bind:value
    {placeholder}
    aria-label={ariaLabel}
    spellcheck="false"
    onkeydown={onKey}
  />
  {#if value}
    <button type="button" class="fi-clear" onclick={clear} aria-label="Clear search"><X size={11} /></button>
  {/if}
</div>

<style>
  .filter-input {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;
    height: 24px; padding: 0 7px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-muted);
  }
  .filter-input:focus-within { border-color: var(--border-focus); }
  input {
    flex: 1; min-width: 0; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
  }
  input::placeholder { color: var(--text-disabled); }
  .fi-clear {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; flex-shrink: 0;
    background: transparent; border: none; border-radius: 50%;
    color: var(--text-muted); cursor: pointer;
  }
  .fi-clear:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
