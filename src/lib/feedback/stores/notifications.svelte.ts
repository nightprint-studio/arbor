import { openUrl, openPath } from '@tauri-apps/plugin-opener';
import { getCurrentWindow } from '@tauri-apps/api/window';

export type NotificationLevel = 'info' | 'success' | 'warning' | 'error';

/** Tagged union of "click actions" a notification can carry.  Stored as
 *  data (no function references) so notifications survive across reloads.
 *  The `NotificationsOverlay` dispatches the action via a small mapper —
 *  add new variants and a matching case in `dispatchNotificationAction`. */
export type NotificationAction =
  | { kind: 'open-link-manager';   label: string; link_id: string }
  | { kind: 'open-tab-by-repo-id'; label: string; repo_id: string }
  | { kind: 'open-url';            label: string; url: string }
  | { kind: 'open-path';           label: string; path: string; reveal?: boolean }
  | { kind: 'plugin-action';       label: string; plugin: string; action: string; ctx?: Record<string, unknown> }
  /** Deep-link to a local pipeline run — opens `PipelineRunDetailModal`. */
  | { kind: 'open-pipeline-run';   label: string; run_id: string };

export interface AppNotification {
  id:        string;
  title:     string;
  message:   string;
  level:     NotificationLevel;
  plugin?:   string;
  timestamp: number;
  /** Optional click action rendered as a button in the overlay row.
   *  When the user clicks it, `dispatchNotificationAction` runs the
   *  associated side-effect (e.g. opens the Linked Worktrees manager). */
  action?:   NotificationAction;
}

// Per-window storage key. localStorage is shared across same-origin Tauri
// windows, so each window namespaces its bell archive by its label to avoid
// clobbering the others' persisted list. The main window keeps the original
// key for back-compat.
const STORAGE_KEY = (() => {
  try {
    const label = getCurrentWindow().label;
    return label && label !== 'main' ? `arbor:notifications:${label}` : 'arbor:notifications';
  } catch {
    return 'arbor:notifications';
  }
})();
const MAX_PERSISTED = 200;
/** How long a freshly-added notification stays in the transient bottom-right
 *  stack before fading out. The bell archive keeps it forever (until the
 *  user dismisses or clears all). */
const TRANSIENT_MS = 6000;

let counter = 0;

function loadStored(): AppNotification[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    for (const n of arr) {
      const m = typeof n?.id === 'string' ? n.id.match(/^notif-(\d+)$/) : null;
      if (m) counter = Math.max(counter, parseInt(m[1], 10));
    }
    return arr as AppNotification[];
  } catch {
    return [];
  }
}

function createNotificationsStore() {
  let notifications = $state<AppNotification[]>(loadStored());
  // IDs of notifications still considered "fresh" — only these render in the
  // transient bottom-right stack. Loaded-from-storage notifications are
  // never marked fresh, so reopening the app doesn't dump the whole archive
  // back into view.
  let freshIds = $state(new Set<string>());

  function markFresh(id: string) {
    freshIds = new Set(freshIds).add(id);
    setTimeout(() => {
      if (!freshIds.has(id)) return;
      const next = new Set(freshIds);
      next.delete(id);
      freshIds = next;
    }, TRANSIENT_MS);
  }

  function persist() {
    try {
      const tail = notifications.length > MAX_PERSISTED
        ? notifications.slice(-MAX_PERSISTED)
        : notifications;
      localStorage.setItem(STORAGE_KEY, JSON.stringify(tail));
    } catch {
      /* quota / serialization errors — keep in-memory state only */
    }
  }

  function add(
    title:   string,
    message: string,
    level:   NotificationLevel = 'info',
    plugin?: string,
    action?: NotificationAction,
  ): string {
    const id = `notif-${++counter}`;
    notifications = [...notifications, { id, title, message, level, plugin, timestamp: Date.now(), action }];
    markFresh(id);
    persist();
    return id;
  }

  function dismiss(id: string) {
    notifications = notifications.filter(n => n.id !== id);
    if (freshIds.has(id)) {
      const next = new Set(freshIds);
      next.delete(id);
      freshIds = next;
    }
    persist();
  }

  function clearAll() {
    notifications = [];
    freshIds = new Set();
    persist();
  }

  /** Notifications still in the transient window — drives the bottom-right
   *  stack so cards auto-fade after `TRANSIENT_MS` while the bell archive
   *  keeps the full history. */
  function transient(): AppNotification[] {
    if (freshIds.size === 0) return [];
    return notifications.filter(n => freshIds.has(n.id));
  }

  return {
    get notifications() { return notifications; },
    get count()         { return notifications.length; },
    get transient()     { return transient(); },
    add,
    dismiss,
    clearAll,
  };
}

export const notificationsStore = createNotificationsStore();

/** Run the side-effect associated with a notification action.  Imports
 *  uiStore lazily to avoid a circular dep at module-load time (uiStore
 *  itself doesn't depend on notifications, but other stores wire through
 *  it and the chain is brittle). */
export async function dispatchNotificationAction(action: NotificationAction): Promise<void> {
  switch (action.kind) {
    case 'open-link-manager': {
      const { uiStore } = await import('$lib/stores/ui.svelte');
      uiStore.openLinkManager(action.link_id);
      return;
    }
    case 'open-tab-by-repo-id': {
      // Best-effort: find an open tab for this repo and activate it.  If no
      // tab is currently open we don't auto-spawn one (paths can move and
      // this is a passive action).
      const { tabsStore }       = await import('$lib/stores/corvus/tabs.svelte');
      const { workspacesStore } = await import('$lib/stores/corvus/workspaces.svelte');
      const entry = workspacesStore.registry.find(r => r.id === action.repo_id);
      if (!entry) return;
      const tab = tabsStore.tabs.find(t => t.path === entry.path);
      if (tab) tabsStore.setActive(tab.id);
      return;
    }
    case 'open-url': {
      try { await openUrl(action.url); } catch { /* ignore */ }
      return;
    }
    case 'open-path': {
      // A `reveal` action means "show this in the file explorer" → route it
      // through the shared helper, which picks the OS file manager or Arbor's
      // built-in explorer (selecting the file) per the user's settings. A
      // plain open hands the path to the OS default handler (folder →
      // Explorer/Finder, file → default editor) unchanged. Empty path is a
      // no-op (caller bug, but better than throwing).
      try {
        if (action.reveal) {
          const { revealFile } = await import('$lib/utils/reveal');
          await revealFile(action.path);
        } else {
          await openPath(action.path);
        }
      } catch { /* ignore */ }
      return;
    }
    case 'plugin-action': {
      const { firePluginAction } = await import('$lib/ipc/plugin');
      const ctx = action.ctx ? JSON.stringify(action.ctx) : '{}';
      await firePluginAction(action.plugin, action.action, ctx);
      return;
    }
    case 'open-pipeline-run': {
      const { pipelinesStore } = await import('$lib/stores/corvus/pipelines.svelte');
      pipelinesStore.setActiveRun(action.run_id);
      return;
    }
  }
}
