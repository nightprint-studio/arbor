<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from './Dropdown.svelte';
  import { ENCODING_CHOICES } from '$lib/stores/encodingOverrides.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Encoding label currently in effect (auto-detected or override). */
    encoding: string;
    /** True when the user has pinned the encoding via the picker — drives
     *  the warning tint to make the override visible. */
    overridden?: boolean;
    /** Fired when the user picks a different encoding. `undefined` means
     *  "clear override / go back to auto-detect". */
    onChange?: (encoding: string | undefined) => void;
    /** Visually compact 14px variant — for cramped headers (file list rows
     *  in a future iteration). Defaults to the 16px size that matches the
     *  diff toolbar buttons. */
    compact?: boolean;
  }

  let { encoding, overridden = false, onChange, compact = false }: Props = $props();

  // Read-only display (no callback) → just renders the badge, no dropdown.
  const interactive = $derived(typeof onChange === 'function');

  const items = $derived<DropdownItem[]>(
    ENCODING_CHOICES.map(c => ({
      kind:    'item',
      id:      c.value || 'auto',
      label:   c.label,
      // `Auto-detect` is the canonical "no override". Mark it active when
      // there's no override pinned; otherwise mark the pinned encoding.
      active:  c.value === ''
        ? !overridden
        : overridden && c.value === encoding,
      onclick: () => onChange?.(c.value === '' ? undefined : c.value),
    })),
  );
</script>

{#if interactive}
  <Dropdown
    position="fixed"
    direction="down"
    width="240px"
    {items}
  >
    {#snippet trigger({ open, toggle })}
      <button
        type="button"
        class="enc-pill"
        class:overridden
        class:open
        class:compact
        onclick={toggle}
        aria-haspopup="listbox"
        aria-expanded={open}
        use:tooltip={overridden
          ? { content: `Encoding (overridden): ${encoding}`, description: 'Click to change' }
          : { content: `Encoding (auto-detected): ${encoding}`, description: 'Click to override' }}
      >
        <span class="enc-label">{encoding}</span>
        <ChevronDown size={compact ? 9 : 10} />
      </button>
    {/snippet}
  </Dropdown>
{:else}
  <span
    class="enc-pill static"
    class:overridden
    class:compact
    use:tooltip={overridden ? `Encoding (overridden): ${encoding}` : `Encoding (auto-detected): ${encoding}`}
  >
    <span class="enc-label">{encoding}</span>
  </span>
{/if}

<style>
  .enc-pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 16px;
    padding: 0 5px;
    font-size: 10px;
    font-weight: 600;
    font-family: var(--font-code);
    line-height: 1;
    white-space: nowrap;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    cursor: default;
    flex-shrink: 0;
    transition: background-color 80ms ease, border-color 80ms ease, color 80ms ease;
  }
  /* Interactive variant — the trigger button form. */
  button.enc-pill { cursor: pointer; }
  button.enc-pill:hover,
  button.enc-pill.open {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }

  .enc-pill.compact {
    height: 14px;
    padding: 0 4px;
    font-size: 9px;
    gap: 2px;
  }

  /* User has pinned an explicit encoding — warning tint to surface the
     override at a glance. Subtle, never alarming. */
  .enc-pill.overridden {
    background: color-mix(in srgb, var(--warning) 14%, transparent);
    color:      var(--warning);
    border-color: color-mix(in srgb, var(--warning) 32%, transparent);
  }
  button.enc-pill.overridden:hover,
  button.enc-pill.overridden.open {
    background: color-mix(in srgb, var(--warning) 22%, transparent);
  }

  .enc-label {
    /* Some labels (windows-1252, ISO-8859-15) are wider than UTF-8 — let
       them through without truncation; the pill is rendered inside a flex
       header so siblings absorb the extra width. */
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 16ch;
  }
</style>
