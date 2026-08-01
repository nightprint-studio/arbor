/**
 * The notes of the open vault: which ones exist, which ones are open, and which
 * of those has unsaved bytes.
 *
 * Separate from `vault.svelte.ts` for the same reason that one is separate from
 * `sync.svelte.ts`: that store answers "which folder are we looking at", this one
 * answers "what is inside it and what is on screen". The dependency runs
 * vault → notes and never back — opening or closing a vault invalidates
 * everything here, and nothing here can change which vault is open.
 *
 * **Nothing in this file writes without a click.** `refresh` and the catalogue
 * loads are reads over the backend's in-memory index and are safe to run
 * unattended; `save` is the only method that puts bytes on disk and it is reached
 * from `Ctrl+S`, from the editor losing focus, or from closing a dirty tab.
 * Opening a note reads it; it never creates one.
 *
 * ## How the catalogue is built, and why it looks indirect
 *
 * `garrulus-be` has no "list every note" handler (see `src/lib/ipc/garrulus.ts` —
 * there is no wrapper because there is no method). What it does have is a quick
 * switcher whose scorer treats an empty needle as matching everything, which is
 * documented as *the* way to list a whole vault
 * (`crates/products/garrulus/index/src/fuzzy.rs::score`). So:
 *
 *  * `quickSwitch('', …)` — every note, as id + title;
 *  * `search('type:<id>')` once per declared type — which notes are of it, since a
 *    `Hit` does not carry the type;
 *  * `search('pinned:true')` — the frontmatter flag that pins a note.
 *
 * All of it is `Index` reads with no disk I/O, and the whole set is a handful of
 * round trips even on a large vault. It is still a workaround: one
 * `garrulus_list_notes` returning path + title + type + mtime would replace all
 * three calls, carry the two things this cannot know (the path of a note that
 * declares a `uid`, and modification times), and is the right fix.
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  onVaultChanged,
  quickSwitch as ipcQuickSwitch,
  readNote as ipcReadNote,
  search as ipcSearch,
  writeNote as ipcWriteNote,
  type Hit,
} from '$lib/ipc/garrulus';
import { garrulusVaultStore } from './vault.svelte';

/** Ceiling on the enumeration. Past this the sidebar is not the tool anyway —
 *  search is — and an unbounded list would be a promise this store cannot keep. */
const CATALOGUE_LIMIT = 100_000;

/** How many notes the "recent" list remembers. Long enough to get back to what
 *  you were doing, short enough that it is still a shortlist. */
const RECENT_LIMIT = 8;

/** Trailing debounce on `garrulus:vault-changed`. The backend already debounces
 *  the watcher; this second one exists because *our own* saves fire that event,
 *  and a catalogue rebuild per keystroke-flush is work nobody asked for. */
const WATCH_DEBOUNCE_MS = 600;

/**
 * The vault-relative path an index id addresses, or `null`.
 *
 * A note's id is its path **unless** its frontmatter declares a `uid`, in which
 * case the index answers with the uid and there is no way back to the file from
 * here — every note-addressing backend call takes a path. Rather than guess, such
 * a note is listed and marked unopenable, which is at least true.
 */
export function notePathOfId(id: string): string | null {
  return /\.(md|markdown)$/i.test(id) ? id : null;
}

/** The file name of a vault-relative path — the fallback title for a note the
 *  catalogue has not caught up with yet. */
function noteFileName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

/** One note as the sidebar knows it — everything but its body. */
export interface CatalogueNote {
  /** Index id: the vault-relative path, or a frontmatter `uid`. */
  id: string;
  /** Display title — frontmatter `title`, else the first `#`, else the stem. */
  title: string;
  /** Where it lives, or `null` when the id is a uid. See {@link notePathOfId}. */
  path: string | null;
  /** The type it was classified as, or `null` for an untyped note. */
  typeId: string | null;
  /** Frontmatter said `pinned: true`. */
  pinned: boolean;
}

/** One note open in the centre column. The bytes are the record: `saved` is what
 *  the last read or write put on disk, `text` is what the editor holds. */
export interface OpenNote {
  path: string;
  title: string;
  typeId: string | null;
  /** Text as of the last successful read or write. */
  saved: string;
  /** Text in the editor right now. */
  text: string;
  /** Bumped on every (re)read, so the editor knows to rebuild its document
   *  rather than diff it. `0` means "not read yet". */
  revision: number;
  loading: boolean;
  saving: boolean;
  /** Why the note could not be read, or `null`. */
  error: string | null;
}

/** A `type:` term the query parser will read back as the type it names. */
function typeFilter(typeId: string): string | null {
  if (typeId.includes('"')) return null; // unquotable — skip rather than mis-query
  return /\s/.test(typeId) ? `type:"${typeId}"` : `type:${typeId}`;
}

/** Hits → ids, tolerating a query the backend refused. */
async function idsOf(query: string): Promise<string[]> {
  try {
    const hits = await ipcSearch(query);
    return hits.map((h: Hit) => h.id);
  } catch {
    return [];
  }
}

