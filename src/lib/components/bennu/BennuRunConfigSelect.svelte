<script lang="ts">
  /**
   * The run-configuration selector — the chip left of ▷ / 🐞 in the title bar.
   *
   * IntelliJ's arrangement, and for its reason: the two buttons beside it do something
   * irreversible-ish (they start a process), and until now nothing on screen said WHAT they
   * would start. The name of the target belongs next to the button that launches it, not
   * three clicks away inside an editor.
   *
   * Picking one here makes it **active** — it does not run it. Choosing what ▷ means and
   * pressing ▷ are separate acts, and merging them would make the list a minefield.
   *
   * The dropdown is grouped by category, the same grouping the editor's list uses (one
   * `groupedFor` in the store), so the two views of the same configurations agree.
   */
  import { ChevronDown, SlidersHorizontal, Plus } from 'lucide-svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunConfigStore, type RunConfig } from '$lib/stores/bennu/run-config.svelte';
  import { runConfigIcon } from './run-kinds';

  const root = $derived(projectStore.project?.root ?? null);
  const groups = $derived(root ? bennuRunConfigStore.groupedFor(root) : []);
  const active = $derived(root ? bennuRunConfigStore.activeFor(root) : null);
  const count = $derived(groups.reduce((n, g) => n + g.configs.length, 0));
  const isMultiModule = $derived((projectStore.project?.modules ?? []).length > 0);

  /** The module a configuration is for — its own for a JVM one, its scope's target for a
   *  module-scoped test run. Empty means the project root. */
  function moduleOf(c: RunConfig): string {
    return c.kind === 'junit'
      ? (c.testScope === 'module' ? c.testTarget : '')
      : c.module;
  }

  const items = $derived<DropdownItem[]>([
    ...groups.map((g) => ({
      kind: 'group' as const,
      id: `k:${g.kind}`,
      label: g.label,
      items: g.configs.map((c) => ({
        kind: 'item' as const,
        id: c.id,
        label: c.name || 'Unnamed',
        // Which module, on a reactor — four configurations called "Application" are otherwise
        // the same row four times.
        meta: isMultiModule ? moduleOf(c) || undefined : undefined,
        icon: runConfigIcon(c),
        // A check, not a highlight: this is a single-choice list and the check is what says
        // "this is the one ▷ will run".
        active: c.id === active?.id,
        onclick: () => root && bennuRunConfigStore.setActive(root, c.id),
      })),
    })),
    ...(count ? [{ kind: 'separator' as const }] : []),
    {
      kind: 'item' as const,
      id: 'edit',
      label: count ? 'Edit Configurations…' : 'Add Configuration…',
      icon: count ? SlidersHorizontal : Plus,
      onclick: () => bennuUiStore.openRunConfig(),
    },
  ]);
</script>

<Dropdown {items} position="fixed" direction="down" width="260px">
  {#snippet trigger({ open, toggle })}
    <button
      type="button"
      class="rcs"
      class:open
      class:empty={!active}
      onclick={toggle}
      disabled={!root}
      use:tooltip={active
        ? { content: `Run configuration: ${active.name}` }
        : { content: 'No run configuration yet — pick or create one' }}
      aria-label="Run configuration"
      aria-haspopup="menu"
      aria-expanded={open}
    >
      {#if active}
        {@const Ic = runConfigIcon(active)}
        <Ic size={13} />
        <span class="rcs-name">{active.name || 'Unnamed'}</span>
        {#if isMultiModule && moduleOf(active)}
          <span class="rcs-mod">{moduleOf(active)}</span>
        {/if}
      {:else}
        <Plus size={13} />
        <span class="rcs-name">Add Configuration</span>
      {/if}
      <ChevronDown size={12} />
    </button>
  {/snippet}
</Dropdown>

<style>
  /* Sized and coloured like the run buttons beside it, but wearing a surface so it reads as
     a FIELD (something with a value) rather than as a third button. */
  .rcs {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    max-width: 190px;
    padding: 0 6px;
    background: var(--bg-overlay);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font: var(--font-size-xs) var(--font-ui-sans);
    cursor: pointer;
    -webkit-app-region: no-drag;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .rcs:hover:not(:disabled), .rcs.open {
    background: color-mix(in srgb, var(--text-primary) 12%, transparent);
    color: var(--text-primary);
  }
  .rcs:disabled { opacity: 0.45; cursor: default; }
  /* Nothing configured yet: quieter, because it is an invitation and not a value. */
  .rcs.empty { background: transparent; border-color: var(--border-subtle); color: var(--text-muted); }
  .rcs-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The module, quieter than the name — on a reactor it is what disambiguates four
     configurations that are all called "Application". */
  .rcs-mod {
    min-width: 0; flex-shrink: 2;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .rcs:hover:not(:disabled) .rcs-mod, .rcs.open .rcs-mod { color: var(--text-muted); }
</style>
