/**
 * The open vault, the vaults this profile knows about, and the note types the
 * open one declares.
 *
 * Separate from `sync.svelte.ts` because the two answer different questions:
 * that one is "where does this vault stand against its destination", this one is
 * "which vault are we even talking about". Everything on screen belongs to one
 * vault, so this is the store the title bar, the footer, the sidebar and the
 * palette all read to know what they are describing.
 *
 * **Nothing here opens a vault on its own** (`docs/garrulus-design.md` §4.2).
 * `listVaults` and `listTypes` are reads and run unattended; `open`, `close` and
 * `rebuild` are reached from a click and only from a click. Opening a vault
 * parses and indexes a whole folder, which is why even reopening the last one is
 * *offered* by the start pane rather than done on mount.
 *
 * **The one edge to another store.** Opening or closing a vault invalidates the
 * sync state — it is per-vault — so `adopt()` and `close()` refresh it. The
 * dependency runs vault → sync and never back: putting it here means the three
 * ways a vault can become open (the picker, the palette's per-vault entries, the
 * start pane's reopen offer) cannot each forget it.
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { relativeTime } from '$lib/utils/diff-formatter';
import { garrulusSyncStore } from './sync.svelte';
import {
  closeVault as ipcCloseVault,
  listTypes as ipcListTypes,
  listVaults as ipcListVaults,
  onGarrulusBeUp,
  openVault as ipcOpenVault,
  rebuildIndex as ipcRebuildIndex,
  type NoteType,
  type VaultEntry,
  type VaultSummary,
} from '$lib/ipc/garrulus';

/**
 * When a vault was last opened, in words — or `null` for one never opened.
 *
 * Here rather than in either consumer because both the vault list and the start
 * pane's reopen offer say it, and both would otherwise have to remember the
 * unit: `last_opened` is Unix **milliseconds** and `relativeTime` takes seconds.
 */
export function vaultLastSeen(entry: VaultEntry): string | null {
  return entry.last_opened ? relativeTime(entry.last_opened / 1000) : null;
}

