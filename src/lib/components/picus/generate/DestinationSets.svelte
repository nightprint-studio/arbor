<script lang="ts">
  /**
   * The Destinations card's own handle on the saved sets: arm one, or name these.
   *
   * The **list** of sets lives in the sidebar, where "where is this going?" is
   * already the question being answered; this is the copy that sits where the
   * destinations themselves are edited, so saving is offered at the moment you
   * have just finished arranging them. Both read one store, and neither owns the
   * dialogs — see {@link SaveDestinationSetModal}.
   */
  import { BookmarkPlus, Bookmark, TriangleAlert } from 'lucide-svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { destinationSetsStore } from '$lib/stores/picus/destination-sets.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';

  const items = $derived<DropdownItem[]>(
    destinationSetsStore.sets.map((set) => {
      const broken = set.destinations.filter((d) => d.problem).length;
      return {
        kind: 'item' as const,
        id: set.name,
        label: set.name,
        subtitle: broken
          ? `${set.destinations.length - broken} of ${set.destinations.length} usable`
          : `${set.destinations.length} destination${set.destinations.length === 1 ? '' : 's'}`,
        icon: broken ? TriangleAlert : Bookmark,
        active: destinationSetsStore.activeName === set.name,
        onclick: () => destinationSetsStore.apply(set.name),
      };
    }),
  );
</script>

{#if destinationSetsStore.sets.length}
  <!-- `fixed`: the card header sits inside a scrolling column, and an absolutely
       positioned menu would be clipped by it. -->
  <Dropdown {items} position="fixed" width="280px">
    {#snippet trigger({ open, toggle })}
      <Button
        variant="ghost"
        size="xs"
        ariaExpanded={open}
        ariaLabel="Apply a saved set of destinations"
        tooltip={'Arm a saved set of destinations'}
        onclick={toggle}
      >
        {#snippet iconStart()}<Bookmark size={13} />{/snippet}
        Sets
      </Button>
    {/snippet}
  </Dropdown>
{/if}

<Button
  variant="ghost"
  size="xs"
  disabled={!dmlStore.targets.length || !picusProjectStore.attached}
  ariaLabel="Save these destinations as a set"
  tooltip={dmlStore.targets.length
    ? 'Save these destinations under a name, with the repository — update files are stored as their folder, so the set still works next release'
    : { content: 'There are no destinations to save' }}
  onclick={() => picusUiStore.openDestinationSetSave()}
>
  {#snippet iconStart()}<BookmarkPlus size={13} />{/snippet}
  Save as…
</Button>
