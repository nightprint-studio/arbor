/**
 * Garrulus sync state — everything the title bar's one sync control reads.
 *
 * `docs/garrulus-design.md` §4.2 is the rule this store exists to keep:
 * **nothing writes without a click.** The only thing that runs unattended is the
 * backend's probe, which fetches and compares and cannot change a byte; what
 * arrives here is its verdict (`garrulus:sync-state`). Every method below that
 * can change a byte — `syncNow`, `pull`, `push`, `setRemote`, `clearRemote` — is
 * reached from a handler a user's click hit, and nothing in this file starts a
 * timer. A sync that fired on its own would be the bug, not the feature.
 *
 * **The connectivity toast is the backend's decision, not this store's.** The
 * event carries `toast: 'lost' | 'regained' | null`, already gated to one per
 * episode (CLAUDE.md's auto-reconnect rules). Re-gating it here would be a
 * second policy free to disagree with the first, so the rule is literal: show it
 * when it is non-null, and never synthesise one from `state`.
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  clearRemote as ipcClearRemote,
  onGarrulusBeUp,
  onSyncState,
  pull as ipcPull,
  push as ipcPush,
  remoteConfig as ipcRemoteConfig,
  setFocus as ipcSetFocus,
  setRemote as ipcSetRemote,
  syncNow as ipcSyncNow,
  syncState as ipcSyncState,
  syncStateCount,
  syncStateTag,
  testRemote as ipcTestRemote,
  type RemoteConfig,
  type RemoteDescriptor,
  type RemoteStatus,
  type SyncState,
  type SyncStateEvent,
} from '$lib/ipc/garrulus';

/** Plural-aware note count, so no call site spells `${n === 1 ? '' : 's'}`. */
function notes(n: number): string {
  return `${n} note${n === 1 ? '' : 's'}`;
}

/** The two halves of a `diverged`, or zeroes for every other state. */
function divergence(state: SyncState): { ahead: number; behind: number } {
  if (typeof state === 'object' && 'diverged' in state) return state.diverged;
  return { ahead: 0, behind: 0 };
}

/** What a configured destination is called before its descriptor has arrived.
 *
 *  `garrulus_sync_state` answers with the state alone, so between opening a
 *  vault and touching the remote there is a config but no `RemoteDescriptor`.
 *  A label derived from the config is what the button shows in that window —
 *  the alternative is a control that says "Synced ·" and then nothing. */
function labelFromConfig(config: RemoteConfig | null): string | null {
  if (!config) return null;
  if (config.kind === 'git') return config.gitRemote?.trim() || 'origin';
  const folder = config.folder?.trim();
  if (!folder) return 'folder';
  // Basename, whichever separator the platform wrote it with.
  const parts = folder.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? folder;
}

