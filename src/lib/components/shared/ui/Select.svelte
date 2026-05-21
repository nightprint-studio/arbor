<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';
  import Dropdown from './Dropdown.svelte';
  import type { DropdownItem } from './Dropdown.svelte';

  interface Option {
    value: string | number;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value: string | number;
    options: Option[];
    disabled?: boolean;
    narrow?: boolean;
    /**
     * Cap on the menu's visual height in pixels. Passed straight to the
     * underlying `Dropdown` — important when the dropdown might open
     * *upward* (`flipUp`), because Dropdown's positioning formula uses
     * this value as the worst-case menu height. Leaving it unset means
     * 420 is assumed, which leaves a visible gap above the trigger when
     * the actual menu is much shorter.
     *
     * If omitted, Select auto-derives a tight value from `options.length`
     * (32px per item + 16px padding, capped at 420). Pass an explicit
     * value when items render taller (subtitle, avatar, …).
     */
    maxHeight?: number;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(),
    options,
    disabled  = false,
    narrow    = false,
    maxHeight,
    onchange,
  }: Props = $props();

  /** Tight bound on the menu height so the upward-flip placement doesn't
   *  reserve room for a phantom 420px menu. Caller can override. */
  const derivedMaxHeight = $derived(
    maxHeight ?? Math.min(420, options.length * 32 + 16),
  );

  const items = $derived<DropdownItem[]>(
    options.map(o => ({
      kind:     'item',
      id:       String(o.value),
      label:    o.label,
      active:   String(o.value) === String(value),
      disabled: o.disabled,
      onclick:  () => {
        value = o.value;
        onchange?.(String(o.value));
      },
    })),
  );

  const selectedLabel = $derived(
    options.find(o => String(o.value) === String(value))?.label ?? '',
  );
</script>

<div class="select-wrap" class:narrow>
  <Dropdown
    position="fixed"
    direction="down"
    matchTriggerWidth
    maxHeight={derivedMaxHeight}
    {items}
  >
    {#snippet trigger({ open, toggle })}
      <button
        class="select-input"
        class:narrow
        onclick={toggle}
        {disabled}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <span class="select-input-label">{selectedLabel}</span>
        <ChevronDown size={11} />
      </button>
    {/snippet}
  </Dropdown>
</div>

<style>
  .select-wrap { display: inline-block; }
  .select-wrap.narrow { width: 120px; }
  .select-wrap :global(.dd-root) { width: 100%; }

  .select-input {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    padding: 5px 8px;
    cursor: pointer;
    outline: none;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .select-input:focus,
  .select-input[aria-expanded='true'] { border-color: var(--border-focus); }
  .select-input:disabled { opacity: 0.45; cursor: not-allowed; }
  .select-input-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
