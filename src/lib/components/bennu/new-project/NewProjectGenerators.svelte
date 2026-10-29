<script lang="ts">
  /**
   * The left column of the wizard: project types, grouped by language. Arrow keys move the
   * selection (the form follows at once, like IntelliJ's), Enter or → moves to the form.
   */
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { NewProjectCatalog } from '$lib/ipc/bennu/new-project';
  import { generatorsIn, neighbour } from './new-project-helpers';
  import NewProjectIcon from './NewProjectIcon.svelte';

  let {
    catalog,
    selected,
    onselect,
    onenter,
  }: {
    catalog: NewProjectCatalog;
    selected: string;
    onselect: (id: string) => void;
    /** Enter / → : hand focus to the form. */
    onenter: () => void;
  } = $props();

  /** Catalog order, flattened, so ↑/↓ run through the categories without a seam. */
  const ordered = $derived(catalog.categories.flatMap((c) => generatorsIn(catalog.generators, c.id)));

  function onKey(e: KeyboardEvent) {
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (step) {
      e.preventDefault();
      const next = neighbour(ordered, selected, step as 1 | -1);
      if (next) onselect(next.id);
    } else if (e.key === 'Enter' || e.key === 'ArrowRight') {
      e.preventDefault();
      onenter();
    } else if (e.key === 'Home' && ordered[0]) {
      e.preventDefault();
      onselect(ordered[0].id);
    } else if (e.key === 'End' && ordered.length) {
      e.preventDefault();
      onselect(ordered[ordered.length - 1].id);
    }
  }
</script>

<div
  class="gens"
  role="listbox"
  tabindex="0"
  aria-label="Project types"
  aria-orientation="vertical"
  data-np-gens
  onkeydown={onKey}
>
  {#each catalog.categories as cat (cat.id)}
    <FormSection label={cat.label} first={cat.id === catalog.categories[0]?.id}>
      {#each generatorsIn(catalog.generators, cat.id) as g (g.id)}
        <div class="gen" class:off={!g.available} use:tooltip={g.reason ?? g.description}>
          <SidebarItem selected={g.id === selected} onclick={() => onselect(g.id)}>
            {#snippet icon()}<NewProjectIcon name={g.icon} />{/snippet}
            {g.label}
          </SidebarItem>
        </div>
      {/each}
    </FormSection>
  {/each}
</div>

<style>
  .gens {
    height: 100%; overflow-y: auto; padding: 4px 6px 10px;
    outline: none;
  }
  .gens:focus-visible { box-shadow: inset 0 0 0 1px var(--border-focus); }
  .gen { border-radius: var(--radius-sm); overflow: hidden; }
  .gen.off { opacity: 0.5; }
</style>
