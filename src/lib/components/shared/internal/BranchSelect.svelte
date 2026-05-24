<!--
  BranchSelect — Arbor-specific branch picker.

  Lives under shared/internal/ because it's reused across modals (Create MR,
  rebase, cherry-pick, …) but is not generic enough to live in shared/ui/ —
  it knows about Arbor's `BranchInfo` shape, the `origin/` prefix
  normalisation, and the "branch not in list" sticky entry.

  For the underlying widget see shared/ui/Dropdown.svelte.
-->
<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';

  interface Props {
    value:        string;
    branches:     string[];
    loading?:     boolean;
    disabled?:    boolean;
    placeholder?: string;
    /** Search-filter the menu once the branch list grows past this count. */
    searchThreshold?: number;
  }

  let {
    value           = $bindable(),
    branches,
    loading         = false,
    disabled        = false,
    placeholder     = '— pick a branch —',
    searchThreshold = 12,
  }: Props = $props();

  const items: DropdownItem[] = $derived.by(() => {
    const out: DropdownItem[] = branches.map(b => ({
      kind:    'item',
      id:      b,
      label:   b,
      active:  value === b,
      onclick: () => { value = b; },
    }));
    // Surface a sticky entry for an externally-supplied value not in the list
    // (e.g. a branch from another remote, or a freshly typed name).
    if (value && !branches.includes(value)) {
      out.unshift({
        kind:    'item',
        id:      value,
        label:   value,
        active:  true,
        onclick: () => { value = value; },
      });
    }
    return out;
  });
</script>

<div class="select-wrap">
  <Dropdown
    position="fixed"
    direction="down"
    matchTriggerWidth
    searchable={branches.length > searchThreshold}
    searchPlaceholder="Filter branches…"
    {items}
    {loading}
  >
    {#snippet trigger({ open, toggle })}
      <button
        class="field-select"
        onclick={toggle}
        disabled={disabled || loading}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <span class="field-select-label">{loading ? 'Loading…' : (value || placeholder)}</span>
        <ChevronDown size={12} />
      </button>
    {/snippet}
  </Dropdown>
</div>

<style>
  .select-wrap { display: flex; align-items: center; }
  .select-wrap :global(.dd-root) { width: 100%; }
  .field-select {
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
    font-family: var(--font-code);
    font-size: 11px;
    padding: 7px 10px;
    outline: none;
    cursor: pointer;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .field-select:hover,
  .field-select[aria-expanded='true'] { border-color: var(--accent); }
  .field-select:focus-visible { border-color: var(--border-focus); }
  .field-select:disabled { opacity: 0.5; cursor: default; }
  .field-select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
