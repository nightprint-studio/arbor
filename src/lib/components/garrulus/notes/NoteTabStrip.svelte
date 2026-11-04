<script lang="ts">
  /**
   * The note tabs, when there is more than one note open.
   *
   * The shared `Tabs` widget in its `panel` variant — the same strip Corvus's
   * repository tabs and the terminal use — so a Garrulus tab drags, overflows and
   * closes exactly like every other tab in the suite. Only the inside of a tab is
   * ours: the note type's colour on the left and the unsaved dot on the right,
   * per `docs/mockups/garrulus-ui.html`.
   *
   * One note open means no strip: a single tab is a title bar pretending to be a
   * choice, and the note's name is already in the header below it.
   */
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { garrulusNotesStore, type OpenNote } from '$lib/stores/garrulus/notes.svelte';

  interface Props {
    open: OpenNote[];
    activePath: string | null;
    onSelect: (path: string) => void;
    onClose: (path: string) => void;
  }

  let { open, activePath, onSelect, onClose }: Props = $props();

  const items = $derived<TabItem[]>(
    open.map((note) => ({
      id: note.path,
      label: note.title,
      title: note.path,
      data: note,
    })),
  );
</script>

<!-- The strip fills its parent (`height: 100%`), so the parent has to have a height: the shared
     token, the same box Corvus and Bennu wrap their tabs in. -->
<div class="gnt">
<Tabs
  {items}
  value={activePath}
  variant="panel"
  size="sm"
  closable
  overflow
  ariaLabel="Open notes"
  onSelect={(id) => onSelect(id)}
  onClose={(id) => onClose(id)}
>
  {#snippet itemContent({ item }: { item: TabItem; active: boolean })}
    {@const note = item.data as OpenNote}
    <span
      class="gnt-dot"
      style:background={garrulusNotesStore.accentFor(note.typeId) ?? 'var(--text-disabled)'}
    ></span>
    <span class="gnt-name">{item.label}</span>
    {#if note.text !== note.saved}
      <span class="gnt-dirty" aria-label="Unsaved changes"></span>
    {/if}
  {/snippet}
</Tabs>
</div>

<style>
  .gnt {
    display: flex;
    align-items: stretch;
    flex: none;
    height: var(--tab-strip-height);
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
  }
  .gnt :global(.tabs) { flex: 1; min-width: 0; }

  .gnt-dot {
    width: 6px;
    height: 6px;
    border-radius: 2px;
    flex: none;
  }

  .gnt-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gnt-dirty {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: none;
    background: var(--warning);
  }
</style>
