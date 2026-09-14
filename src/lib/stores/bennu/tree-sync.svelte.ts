/**
 * Keeps the Project tree in step with the disk.
 *
 * Two inputs. The backend's watcher (`arbor://bennu/tree-changed`) is the fast path: it names
 * what changed a moment after it did. The window regaining focus is the safety net: a watcher
 * can miss things — Windows drops the tail of a burst that overflows its buffer without saying so
 * — so on focus the root and the expanded folders are re-listed one level deep, and a
 * disagreement with what is on screen reloads the tree. Cheap, off the UI thread, and throttled.
 *
 * The decisions themselves are pure, in `components/bennu/tree-changes.ts`.
 */

import { listen } from '@tauri-apps/api/event';
import { projectStore } from './project.svelte';
import { bennuUiStore } from './ui.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { TREE_CHANGED, type TreeChanged } from '$lib/ipc/bennu/tree-watch';
import { projectTree } from '$lib/ipc/bennu';
import { reindex } from '$lib/ipc/bennu/nav';
import {
  describeNotice,
  findNode,
  isEmptyNotice,
  listingDiffers,
  manifestChanged,
  mergeNotices,
  needsTreeReload,
  reconcileTargets,
  renamedExpansion,
  structureNotice,
  type StructureNotice,
} from '$lib/components/bennu/tree-changes';

/** How long notices for one root keep merging before one is shown. A module moved by an IDE is
 *  several bursts: the folder, then the parent pom. */
const NOTICE_SETTLE_MS = 1500;
/** How long a notice stays up — and, while it does, a new one for the same root replaces it merged. */
const NOTICE_MS = 8000;
/** The least time between two focus reconciliations. Alt-Tabbing is not a reason to re-list. */
const RECONCILE_GAP_MS = 5000;
/** Directories one reconciliation re-lists at most. */
const RECONCILE_MAX_DIRS = 32;

interface ShownNotice { id: string; notice: StructureNotice; at: number }

function createBennuTreeSyncStore() {
  const settling = new Map<string, { notice: StructureNotice; timer: ReturnType<typeof setTimeout> }>();
  const shown = new Map<string, ShownNotice>();
  let reconcileAt = 0;
  let reconciling = false;

  function onTreeChanged(payload: TreeChanged | undefined) {
    const root = payload?.root;
    if (!payload || !root) return;
    if (needsTreeReload(payload)) {
      // Before the reload, so a renamed folder that was open is still open when its rows arrive.
      for (const [from, to] of renamedExpansion(bennuUiStore.treeExpanded, root, payload.changes ?? [])) {
        bennuUiStore.setExpanded(from, false);
        bennuUiStore.setExpanded(to, true);
      }
      projectStore.refreshTreeOf(root);
    }
    // A manifest is the one file whose CONTENT the tree cannot express: rename an `<artifactId>`
    // and the project is called something else, but nothing on screen was told. Re-read only the
    // model, never re-open.
    if (manifestChanged(payload)) void projectStore.refreshProjectInfo(root);
    const notice = structureNotice(payload);
    if (notice) queueNotice(root, notice);
  }

  function queueNotice(root: string, notice: StructureNotice) {
    const previous = settling.get(root);
    if (previous) clearTimeout(previous.timer);
    const merged = previous ? mergeNotices(previous.notice, notice) : notice;
    const timer = setTimeout(() => {
      settling.delete(root);
      showNotice(root, merged);
    }, NOTICE_SETTLE_MS);
    settling.set(root, { notice: merged, timer });
  }

  /** One toast per root: a notice arriving while the last one is still up replaces it, merged. */
  function showNotice(root: string, notice: StructureNotice) {
    const last = shown.get(root);
    let next = notice;
    if (last && Date.now() - last.at < NOTICE_MS) {
      toastStore.dismiss(last.id);
      next = mergeNotices(last.notice, notice);
    }
    shown.delete(root);
    if (isEmptyNotice(next)) return;
    const project = projectStore.workspaceRoots.length > 1 ? root.split(/[\\/]/).filter(Boolean).pop() : undefined;
    const action = next.rebuildIndex
      ? {
          label: 'Rebuild index',
          onClick: () => {
            void reindex(root).catch(() => toastStore.show('Could not rebuild the index', 'error'));
          },
        }
      : undefined;
    const id = toastStore.show(describeNotice(next, project), 'info', NOTICE_MS, action);
    shown.set(root, { id, notice: next, at: Date.now() });
  }

  /** Re-list the root and the expanded folders one level deep; reload when any disagrees. */
  async function reconcile() {
    const root = projectStore.project?.root;
    if (!root || projectStore.isDemo || !projectStore.tree || reconciling) return;
    const now = Date.now();
    if (now - reconcileAt < RECONCILE_GAP_MS) return;
    reconcileAt = now;
    reconciling = true;
    try {
      const targets = reconcileTargets(root, bennuUiStore.treeExpanded, RECONCILE_MAX_DIRS);
      const listings = await Promise.all(
        targets.map((dir) => projectTree(dir, 1).then((fresh) => ({ dir, fresh }), () => null)),
      );
      // A switch while the listings were out: they answer for a tree no longer on screen.
      if (projectStore.project?.root !== root) return;
      const tree = projectStore.tree;
      if (listings.some((l) => l && listingDiffers(findNode(tree, l.dir), l.fresh))) {
        projectStore.refreshTreeOf(root);
      }
    } finally {
      reconciling = false;
    }
  }

  return {
    /** Subscribe to the watcher and to focus. Resolves the detach. */
    async attach(): Promise<() => void> {
      let unlisten: () => void = () => {};
      try {
        unlisten = await listen<TreeChanged>(TREE_CHANGED, (e) => onTreeChanged(e.payload));
      } catch { /* not in a Tauri window — the focus net still works */ }
      const onFocus = () => void reconcile();
      const onVisible = () => { if (!document.hidden) void reconcile(); };
      window.addEventListener('focus', onFocus);
      document.addEventListener('visibilitychange', onVisible);
      return () => {
        unlisten();
        window.removeEventListener('focus', onFocus);
        document.removeEventListener('visibilitychange', onVisible);
        for (const { timer } of settling.values()) clearTimeout(timer);
        settling.clear();
      };
    },
  };
}

export const bennuTreeSyncStore = createBennuTreeSyncStore();
