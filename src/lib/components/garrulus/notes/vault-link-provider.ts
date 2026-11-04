/**
 * What the vault tells the shared markdown editor about links and tags.
 *
 * The editor owns the syntax (`$lib/utils/markdown-vault`); this owns the answers — which
 * titles exist, which tags are in use, and what following either one does. Everything is read
 * from stores that are already in memory, because the editor asks on keystrokes.
 *
 * **Resolution mirrors the backend's.** The index's resolver keys a note by its title and by
 * the last segment of it, lowercased with `.md` dropped (`garrulus-index::graph`). The editor
 * must agree or a link would be drawn as resolved here and reported as broken by Problems; the
 * backend stays the authority, this is the same rule applied locally so it can answer
 * synchronously.
 */

import { tags as ipcTags } from '$lib/ipc/garrulus';
import { linkKey, type VaultLinkProvider } from '$lib/utils/markdown-vault';
import { garrulusNotesStore, type CatalogueNote } from '$lib/stores/garrulus/notes.svelte';
import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';
import { tagToken } from '../search/query-tokens';
import { noteFolder } from '../panels/note-path';
import { stemFromTitle } from './note-naming';

const SUGGESTION_LIMIT = 30;

/** Last segment of a link key — `projects/arbor` also answers to `arbor`. */
function leafKey(raw: string): string {
  const key = linkKey(raw);
  return key.slice(Math.max(key.lastIndexOf('/'), key.lastIndexOf('\\')) + 1);
}

let knownTags: string[] = [];

/** Re-read the vault's tags. A read over the in-memory index; safe to call on any change. */
export async function refreshVaultTags(): Promise<void> {
  try {
    knownTags = await ipcTags();
  } catch {
    // Completion just offers nothing until the next refresh; the editor is unaffected.
  }
}

/** Title-prefix matches first, then substring matches, each alphabetical. */
export function rankTitles(notes: readonly CatalogueNote[], query: string): CatalogueNote[] {
  const q = query.trim().toLowerCase();
  const withPath = notes.filter((n) => n.path);
  if (!q) return withPath.slice(0, SUGGESTION_LIMIT);
  const starts: CatalogueNote[] = [];
  const contains: CatalogueNote[] = [];
  for (const n of withPath) {
    const t = n.title.toLowerCase();
    if (t.startsWith(q)) starts.push(n);
    else if (t.includes(q)) contains.push(n);
  }
  const byTitle = (a: CatalogueNote, b: CatalogueNote) => a.title.localeCompare(b.title);
  return [...starts.sort(byTitle), ...contains.sort(byTitle)].slice(0, SUGGESTION_LIMIT);
}

/** Build the provider the note editor mounts with. */
export function createVaultLinkProvider(): VaultLinkProvider {
  // Rebuilt only when the catalogue array is replaced, which is how the store refreshes it.
  let keysFor: readonly CatalogueNote[] | null = null;
  let keys = new Map<string, CatalogueNote>();

  function keyed(): Map<string, CatalogueNote> {
    const notes = garrulusNotesStore.notes;
    if (notes !== keysFor) {
      keysFor = notes;
      keys = new Map();
      for (const n of notes) {
        if (!n.path) continue;
        // First note to claim a key keeps it, like the backend's resolver.
        for (const k of [linkKey(n.title), leafKey(n.title)]) if (k && !keys.has(k)) keys.set(k, n);
      }
    }
    return keys;
  }

  const find = (target: string): CatalogueNote | undefined => {
    const map = keyed();
    return map.get(linkKey(target)) ?? map.get(leafKey(target));
  };

  return {
    resolves: (target) => find(target) != null,

    suggest: (query) =>
      rankTitles(garrulusNotesStore.notes, query).map((n) => ({
        target: n.title,
        detail: noteFolder(n.path as string) || undefined,
      })),

    open: (target, _heading, _split) => {
      const note = find(target);
      if (note?.path) {
        void garrulusNotesStore.openNote(note.path);
        return;
      }
      // An unresolved link is how a note gets made in this dialect: following it creates the
      // note, empty, at the vault root. `create` refuses to overwrite, so a catalogue that is a
      // moment stale cannot clobber a note that already exists.
      const stem = stemFromTitle(target.split('/').pop() ?? target);
      if (stem) void garrulusNotesStore.create(`${stem}.md`);
    },

    tags: () => knownTags,

    openTag: (tag) => {
      garrulusUiStore.selectSection('search');
      void garrulusSearchStore.toggleAndRun(tagToken(tag));
    },
  };
}
