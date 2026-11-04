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
import { withTimeout } from '$lib/utils/with-timeout';

/** How long the backend gets to say which dictionaries are installed. It is a file-existence check:
 *  silence for this long means the backend is not serving (yet), not that the answer is slow. */
const STATUS_TIMEOUT_MS = 8000;

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
  /** Whether {@link loadStatus} has ever succeeded — see `statusKnown`. */
  let known = $state(false);
  /** Why the last {@link loadStatus} failed, or null — what the Spelling page shows instead of waiting. */
  let statusError = $state<string | null>(null);
  /** A status request is in flight. */
  let statusLoading = $state(false);
  let statusInflight: Promise<void> | null = null;
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

    /**
     * Whether the backend has ever answered which dictionaries are on disk.
     *
     * Distinct from `installed`, and the distinction is a defect rather than a nicety: a screen
     * that cannot tell "none installed" from "nobody has answered yet" offers to download
     * dictionaries that are sitting on disk, which is what it did every time the first read
     * happened before the backend was up.
     */
    get statusKnown() { return known; },
    /** The reason the last status request failed or timed out; null after a success. */
    get statusError() { return statusError; },
    get statusLoading() { return statusLoading; },

    /**
     * Refresh the install status from the BE.
     *
     * A failure leaves the last known answer alone instead of writing "not installed" over it —
     * but it is no longer silent: it is kept in `statusError`, so a screen can say the backend did
     * not answer and offer to ask again. The call is bounded by a timeout, because an IPC to a
     * backend that is not serving yet does not reject, it just never returns — and a store waiting
     * on it stays on "asking…" forever. Concurrent callers share one request.
     */
    loadStatus(): Promise<void> {
      if (statusInflight) return statusInflight;
      statusLoading = true;
      statusInflight = (async () => {
        try {
          status = await withTimeout(ipcStatus(), STATUS_TIMEOUT_MS, 'The Bennu backend');
          known = true;
          statusError = null;
        } catch (e) {
          statusError = e instanceof Error ? e.message : String(e);
        } finally {
          statusLoading = false;
          statusInflight = null;
        }
      })();
      return statusInflight;
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
        known = true;
        revision += 1;
        if (status?.installed) toastStore.show('Spell-check dictionaries installed.', 'success');
      } catch (e) {
        toastStore.show(`Dictionary download failed — ${e instanceof Error ? e.message : String(e)}`, 'error');
      } finally {
        downloading = false;
        progress = null;
      }
    },

    /**
     * Say the dictionaries changed under somebody else's write.
     *
     * The dictionary list edits the files directly (it replaces a whole list, which is not what
     * `addToDictionary` does), and the editor's spell effect has to re-run for a word that was
     * *removed* just as much as for one that was added.
     */
    bumpRevision() {
      revision += 1;
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