function createGarrulusNotesStore() {
  let catalogue = $state<CatalogueNote[]>([]);
  let catalogueLoading = $state(false);
  let catalogueError = $state<string | null>(null);

  let open = $state<OpenNote[]>([]);
  let activePath = $state<string | null>(null);

  /** Session-shaped: what this window has opened, most recent first. Persisting
   *  it would be a setting, and settings live in `garrulus/config.toml` through
   *  the backend — which has no wrapper for them yet. */
  let recent = $state<{ path: string; at: number }[]>([]);

  const byPath = $derived(
    new Map(catalogue.filter((n) => n.path).map((n) => [n.path as string, n])),
  );

  const pinned = $derived(
    catalogue.filter((n) => n.pinned).sort((a, b) => a.title.localeCompare(b.title)),
  );

  /** Recently opened notes that are still in the vault, newest first. */
  const recentNotes = $derived(
    recent
      .map((r) => ({ at: r.at, note: byPath.get(r.path) }))
      .filter((r): r is { at: number; note: CatalogueNote } => r.note != null),
  );

  const active = $derived(open.find((n) => n.path === activePath) ?? null);

  let unlistenChanged: UnlistenFn | null = null;
  let watchTimer: ReturnType<typeof setTimeout> | null = null;
  let started = false;
  /** Monotonic, so two reads in the same millisecond still differ. */
  let loadSeq = 0;
  /** Which vault the catalogue describes — the guard against a load that lands
   *  after the user already switched vaults. */
  let loadedVaultId: string | null = null;
  /** Which note types it was built against. They arrive a moment after the vault
   *  does, and a note's colour comes from them, so the catalogue is worth
   *  re-reading when the set changes. */
  let loadedTypeKey = '';

  function findOpen(path: string): OpenNote | undefined {
    return open.find((n) => n.path === path);
  }

  /**
   * Read the whole vault into the catalogue.
   *
   * Reads only. Type membership and the pinned flag are separate queries because
   * a `Hit` carries neither; they run together with the enumeration so one slow
   * one does not serialise the rest.
   */
  async function loadCatalogue(vaultId: string): Promise<void> {
    catalogueLoading = true;
    catalogueError = null;
    try {
      const types = garrulusVaultStore.types;
      const [hits, pinnedIds, membership] = await Promise.all([
        ipcQuickSwitch('', CATALOGUE_LIMIT),
        idsOf('pinned:true'),
        Promise.all(
          types.map(async (t) => {
            const q = typeFilter(t.id);
            return { id: t.id, ids: q ? await idsOf(q) : [] };
          }),
        ),
      ]);

      // A vault switched while this was in flight owns the screen now.
      if (loadedVaultId !== vaultId) return;

      const typeOf = new Map<string, string>();
      for (const m of membership) for (const id of m.ids) typeOf.set(id, m.id);
      const isPinned = new Set(pinnedIds);

      catalogue = hits.map((h) => ({
        id: h.id,
        title: h.title,
        path: notePathOfId(h.id),
        typeId: typeOf.get(h.id) ?? null,
        pinned: isPinned.has(h.id),
      }));
    } catch (e) {
      if (loadedVaultId !== vaultId) return;
      // An unreachable backend is a real state, not an empty vault: say so rather
      // than draw a tree with nothing in it.
      catalogueError = `${e}`;
      catalogue = [];
    } finally {
      if (loadedVaultId === vaultId) catalogueLoading = false;
    }
  }

  function clearAll(): void {
    catalogue = [];
    catalogueError = null;
    catalogueLoading = false;
    open = [];
    activePath = null;
    recent = [];
  }

  function rememberRecent(path: string): void {
    recent = [{ path, at: Date.now() }, ...recent.filter((r) => r.path !== path)].slice(
      0,
      RECENT_LIMIT,
    );
  }

  /** Adopt a note that is already on disk into a tab and read it. */
  async function load(note: OpenNote): Promise<void> {
    note.loading = true;
    note.error = null;
    try {
      const source = await ipcReadNote(note.path);
      note.saved = source.text;
      note.text = source.text;
      note.revision = ++loadSeq;
    } catch (e) {
      note.error = `${e}`;
    } finally {
      note.loading = false;
    }
  }

  return {
    // ── The catalogue ────────────────────────────────────────────────────────
    get notes() { return catalogue; },
    get loading() { return catalogueLoading; },
    get error() { return catalogueError; },
    get pinned() { return pinned; },
    get recent() { return recentNotes; },

    // ── Open notes ───────────────────────────────────────────────────────────
    get open() { return open; },
    get active() { return active; },
    get activePath() { return activePath; },
    isDirty(path: string): boolean {
      const n = findOpen(path);
      return n != null && n.text !== n.saved;
    },

    /**
     * The colour a note type paints its notes with, or `null` for an untyped one.
     *
     * Here rather than in each of the four surfaces that draw a dot: the tree,
     * the pinned list, the recents list and the tab strip all ask the same
     * question, and a fallback that differs between them reads as a bug.
     */
    accentFor(typeId: string | null | undefined): string | null {
      if (!typeId) return null;
      return garrulusVaultStore.types.find((t) => t.id === typeId)?.accent ?? null;
    },

    /** What a note type is called, for a tooltip. */
    typeName(typeId: string | null | undefined): string | null {
      if (!typeId) return null;
      return garrulusVaultStore.types.find((t) => t.id === typeId)?.name ?? typeId;
    },

    // ── Lifetime ─────────────────────────────────────────────────────────────

    /**
     * Subscribe to the watcher. Idempotent, so a surface that mounts twice under
     * HMR does not end up with two listeners.
     *
     * Reads only: the event says the vault moved on disk (a pull, Obsidian, the
     * other PC), and the answer is to re-read the catalogue. Open buffers are
     * deliberately left alone — replacing the text under a caret is how an editor
     * loses an edit, and the note the user is typing in is the one they care
     * about most.
     */
    async init(): Promise<void> {
      if (started) return;
      started = true;
      try {
        unlistenChanged = await onVaultChanged(() => {
          if (watchTimer) clearTimeout(watchTimer);
          watchTimer = setTimeout(() => {
            watchTimer = null;
            if (loadedVaultId) void loadCatalogue(loadedVaultId);
          }, WATCH_DEBOUNCE_MS);
        });
      } catch {
        // No dispatcher yet. The catalogue still loads when a vault opens.
      }
    },

    dispose(): void {
      unlistenChanged?.();
      unlistenChanged = null;
      if (watchTimer) clearTimeout(watchTimer);
      watchTimer = null;
      started = false;
    },

    /**
     * Follow the open vault. Called with the vault's id — or `null` when none is
     * open — and a signature of its note types, which the vault store fetches a
     * round trip after the vault itself. One entry point, so "what we are looking
     * at changed" is handled once instead of at each surface that notices.
     *
     * A new vault empties everything, including the open tabs. Types arriving for
     * the vault already loaded only re-reads the catalogue: closing the note
     * someone is writing in because a colour became available would be absurd.
     */
    syncWithVault(vaultId: string | null, typeKey = ''): void {
      if (vaultId === loadedVaultId && typeKey === loadedTypeKey) return;
      const switched = vaultId !== loadedVaultId;
      loadedVaultId = vaultId;
      loadedTypeKey = typeKey;
      if (switched) clearAll();
      if (vaultId) void loadCatalogue(vaultId);
    },

    /** Re-read the catalogue. A read — offered on a button, safe on an event. */
    refresh(): void {
      if (loadedVaultId) void loadCatalogue(loadedVaultId);
    },

    // ── Actions. Each one is reached from a click or a keystroke. ────────────

    /** Open a note in a tab and make it the one on screen. Reads; never creates. */
    async openNote(path: string): Promise<void> {
      rememberRecent(path);
      const already = findOpen(path);
      if (already) {
        activePath = path;
        if (already.error) await load(already);
        return;
      }
      const meta = byPath.get(path);
      const note: OpenNote = {
        path,
        title: meta?.title ?? noteFileName(path),
        typeId: meta?.typeId ?? null,
        saved: '',
        text: '',
        revision: 0,
        loading: true,
        saving: false,
        error: null,
      };
      open = [...open, note];
      activePath = path;
      await load(findOpen(path) ?? note);
    },

    /** Bring an already-open tab forward. */
    activate(path: string): void {
      if (findOpen(path)) activePath = path;
    },

    /**
     * Close a tab. Refuses a dirty one — the caller asks first
     * (`notes/CloseNoteModal`), then passes `force`. A silent close that dropped
     * an edit would be the worst possible reading of "close".
     */
    close(path: string, force = false): boolean {
      const note = findOpen(path);
      if (!note) return true;
      if (!force && note.text !== note.saved) return false;
      const index = open.findIndex((n) => n.path === path);
      open = open.filter((n) => n.path !== path);
      if (activePath === path) {
        const next = open[Math.min(index, open.length - 1)];
        activePath = next?.path ?? null;
      }
      return true;
    },

    /** Next / previous tab, wrapping. */
    cycle(delta: number): void {
      if (open.length < 2) return;
      const i = open.findIndex((n) => n.path === activePath);
      const next = (i + delta + open.length) % open.length;
      activePath = open[next].path;
    },

    /** What the editor typed. Nothing is written here — `save` does that. */
    setText(path: string, text: string): void {
      const note = findOpen(path);
      if (note) note.text = text;
    },

    /**
     * Put the note's bytes on disk, exactly as the editor holds them.
     *
     * No normalisation of any kind: the file is the record, and a save that
     * reformatted it would rewrite lines the user never touched and turn every
     * sync into a diff nobody can read.
     */
    async save(path?: string): Promise<void> {
      const note = path ? findOpen(path) : active;
      if (!note || note.saving || note.loading) return;
      if (note.text === note.saved) return;
      const text = note.text;
      note.saving = true;
      try {
        await ipcWriteNote(note.path, text);
        note.saved = text;
      } catch (e) {
        toastStore.show(`Saving ${note.title} failed — ${e}`, 'error');
      } finally {
        note.saving = false;
      }
    },
  };
}

export const garrulusNotesStore = createGarrulusNotesStore();