function createGarrulusVaultStore() {
  let vault = $state<VaultSummary | null>(null);
  let entries = $state<VaultEntry[]>([]);
  let types = $state<NoteType[]>([]);
  /** A vault-level action the user started is running. */
  let busy = $state(false);

  const isOpen = $derived(vault !== null);

  /**
   * The vault worth offering to reopen: the most recently opened one, when none
   * is open now. `listVaults` answers most-recent-first, so the first entry that
   * has ever been opened is it.
   */
  const lastOpened = $derived(
    vault ? null : (entries.find((e) => e.last_opened != null) ?? null),
  );

  let unlistenBeUp: UnlistenFn | null = null;
  let started = false;

  /** Re-read the registry. A read: safe to run whenever it might have moved. */
  async function refreshRegistry(): Promise<void> {
    try {
      entries = await ipcListVaults();
    } catch {
      // No `garrulus-be` yet. The list stays as it was — an empty registry and
      // an unreachable one are not the same claim, and `onGarrulusBeUp` retries.
    }
  }

  /** Re-read the open vault's note types. Empty when no vault is open. */
  async function refreshTypes(): Promise<void> {
    if (!vault) {
      types = [];
      return;
    }
    try {
      types = await ipcListTypes();
    } catch {
      types = [];
    }
  }

  /**
   * Record a vault the backend has already opened.
   *
   * The picker performs its own `openVault` (it needs the failure inline, next
   * to the folder that produced it), so the summary arrives here rather than
   * being fetched. Every other way in funnels through this too, which is what
   * keeps the follow-up reads in one place.
   */
  function adopt(summary: VaultSummary): void {
    vault = summary;
    void refreshTypes();
    // The registry moved: this vault is now the most recently opened one.
    void refreshRegistry();
    void garrulusSyncStore.refresh();
  }

  /**
   * Run one user-initiated vault action — the in-flight flag and the error toast
   * exist once rather than at each of the three call sites.
   */
  async function act<T>(verb: string, run: () => Promise<T>): Promise<T | null> {
    if (busy) return null;
    busy = true;
    try {
      return await run();
    } catch (e) {
      toastStore.show(`${verb} failed — ${e}`, 'error');
      return null;
    } finally {
      busy = false;
    }
  }

  // Named `openAt` rather than `open` so nothing in this module can read as the
  // global of that name; it is exported below under the name the callers use.
  async function openAt(path: string): Promise<void> {
    const summary = await act('Opening the vault', () => ipcOpenVault(path));
    if (summary) adopt(summary);
  }

  return {
    get vault() { return vault; },
    /** Display name of the open vault, or `null` — what the chrome shows. */
    get name() { return vault?.display_name ?? null; },
    /** Absolute root of the open vault, or `null`. */
    get root() { return vault?.root ?? null; },
    /** Notes indexed at open, or `null` when no vault is open. */
    get noteCount() { return vault?.note_count ?? null; },
    get isOpen() { return isOpen; },
    /** Every known vault, most recently opened first. */
    get entries() { return entries; },
    /** Note types declared inside the open vault. */
    get types() { return types; },
    /** The vault worth offering to reopen, or `null`. */
    get lastOpened() { return lastOpened; },
    get busy() { return busy; },

    /**
     * Read the registry and subscribe. Idempotent, so a shell that mounts twice
     * under HMR does not end up with two listeners.
     *
     * Reads only — this is what makes the reopen offer an offer.
     */
    async init(): Promise<void> {
      if (started) return;
      started = true;
      await refreshRegistry();
      try {
        unlistenBeUp = await onGarrulusBeUp(() => {
          // A backend that just attached has no vault open, whatever this window
          // last saw: a respawned `garrulus-be` dropped the index and the
          // watcher with it. Saying so puts the reopen offer back on screen
          // instead of leaving a name in the title bar that answers nothing.
          vault = null;
          types = [];
          void refreshRegistry();
        });
      } catch {
        // No dispatcher. The one-shot read above still stands.
      }
    },

    dispose(): void {
      unlistenBeUp?.();
      unlistenBeUp = null;
      started = false;
    },

    refreshRegistry,
    adopt,

    // ── The actions. Each one is reached from a click, and only from a click. ──

    /** Open the vault rooted at `path`, and make it the one on screen. */
    open: openAt,

    /** Open a vault the registry already knows, by its id — the palette's
     *  per-vault entries, which address a vault by name rather than by path. */
    async openById(id: string): Promise<void> {
      const entry = entries.find((e) => e.id === id);
      if (!entry) {
        toastStore.show('That vault is no longer in the list.', 'warning');
        return;
      }
      await openAt(entry.path);
    },

    /** Stop the watcher, drop the index, detach the remote. */
    async close(): Promise<void> {
      if (!vault) return;
      const name = vault.display_name;
      const done = await act('Closing the vault', async () => {
        await ipcCloseVault();
        return true;
      });
      if (!done) return;
      vault = null;
      types = [];
      void refreshRegistry();
      void garrulusSyncStore.refresh();
      toastStore.show(`${name} closed.`, 'info');
    },

    /**
     * Re-read every note and rebuild the index.
     *
     * The answer to a vault changed by something other than Garrulus. Same work
     * an open does, so it is offered and never run on a timer.
     */
    async rebuild(): Promise<void> {
      if (!vault) return;
      const count = await act('Rebuilding the index', () => ipcRebuildIndex());
      if (count === null) return;
      if (vault) vault = { ...vault, note_count: count };
      void refreshTypes();
      toastStore.show(`Index rebuilt — ${count} note${count === 1 ? '' : 's'}.`, 'success');
    },
  };
}

export const garrulusVaultStore = createGarrulusVaultStore();
