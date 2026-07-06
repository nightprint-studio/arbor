/**
 * Bennu spell-check store — opt-in-per-project Hunspell checking of identifiers +
 * comments.
 *
 * Owns: the per-project enabled flag (opt-in; session-only for now — persistence to
 * a `[bennu]` config section is a follow-up), the dictionary install status, and the
 * download lifecycle (progress via `arbor://bennu/dict-progress`). The editor pulls
 * `bennu_spellcheck` when `enabledFor(root)` and dictionaries are installed, and
 * `addToDictionary` powers the "Add to dictionary" quick-fix.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SvelteSet } from 'svelte/reactivity';
import { spellStatus as ipcStatus, downloadDictionaries as ipcDownload, dictAdd as ipcDictAdd, type SpellStatus } from '$lib/ipc/bennu/spell';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

function createBennuSpellStore() {
  // Per-project enable (opt-in). Session-only; keyed by project root.
  const enabledRoots = new SvelteSet<string>();
  let status = $state<SpellStatus | null>(null);
  let downloading = $state(false);
  let progress = $state<string | null>(null);
  // Bumped whenever the dictionaries change (download / add-word) so the editor's
  // spell effect re-runs.
  let revision = $state(0);

  let attached = false;
  let unlisten: UnlistenFn | null = null;

  return {
    get status() { return status; },
    get installed() { return status?.installed ?? false; },
    get downloading() { return downloading; },
    get progress() { return progress; },
    get revision() { return revision; },

    /** Whether spell-check is on for a project AND dictionaries are installed. */
    activeFor(root: string | null): boolean {
      return !!root && enabledRoots.has(root) && (status?.installed ?? false);
    },
    enabledFor(root: string | null): boolean {
      return !!root && enabledRoots.has(root);
    },
    setEnabled(root: string, on: boolean) {
      if (on) enabledRoots.add(root); else enabledRoots.delete(root);
    },

    /** Subscribe to download-progress events (once, from BennuWindow.onMount) + load
     *  the current install status. Returns a detach fn. */
    async attach(): Promise<UnlistenFn> {
      if (!attached) {
        attached = true;
        unlisten = await listen<{ lang: string; file: string; done: boolean }>(
          'arbor://bennu/dict-progress',
          (e) => { progress = `${e.payload.lang} · ${e.payload.file}`; },
        );
      }
      void this.loadStatus();
      return () => { unlisten?.(); attached = false; };
    },

    /** Refresh the install status from the BE. */
    async loadStatus() {
      try { status = await ipcStatus(); } catch { status = { installed: false, languages: [] }; }
    },

    /** Download the EN + IT dictionaries (job-like; progress via events). Surfaces a failure as a
     *  toast — the BE now returns an error when NOTHING downloads (offline / blocked URL), instead of
     *  a silent no-op that looked like the button did nothing. */
    async download() {
      if (downloading) return;
      downloading = true;
      progress = 'Starting…';
      try {
        status = await ipcDownload();
        revision += 1;
        if (status?.installed) toastStore.show('Spell-check dictionaries installed.', 'success');
      } catch (e) {
        toastStore.show(`Dictionary download failed — ${e instanceof Error ? e.message : String(e)}`, 'error');
      } finally {
        downloading = false;
        progress = null;
      }
    },

    /** Add a word to a custom dictionary + bump the revision so the editor re-lints. */
    async addToDictionary(word: string, scope: 'project' | 'global', root: string) {
      try {
        await ipcDictAdd(word, scope, root);
        revision += 1;
      } catch {
        /* best-effort */
      }
    },
  };
}

export const bennuSpellStore = createBennuSpellStore();
