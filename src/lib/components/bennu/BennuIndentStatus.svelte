<script lang="ts">
  /**
   * Bennu status-bar indentation control — the VS Code-style "Spaces: 4" widget.
   * Shows the active indent style + tab width, and (on click / keyboard) opens an
   * upward menu to switch tabs↔spaces and pick the tab width. Writes flow through
   * `bennuSettingsStore`, so the open editor reconfigures live (CodeEditor applies
   * `tabSize` + `indentUnit` via a compartment).
   *
   * NOTE: like the rest of the bennu settings the store is MOCK-persisted today
   * (rule 11 seam) — the choice survives the session, not a restart, until the
   * typed `[bennu]` config lands. The control is intentionally identical in shape
   * to BennuSettingsModal's indent options so the two never drift.
   */
  import { IndentIncrease } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';

  /** Offered tab widths — the common IntelliJ/VS Code set. */
  const TAB_WIDTHS = [2, 4, 8] as const;

  const label = $derived(
    bennuSettingsStore.indentStyle === 'spaces'
      ? `Spaces: ${bennuSettingsStore.tabSize}`
      : `Tab Size: ${bennuSettingsStore.tabSize}`,
  );

  const items: DropdownItem[] = $derived([
    {
      kind: 'group',
      id: 'style',
      label: 'Indent Using',
      items: [
        {
          kind: 'item',
          id: 'spaces',
          label: 'Spaces',
          active: bennuSettingsStore.indentStyle === 'spaces',
          onclick: () => bennuSettingsStore.setIndentStyle('spaces'),
        },
        {
          kind: 'item',
          id: 'tabs',
          label: 'Tabs',
          active: bennuSettingsStore.indentStyle === 'tabs',
          onclick: () => bennuSettingsStore.setIndentStyle('tabs'),
        },
      ],
    },
    { kind: 'separator' },
    {
      kind: 'group',
      id: 'width',
      label: 'Tab Width',
      items: TAB_WIDTHS.map((w) => ({
        kind: 'item' as const,
        id: `w${w}`,
        label: String(w),
        active: bennuSettingsStore.tabSize === w,
        onclick: () => bennuSettingsStore.setTabSize(w),
      })),
    },
  ]);
</script>

<Dropdown {items} position="fixed" direction="up" width="180px" selectionMode="single">
  {#snippet trigger({ toggle, open })}
    <button
      type="button"
      class="bf-btn"
      class:open
      onclick={toggle}
      use:tooltip={'Select indentation'}
    >
      <IndentIncrease size={12} />
      {label}
    </button>
  {/snippet}
</Dropdown>

<style>
  /* Mirrors .bf-item from BennuStatusBar, but interactive (button reset + hover). */
  .bf-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    background: transparent;
    border: none;
    padding: 2px 6px;
    margin: 0;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .bf-btn :global(svg) { color: var(--text-disabled); }
  .bf-btn:hover,
  .bf-btn.open { background: var(--bg-hover); color: var(--text-secondary); }
  .bf-btn:hover :global(svg),
  .bf-btn.open :global(svg) { color: var(--text-secondary); }
</style>
