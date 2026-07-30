<script lang="ts">
  /**
   * The saved sets of destinations, as a sidebar section.
   *
   * This is where they belong: the Generate panel answers "where is this going?",
   * and a set is the shortest possible answer to it — the same six places, named
   * once. Clicking one **replaces** the armed destinations rather than adding to
   * them, which is the store's rule and the reason the current one is marked.
   *
   * The list, the applying and the saving all come from `destinationSetsStore`,
   * so this and the Destinations card cannot drift about what exists.
   */
  import { Bookmark, BookmarkPlus, Save, TriangleAlert, Trash2 } from 'lucide-svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { destinationSetsStore } from '$lib/stores/picus/destination-sets.svelte';
  import { dmlStore } from '$lib/stores/picus/dml.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import type { ResolvedSet } from '$lib/ipc/picus/project';

  /** How many of a set's entries the repository can still resolve. */
  function broken(set: ResolvedSet): number {
    return set.destinations.filter((d) => d.problem).length;
  }

  function summary(set: ResolvedSet): string {
    const bad = broken(set);
    if (bad) return `${set.destinations.length - bad} of ${set.destinations.length} usable`;
    return `${set.destinations.length} destination${set.destinations.length === 1 ? '' : 's'}`;
  }

  /**
   * What arming it would actually do, file by file — the question you ask before
   * clicking, and the only place the resolution is visible before it happens.
   */
  function detail(set: ResolvedSet): string {
    return set.destinations
      .map((d) => {
        if (d.problem) return d.problem;
        const notes = [
          d.createsFile ? 'new' : null,
          // Only worth saying on an update: everything else names one file for
          // ever by design, and marking those would be noise on every row.
          d.pinned && d.role === 'update' ? 'fixed name' : null,
        ].filter(Boolean);
        return notes.length ? `${d.file}  (${notes.join(', ')})` : d.file;
      })
      .join('\n');
  }

  function apply(name: string) {
    destinationSetsStore.apply(name);
    picusTabsStore.openGenerate();
  }
</script>

<SidebarSection
  label="Sets"
  badge={destinationSetsStore.sets.length || null}
  badgeTitle="Saved sets of destinations"
  expanded
>
  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      disabled={!dmlStore.targets.length || !picusProjectStore.attached}
      ariaLabel="Save these destinations as a set"
      tooltip={dmlStore.targets.length
        ? 'Save the armed destinations under a name, with the repository'
        : { content: 'There are no destinations to save' }}
      onclick={() => picusUiStore.openDestinationSetSave()}
    >
      {#snippet iconStart()}<BookmarkPlus size={13} />{/snippet}
    </Button>
  {/snippet}

  {#each destinationSetsStore.sets as set (set.name)}
    <SidebarItem
      selected={destinationSetsStore.activeName === set.name}
      onclick={() => apply(set.name)}
    >
      {#snippet icon()}
        {#if broken(set)}
          <span class="dsl-warn"><TriangleAlert size={13} /></span>
        {:else}
          <Bookmark size={13} />
        {/if}
      {/snippet}
      <span class="dsl-name" use:tooltip={{ content: set.name, description: detail(set) }}>
        {set.name}
      </span>
      {#snippet subtitle()}{summary(set)}{/snippet}
      {#snippet actions()}
        {#if dmlStore.targets.length}
          <!-- Overwriting a set had no affordance at all: the only way was to
               retype its name character for character in the save dialog. This
               opens that dialog on the name, so the replacement is still
               confirmed rather than done by a click on a hover button. -->
          <button
            type="button"
            aria-label={`Replace ${set.name} with the armed destinations`}
            use:tooltip={'Replace this set with the armed destinations'}
            onclick={(e) => {
              e.stopPropagation();
              picusUiStore.openDestinationSetSave(set.name);
            }}
          >
            <Save size={12} />
          </button>
        {/if}
        <button
          type="button"
          class="danger"
          aria-label={`Forget ${set.name}`}
          use:tooltip={'Forget this set'}
          onclick={(e) => {
            e.stopPropagation();
            picusUiStore.requestDestinationSetDelete(set.name);
          }}
        >
          <Trash2 size={12} />
        </button>
      {/snippet}
    </SidebarItem>
  {:else}
    <p class="dsl-empty">
      {#if picusProjectStore.attached}
        A set names the places a change like this always goes, so the list is arranged
        once instead of every release.
      {:else}
        Attach a script repository and the sets it declares appear here.
      {/if}
    </p>
    {#if picusProjectStore.attached}
      <!-- Spelled out rather than left to the section's hover action: with nothing
           in the list there is no row to hover, and an affordance that only appears
           over an empty box is one nobody finds. -->
      <div class="dsl-cta">
        <Button
          variant="ghost"
          size="xs"
          block
          disabled={!dmlStore.targets.length}
          tooltip={dmlStore.targets.length
            ? 'Save the armed destinations under a name'
            : { content: 'Arm the destinations first — a set is the list of them' }}
          onclick={() => picusUiStore.openDestinationSetSave()}
        >
          {#snippet iconStart()}<BookmarkPlus size={13} />{/snippet}
          Save these destinations…
        </Button>
      </div>
    {/if}
  {/each}
</SidebarSection>

<style>
  .dsl-name { overflow: hidden; text-overflow: ellipsis; }

  .dsl-cta { padding: 0 8px 6px; }

  .dsl-empty {
    padding: 4px 12px 6px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }

  /* The icon carries the tone; the row's own colour stays neutral. */
  .dsl-warn { display: flex; color: var(--warning); }
</style>
