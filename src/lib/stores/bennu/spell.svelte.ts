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
import { getBennuConfig, setBennuConfig } from '$lib/ipc/bennu/config';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

function createBennuSpellStore() {
  /**
   * The project roots spell-check is on for.
   *
   * **Persisted**, and that is the whole of a defect rather than a nicety. This set used to live
   * for the length of a session, so every project — and every restart of the same one — asked
   * again whether to turn it on and offer the dictionaries, with the dictionaries already
   * installed. The dictionaries are global and downloaded once; who wants them used is per project
   * and is now remembered.
   */
  const enabledRoots = new SvelteSet<string>();
  /** Set once the config has been read, so a save before that cannot write an empty list over it. */
  let loaded = false;

  /**
   * Write the set back, read-modify-write against the freshest config.
   *
   * The same discipline every other config-backed setting uses: another flow owns the rest of the
   * file (encodings, JDK paths, the per-project overrides), and writing a snapshot taken minutes
   * ago would quietly undo whatever it did in between. Fire-and-forget — a persistence hiccup must
   * never block a toggle.
   */
  async function persistRoots() {
    if (!loaded) return;
    try {
      const cur = await getBennuConfig();
      await setBennuConfig({ ...cur, spell_check_roots: [...enabledRoots] });
    } catch {
      // Silent: the toggle has already taken effect in this session, and a dialog about a config
      // file is not what somebody who just pressed a checkbox is asking about.
    }
  }
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
      void persistRoots();
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
      // The remembered roots, before the first file asks whether to check it — otherwise a project
      // that was on last night comes up off and asks again, which is the thing being fixed.
      if (!loaded) {
        try {
          for (const r of (await getBennuConfig()).spell_check_roots ?? []) enabledRoots.add(r);
        } catch {
          // A config that cannot be read is a session with nothing remembered, not a broken store.
        }
        loaded = true;
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
