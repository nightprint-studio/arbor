/**
 * The build queue as the Live panel shows it: running and waiting tickets, kept current by the backend's
 * push event. Mounted only while a view of it is on screen (reference counted).
 */

import { buildQueue, buildQueueCancel, onBuildQueueChanged, type BuildQueueRow } from '$lib/ipc/bennu/build-queue';

function createBuildQueueStore() {
  let rows = $state<BuildQueueRow[]>([]);
  let users = 0;
  let unlisten: (() => void) | null = null;

  return {
    get rows() { return rows; },
    get active() { return rows.length > 0; },

    async mount() {
      users += 1;
      if (users > 1) return;
      unlisten = await onBuildQueueChanged((next) => { rows = next; });
      try { rows = (await buildQueue()).rows; } catch { rows = []; }
    },
    unmount() {
      users = Math.max(0, users - 1);
      if (users > 0) return;
      unlisten?.();
      unlisten = null;
    },
    async cancel(ticket: number) {
      try { await buildQueueCancel(ticket); } catch { /* the ticket was already served */ }
    },
  };
}

export const buildQueueStore = createBuildQueueStore();
