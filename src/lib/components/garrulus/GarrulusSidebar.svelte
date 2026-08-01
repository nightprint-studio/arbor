<script lang="ts">
  /**
   * The left panel — whichever of the four sections the rail last selected.
   *
   * *Notes* is live: the vault tree, what is pinned and what was opened recently
   * (`NotesSection`). The other three still state what they will hold rather than
   * faking rows, and the copy comes from `GARRULUS_SECTIONS` so the palette's
   * "Show Tags and fields" and this panel's own description can never drift into
   * two names for one thing.
   *
   * The header's buttons are this panel's, not the section's, because
   * `PanelShell` owns the header — so *Notes* hands them out through the
   * component instance. Only verbs that work are there: no "new note" until the
   * flow that creates one exists.
   *
   * Deliberately no "Open a vault" button here: the start pane in the centre
   * already carries that action, and two primary calls to the same door on one
   * screen is a choice the user has to read twice.
   */
  import { ChevronsDownUp, ChevronsUpDown, Crosshair, RefreshCw } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import NotesSection from './notes/NotesSection.svelte';
  import TagsPanel from './panels/TagsPanel.svelte';
  import TypesPanel from './panels/TypesPanel.svelte';
  import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import type { GarrulusSection } from './garrulus-palette';

  interface Props {
    /** The section on screen — the rail and this panel read one list. */
    section: GarrulusSection;
  }

  let { section }: Props = $props();

  let notesRef = $state<{
    expandAll: () => void;
    collapseAll: () => void;
    revealActive: () => Promise<void>;
    focusFilter: () => void;
  } | null>(null);

  const showNotes = $derived(section.id === 'notes' && garrulusVaultStore.isOpen);
</script>

<PanelShell title={section.label}>
  {#snippet actions()}
    {#if showNotes}
      <button
        class="ps-btn"
        type="button"
        onclick={() => void notesRef?.revealActive()}
        disabled={!garrulusNotesStore.activePath}
        use:tooltip={'Show the open note in the tree'}
        aria-label="Show the open note in the tree"
      >
        <Crosshair size={14} />
      </button>
      <button
        class="ps-btn"
        type="button"
        onclick={() => notesRef?.collapseAll()}
        use:tooltip={'Collapse all folders'}
        aria-label="Collapse all folders"
      >
        <ChevronsDownUp size={14} />
      </button>
      <button
        class="ps-btn"
        type="button"
        onclick={() => notesRef?.expandAll()}
        use:tooltip={'Expand all folders'}
        aria-label="Expand all folders"
      >
        <ChevronsUpDown size={14} />
      </button>
      <button
        class="ps-btn"
        type="button"
        onclick={() => garrulusNotesStore.refresh()}
        use:tooltip={{
          content: 'Re-read the list of notes',
          description: 'An index read — it does not touch a file.',
        }}
        aria-label="Re-read the list of notes"
      >
        <RefreshCw size={14} />
      </button>
    {/if}
  {/snippet}

  {#if !garrulusVaultStore.isOpen}
    <EmptyState message="No vault open." description={section.description} />
  {:else if section.id === 'notes'}
    <NotesSection bind:this={notesRef} />
  {:else if section.id === 'tags'}
    <!-- Both facet panels filter the search store, so selecting a row is only
         half an action unless the results come forward with it. -->
    <TagsPanel onShowResults={() => garrulusUiStore.showSection('search')} />
  {:else if section.id === 'types'}
    <TypesPanel onShowResults={() => garrulusUiStore.showSection('search')} />
  {:else}
    <EmptyState message="Nothing here yet." description={section.description} />
  {/if}
</PanelShell>
