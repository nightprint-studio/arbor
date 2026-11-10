/**
 * The toasts for Imber's schedule events. A scheduled run happens outside any conversation the user is
 * looking at, so the backend announces it on `arbor://<product>/imber-schedule`: `started` at every start
 * (manual "run now" included), `failed` on the first failure of an episode (the backend already filters).
 * Call once from each product's shell; the returned function stops listening.
 */

import { imberScheduleTopic, type ImberScheduleProduct } from '$lib/ipc/imber-schedule';
import type { ImberScheduleEvent } from '$lib/types/imber-schedule';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import type { ImberSchedulesStore } from './imber-schedules.svelte';
import { uiStore } from './ui.svelte';

/** Short, quiet toasts: a start is information, only a failure raises its voice. */
const STARTED_MS = 2500;
const FAILED_MS = 6000;

export function toastForImberSchedule(e: ImberScheduleEvent): void {
  if (e.kind === 'started') {
    uiStore.showToast(`Imber · Schedule “${e.name}” started`, 'info', STARTED_MS);
  } else {
    const why = e.detail ? `: ${e.detail}` : '';
    uiStore.showToast(`Imber · Schedule “${e.name}” failed${why}`, 'error', FAILED_MS);
  }
}

export function listenImberScheduleEvents(product: ImberScheduleProduct, schedules: ImberSchedulesStore): () => void {
  return setupTauriListeners([{
    event: imberScheduleTopic(product),
    handler: (ev: { payload: ImberScheduleEvent }) => {
      toastForImberSchedule(ev.payload);
      // The window's "last run" and "next run" move with a start or a failure.
      schedules.runEnded();
    },
  }]);
}