function createGarrulusSyncStore() {
  let state = $state<SyncState>('no-remote');
  let descriptor = $state<RemoteDescriptor | null>(null);
  let config = $state<RemoteConfig | null>(null);
  /** An action the user started is running. Drives the control's spinner, and
   *  is the reason two clicks cannot start two syncs. */
  let busy = $state(false);

  const tag = $derived(syncStateTag(state));
  const count = $derived(syncStateCount(state));
  const split = $derived(divergence(state));
  const remoteLabel = $derived(descriptor?.display ?? labelFromConfig(config));

  let unlistenState: UnlistenFn | null = null;
  let unlistenBeUp: UnlistenFn | null = null;
  let started = false;
  /**
   * Last value pushed to `garrulus_set_focus`.
   *
   * Deliberately not `$state`: nothing renders it, it exists only to keep a
   * focus/blur storm from becoming a round trip each, and making it reactive
   * would put this setter's own write into the dependency set of anything that
   * reads the store.
   */
  let pushedFocus: boolean | null = null;

  function applyEvent(e: SyncStateEvent) {
    state = e.state;
    // Literal, per the header: the gating already happened in the backend.
    if (e.toast === 'lost') {
      toastStore.show('The sync destination stopped answering. Retrying.', 'warning');
    } else if (e.toast === 'regained') {
      toastStore.show('The sync destination is back.', 'success');
    }
  }

  /** Re-read state and destination. Cheap, read-only, and safe to call after
   *  anything that might have moved either. */
  async function refresh(): Promise<void> {
    try {
      const [next, cfg] = await Promise.all([ipcSyncState(), ipcRemoteConfig()]);
      state = next;
      config = cfg;
    } catch {
      // No vault open, or `garrulus-be` not attached yet. Neither is a failure
      // worth a toast: what is on screen stays until something can answer.
    }
  }

  function pushFocus(focused: boolean): void {
    if (pushedFocus === focused) return;
    pushedFocus = focused;
    // Fire and forget: a dropped focus ping costs one probe interval, and a
    // rejection here (backend down) would be noise on top of the outage the
    // user is already being told about.
    void ipcSetFocus(focused).catch(() => {});
  }

  /**
   * Run one user-initiated action.
   *
   * Everything that changes bytes goes through here so the in-flight flag, the
   * error toast and the follow-up refresh exist once rather than five times.
   */
  /** The single place `descriptor` is written. A plain closure rather than a
   *  method so no call site depends on how it was invoked. */
  function adopt(status: RemoteStatus, next: RemoteConfig) {
    descriptor = status.descriptor;
    state = status.state;
    config = next;
  }

  async function act<T>(verb: string, run: () => Promise<T>, report: (result: T) => void) {
    if (busy) return;
    busy = true;
    try {
      report(await run());
    } catch (e) {
      toastStore.show(`${verb} failed — ${e}`, 'error');
    } finally {
      busy = false;
      await refresh();
    }
  }

  return {
    get state() { return state; },
    /** Kebab-case tag of `state` — what the control keys its icon and colour off. */
    get tag() { return tag; },
    /** The count `state` carries, or 0. `diverged` reports 0 — read `ahead`/`behind`. */
    get count() { return count; },
    get ahead() { return split.ahead; },
    get behind() { return split.behind; },
    get descriptor() { return descriptor; },
    get config() { return config; },
    /** What to call the destination in a sentence, or `null` when there is none. */
    get remoteLabel() { return remoteLabel; },
    get busy() { return busy; },

    /**
     * Subscribe and take a first reading. Idempotent, so a window that mounts
     * its shell twice under HMR does not end up with two listeners.
     */
    async init(): Promise<void> {
      if (started) return;
      started = true;
      await refresh();
      try {
        unlistenState = await onSyncState(applyEvent);
      } catch {
        // No dispatcher (rare). The one-shot reading above still stands, and
        // every user action refreshes — the button is stale, not wrong.
      }
      try {
        unlistenBeUp = await onGarrulusBeUp(() => {
          // The backend restarted: it has no idea whether this window is in
          // front, so tell it again rather than waiting for the next blur.
          pushedFocus = null;
          pushFocus(typeof document !== 'undefined' ? document.hasFocus() : true);
          void refresh();
        });
      } catch { /* same as above */ }
      pushFocus(typeof document !== 'undefined' ? document.hasFocus() : true);
    },

    /** Drop both listeners. Call from the owner's teardown. */
    dispose(): void {
      unlistenState?.();
      unlistenState = null;
      unlistenBeUp?.();
      unlistenBeUp = null;
      started = false;
      pushedFocus = null;
    },

    /**
     * Tell the backend whether this window has focus.
     *
     * Load-bearing rather than a nicety: a headless backend has no window to
     * ask, so the "only probe while focused" preference silently does nothing
     * until someone pushes this.
     */
    setFocused(focused: boolean): void {
      pushFocus(focused);
    },

    /** Read the state again without changing anything — the `synced` and
     *  `offline` states' main action, and what a manual "check now" runs. */
    refresh,

    // ── The actions. Each one is reached from a click, and only from a click. ──

    /** Commit, pull, push — the sync button's main action. */
    syncNow(message?: string) {
      return act('Sync', () => ipcSyncNow(message), (r) => {
        const parts: string[] = [];
        if (r.applied > 0) parts.push(`${notes(r.applied)} received`);
        if (r.pushed) parts.push('changes sent');
        if (r.conflicts > 0) parts.push(`${r.conflicts} conflict${r.conflicts === 1 ? '' : 's'}`);
        if (parts.length === 0) {
          toastStore.show('Already up to date.', 'info');
          return;
        }
        toastStore.show(`Synced — ${parts.join(' · ')}.`, r.conflicts > 0 ? 'warning' : 'success');
      });
    },

    /** Pull only. Conflicts are a normal outcome here, not an error path. */
    pull() {
      return act('Pull', () => ipcPull(), (r) => {
        if (r.conflicts.length > 0) {
          toastStore.show(
            `${notes(r.applied.length)} received · ${r.conflicts.length} conflict${
              r.conflicts.length === 1 ? '' : 's'
            } to resolve.`,
            'warning',
          );
          return;
        }
        toastStore.show(
          r.applied.length > 0 ? `${notes(r.applied.length)} received.` : 'Nothing new.',
          r.applied.length > 0 ? 'success' : 'info',
        );
      });
    },

    /** Push only. The empty note list means "everything the remote considers
     *  changed", which is what a button press means. */
    push(message?: string) {
      return act('Push', () => ipcPush([], message), () => {
        toastStore.show('Changes sent.', 'success');
      });
    },

    /**
     * Take on a `RemoteStatus` the caller already obtained.
     *
     * The destination form calls the IPC itself — it needs the raw status to show
     * the result of a Test before anything is adopted — and then has to hand the
     * outcome back, because `refresh()` re-reads the state and the config but
     * **not** the descriptor. Without this the store's `descriptor` stayed null
     * forever, and every verb gated on `capabilities.history` was unreachable
     * even on a git remote.
     *
     * The one place `descriptor` is assigned; `setRemote` below goes through it.
     */
    adoptStatus(status: RemoteStatus, next: RemoteConfig) {
      adopt(status, next);
    },

    /** Point the vault at a destination and adopt it. */
    setRemote(next: RemoteConfig) {
      return act('Configuring the destination', () => ipcSetRemote(next), (status: RemoteStatus) => {
        adopt(status, next);
        toastStore.show(`Syncing with ${status.descriptor.display}.`, 'success');
      });
    },

    /** Make the vault local-only again. Changes no file — a git vault keeps its
     *  `.git`, a mirrored vault keeps its mirror. */
    clearRemote() {
      return act('Removing the destination', () => ipcClearRemote(), () => {
        descriptor = null;
        config = null;
        state = 'no-remote';
        toastStore.show('This vault is local-only now.', 'info');
      });
    },

    /**
     * Try a destination without adopting it — the settings form's "test".
     *
     * Deliberately outside `act`: it persists nothing, so it must not claim the
     * control's in-flight state, and the caller needs the failure (that is the
     * whole question being asked) rather than a toast.
     */
    testRemote(candidate: RemoteConfig): Promise<RemoteStatus> {
      return ipcTestRemote(candidate);
    },
  };
}

export const garrulusSyncStore = createGarrulusSyncStore();
