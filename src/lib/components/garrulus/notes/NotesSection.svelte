<script lang="ts">
  /**
   * The sidebar's *Notes* section: a filter, what you pinned, what you touched
   * last, and the vault itself.
   *
   * Layout and order follow `docs/mockups/garrulus-ui.html` (Fissate · Recenti ·
   * Vault). The three sections are collapsible because on a real vault the tree
   * is the tall one and the two shortlists above it are what you actually use —
   * being able to fold the tree away and keep them is the point of having them
   * separate at all.
   *
   * **Keyboard.** The filter is the entry point: `↓` from it steps into the first
   * row, `Esc` clears it. On a row, `↑`/`↓` walk, `Enter` opens, and typing a
   * letter runs the caret back up to the filter with that letter in it
   * (IntelliJ's speed-search). Nothing here needs the mouse.
   *
   * **Nothing here writes.** Opening a note reads it; the catalogue behind these
   * three lists is index reads. The section never creates, renames or deletes —
   * those verbs need surfaces this window does not have yet, and a button that
   * did nothing would be worse than no button.
   */
  import { Search } from 'lucide-svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import NoteList from './NoteList.svelte';
  import NoteTree from './NoteTree.svelte';
  import { buildNoteTree } from './note-tree';
  import { relativeTime } from '$lib/utils/diff-formatter';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';

  let filter = $state('');
  let filterEl = $state<HTMLInputElement | undefined>(undefined);
  let treeRef = $state<{
    expandAll: () => void;
    collapseAll: () => void;
    reveal: (path: string) => Promise<void>;
    focusFirst: () => void;
  } | null>(null);

  let pinnedOpen = $state(true);
  let recentOpen = $state(true);
  let vaultOpen = $state(true);

  const notes = $derived(garrulusNotesStore.notes);
  const tree = $derived(buildNoteTree(notes));
  const activePath = $derived(garrulusNotesStore.activePath);

  const pinnedItems = $derived(garrulusNotesStore.pinned.map((note) => ({ note })));
  const recentItems = $derived(
    garrulusNotesStore.recent.map((r) => ({ note: r.note, meta: relativeTime(r.at / 1000) })),
  );

  function open(path: string) {
    void garrulusNotesStore.openNote(path);
  }

  /** Put the caret back in the filter, at the end of what is there. `focus()`
   *  alone would leave the browser free to select the whole value, and the next
   *  keystroke would then replace the search instead of extending it.
   *
   *  Exported: the panel header's "Filter notes" button lands here too. */
  export function focusFilter() {
    filterEl?.focus();
    const end = filter.length;
    filterEl?.setSelectionRange(end, end);
  }

  function onFilterKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      treeRef?.focusFirst();
      return;
    }
    if (e.key === 'Escape' && filter) {
      e.preventDefault();
      e.stopPropagation();
      filter = '';
    }
  }

  function onTypeAhead(char: string) {
    filter += char;
    focusFilter();
  }

  // ── Exposed to the panel header, which owns the buttons ───────────────────

  export function expandAll() { treeRef?.expandAll(); }
  export function collapseAll() { treeRef?.collapseAll(); }

  /** Show the note that is on screen, in the tree. */
  export async function revealActive() {
    if (activePath) await treeRef?.reveal(activePath);
  }
</script>

<div class="gns">
  <div class="gns-filter">
    <Input
      bind:value={filter}
      bind:element={filterEl}
      type="search"
      size="sm"
      placeholder="Filter notes…"
      ariaLabel="Filter notes"
      clearable
      onkeydown={onFilterKey}
    >
      {#snippet iconStart()}<Search size={12} />{/snippet}
    </Input>
  </div>

  {#if garrulusNotesStore.loading && notes.length === 0}
    <StateBlock tone="loading" label="Reading the vault…" fill={false}>
      {#snippet spinner()}<Spinner size={16} />{/snippet}
    </StateBlock>
  {:else if garrulusNotesStore.error}
    <StateBlock tone="error" fill={false}>
      <span class="gns-error">Could not list the notes — {garrulusNotesStore.error}</span>
      <Button size="sm" variant="secondary" onclick={() => garrulusNotesStore.refresh()}>
        Try again
      </Button>
    </StateBlock>
  {:else}
    <SidebarSection label="Pinned" badge={pinnedItems.length || null} bind:expanded={pinnedOpen}>
      <NoteList
        items={pinnedItems}
        {activePath}
        onOpen={open}
        onLeaveTop={focusFilter}
        emptyMessage="No pinned note — add pinned: true to a note's frontmatter."
      />
    </SidebarSection>

    <SidebarSection label="Recent" badge={recentItems.length || null} bind:expanded={recentOpen}>
      <NoteList
        items={recentItems}
        {activePath}
        onOpen={open}
        onLeaveTop={focusFilter}
        emptyMessage="Nothing opened in this window yet."
      />
    </SidebarSection>

    <SidebarSection label="Vault" badge={notes.length || null} bind:expanded={vaultOpen}>
      <NoteTree
        bind:this={treeRef}
        nodes={tree}
        {activePath}
        {filter}
        onOpen={open}
        {onTypeAhead}
        onLeaveTop={focusFilter}
      />
    </SidebarSection>
  {/if}
</div>

<style>
  .gns {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .gns-filter {
    padding: 6px 8px;
  }

  .gns-error {
    font-size: var(--font-size-xs);
    line-height: 1.45;
    max-width: 40ch;
    text-align: center;
  }
</style>
