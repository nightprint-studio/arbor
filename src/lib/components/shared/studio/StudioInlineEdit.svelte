<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Dropdown, { type DropdownItem } from '../ui/Dropdown.svelte';

  type Mode = 'input' | 'select';

  interface SelectOption { value: string; label?: string }

  interface Props {
    mode: Mode;
    /** Applies the `variant` (enum-variant) styling: syntax-keyword color,
     *  bg-base, right-padding for the dropdown caret. */
    variant?:  boolean;
    options?:  SelectOption[];      // required when mode = 'select'
    placeholder?: string;
    spellcheck?: boolean;
    errorMsg?:   string | null;
    /** input-mode keyboard router (Enter / Escape commit/cancel). */
    onkeydown?:  (e: KeyboardEvent) => void;
    /** select-mode: fires when the user picks an option. The bound
     *  `value` is updated synchronously beforehand. */
    onPick?:     (value: string) => void;
    /** select-mode: fires when the dropdown closes without a pick
     *  (Escape, Tab, click outside). */
    onCancel?:   () => void;
    /** Override widget min-width (px). Default = 80; RON variant uses 140. */
    minWidth?:   number;
    value:       string;
    inputEl?:    HTMLInputElement  | undefined;
  }

  let {
    mode,
    variant = false,
    options = [],
    placeholder,
    spellcheck = false,
    errorMsg,
    onkeydown,
    onPick,
    onCancel,
    minWidth,
    value     = $bindable(),
    inputEl   = $bindable(),
  }: Props = $props();

  function stop(e: Event) { e.stopPropagation(); }

  // ── Select / Dropdown plumbing ──────────────────────────────────────
  // Pick tracking: Dropdown's `onclose` fires for every dismissal path
  // (item click, Escape, outside-click). When the user picks an item we
  // set this flag in the item's `onclick` BEFORE Dropdown's internal
  // `close()` runs, so the close handler can tell "picked something"
  // apart from "dismissed without picking" and call the right callback.
  let picked = false;

  const currentLabel = $derived(
    options.find(o => o.value === value)?.label ?? value
  );

  const dropdownItems: DropdownItem[] = $derived(
    options.map<DropdownItem>((o) => ({
      kind:   'item',
      id:     o.value,
      label:  o.label ?? o.value,
      active: o.value === value,
      onclick: () => {
        picked   = true;
        value    = o.value;
        onPick?.(o.value);
      },
    })),
  );

  function handleClose() {
    if (picked) { picked = false; return; }
    onCancel?.();
  }
</script>

{#if mode === 'select'}
  <span class="sie-wrap" class:sie-variant={variant} style:min-width={minWidth ? `${minWidth}px` : null}>
    <Dropdown
      items={dropdownItems}
      autoOpen
      onclose={handleClose}
      position="fixed"
      width="220px"
    >
      {#snippet trigger(ctx)}
        <button
          type="button"
          class="sie sie-trigger"
          class:sie-variant={variant}
          onclick={(e) => { stop(e); ctx.toggle(); }}
          onmousedown={stop}
          aria-haspopup="listbox"
          aria-expanded={ctx.open}
        >
          <span class="sie-trigger-label">{currentLabel}</span>
          <ChevronDown size={11} strokeWidth={2.2} />
        </button>
      {/snippet}
    </Dropdown>
  </span>
{:else}
  <input
    class="sie"
    bind:this={inputEl}
    bind:value
    {placeholder}
    {spellcheck}
    {onkeydown}
    onclick={stop}
    onmousedown={stop}
  />
{/if}

{#if errorMsg}
  <span class="sie-err" use:tooltip={errorMsg}>!</span>
{/if}

<style>
  .sie {
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--accent);
    border-radius: 3px;
    padding: 0 6px;
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.4;
    height: 20px;
    min-width: 80px;
    max-width: 320px;
    outline: none;
  }
  .sie:focus {
    border-color: var(--accent-strong, var(--accent));
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }

  /* Select trigger — same chrome as the input, with a chevron on the right. */
  .sie-wrap {
    display: inline-flex;
    align-items: center;
  }
  .sie-trigger {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 4px 0 6px;
    cursor: pointer;
    width: 100%;
  }
  .sie-trigger-label {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sie-trigger:focus-visible {
    border-color: var(--accent-strong, var(--accent));
    box-shadow: 0 0 0 2px var(--accent-subtle);
  }

  .sie-variant {
    color: var(--syntax-keyword, #cc7832);
  }
  .sie-variant .sie-trigger-label {
    color: var(--syntax-keyword, #cc7832);
  }

  .sie-err {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-error, rgba(255, 90, 80, 0.18));
    color: var(--text-error, #ff6c5c);
    font-size: 11px;
    font-weight: 700;
    margin-left: 4px;
    cursor: help;
  }
</style>
