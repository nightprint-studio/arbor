<script lang="ts">
  /**
   * A flat list of notes — what the *Pinned* and *Recent* sections of the sidebar
   * are, and nothing else.
   *
   * Kept apart from `NoteTree` because the two are different objects: this one has
   * no hierarchy, no expansion and no virtualisation to justify, and running it
   * through a tree widget would mean explaining to the tree that every node is a
   * leaf. What the two do share — the row's insides and the arrow-key movement
   * between rows — is shared as `NoteRowContent` and `row-focus`, so the sections
   * cannot drift apart visually or on the keyboard.
   */
  import NoteRowContent from './NoteRowContent.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { moveRowFocus } from './row-focus';
  import { garrulusNotesStore, type CatalogueNote } from '$lib/stores/garrulus/notes.svelte';

  interface Props {
    items: { note: CatalogueNote; meta?: string | null }[];
    /** The note on screen, drawn as selected. */
    activePath?: string | null;
    onOpen: (path: string) => void;
    /** Focus walked off the top of the list — the panel puts it back in the
     *  filter box, so ↑ from the first row is never a dead key. */
    onLeaveTop?: () => void;
    emptyMessage: string;
  }

  let { items, activePath = null, onOpen, onLeaveTop, emptyMessage }: Props = $props();

  let listEl = $state<HTMLDivElement | undefined>(undefined);

  function onKeydown(e: KeyboardEvent, note: CatalogueNote) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const moved = moveRowFocus(
        listEl,
        e.currentTarget as HTMLElement,
        e.key === 'ArrowDown' ? 1 : -1,
        '.gn-row',
      );
      if (!moved && e.key === 'ArrowUp') onLeaveTop?.();
      return;
    }
    if (e.key === 'Enter' && note.path) {
      e.preventDefault();
      onOpen(note.path);
    }
  }
</script>

{#if items.length === 0}
  <EmptyState message={emptyMessage} compact />
{:else}
  <div class="gn-list" bind:this={listEl}>
    {#each items as item (item.note.id)}
      {@const path = item.note.path}
      <button
        type="button"
        class="gn-row"
        class:selected={path != null && path === activePath}
        disabled={path == null}
        title={path ?? 'This note declares a frontmatter uid, so it has no path to open by.'}
        onclick={() => path && onOpen(path)}
        onkeydown={(e) => onKeydown(e, item.note)}
      >
        <NoteRowContent
          title={item.note.title}
          accent={garrulusNotesStore.accentFor(item.note.typeId)}
          meta={item.meta}
          pinned={item.note.pinned}
          dirty={path != null && garrulusNotesStore.isDirty(path)}
          muted={path == null}
        />
      </button>
    {/each}
  </div>
{/if}

<style>
  .gn-list {
    display: flex;
    flex-direction: column;
  }

  /* Deliberately the same metrics as `Tree`'s rows (22px, 6px base padding) so
     a Pinned row and a Vault row line up when they sit one above the other. */
  .gn-row {
    display: flex;
    align-items: center;
    height: 22px;
    padding: 0 6px 0 12px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--font-size-sm);
    text-align: left;
    cursor: pointer;
  }
  .gn-row:hover:not(:disabled) { background: var(--bg-hover); }
  .gn-row.selected { background: var(--accent-subtle); }
  .gn-row:focus-visible {
    outline: 1px solid var(--border-focus, var(--accent));
    outline-offset: -1px;
  }
  .gn-row:disabled { cursor: default; }
</style>
