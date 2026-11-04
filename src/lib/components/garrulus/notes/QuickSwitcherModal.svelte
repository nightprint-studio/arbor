<script lang="ts">
  /**
   * Ctrl+O — open a note by what you remember of its title.
   *
   * The shared palette engine over the index's fuzzy scorer. It is a second box
   * from the command palette on purpose (`docs/garrulus-design.md` §9): merging
   * "what can I do" and "where is the note called…" ranks a verb against a title
   * and answers neither well.
   *
   * An empty query lists the recent notes, so `Ctrl+O, Enter` goes back to the one
   * you just left. A note whose id is a frontmatter `uid` has no path the backend
   * can open by, and is left out rather than offered as a row that fails.
   */
  import CommandPaletteShell, {
    type PaletteItem,
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { paletteIcon } from '$lib/utils/plugin-icons';
  import { quickSwitch } from '$lib/ipc/garrulus';
  import { garrulusNotesStore, notePathOfId } from '$lib/stores/garrulus/notes.svelte';
  import { noteFolder } from '../panels/note-path';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  const LIMIT = 40;
  const DEBOUNCE_MS = 60;

  let query = $state('');
  let hits = $state<{ path: string; title: string }[]>([]);
  let loading = $state(false);
  let seq = 0;

  function open(path: string) {
    onClose();
    void garrulusNotesStore.openNote(path);
  }

  function itemOf(path: string, title: string): PaletteItem {
    const note = garrulusNotesStore.notes.find((n) => n.path === path);
    return {
      id: path,
      title,
      subtitle: noteFolder(path) || undefined,
      icon: 'FileText',
      iconColor: garrulusNotesStore.accentFor(note?.typeId) ?? undefined,
      action: () => open(path),
    };
  }

  // Each keystroke asks the backend; only the newest answer is kept, so a slow
  // reply to "bu" cannot land over the reply to "bug".
  $effect(() => {
    const needle = query.trim();
    const mine = ++seq;
    if (!needle) {
      hits = [];
      loading = false;
      return;
    }
    loading = true;
    const timer = setTimeout(async () => {
      try {
        const found = await quickSwitch(needle, LIMIT);
        if (mine !== seq) return;
        hits = found.flatMap((h) => {
          const path = notePathOfId(h.id);
          return path ? [{ path, title: h.title }] : [];
        });
      } catch {
        if (mine === seq) hits = [];
      } finally {
        if (mine === seq) loading = false;
      }
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  const sections = $derived.by<PaletteSection[]>(() => {
    if (!query.trim()) {
      const recent = garrulusNotesStore.recent.map((r) => itemOf(r.note.path as string, r.note.title));
      return recent.length ? [{ id: 'recent', label: 'Recent', items: recent }] : [];
    }
    return hits.length
      ? [{ id: 'notes', label: 'Notes', items: hits.map((h) => itemOf(h.path, h.title)) }]
      : [];
  });
</script>

<CommandPaletteShell
  {onClose}
  iconResolver={paletteIcon}
  {sections}
  bind:query
  {loading}
  loadingLabel="Searching…"
  placeholder="Open a note by title…"
>
  {#snippet emptyMessage()}
    {query.trim() ? `No note matches “${query.trim()}”.` : 'Nothing opened yet — type a title.'}
  {/snippet}
</CommandPaletteShell>
