/** Imber's schedules in Corvus: the shared store, wired to `corvus-be` and the active repository. */

import { corvusImberScheduleApi } from '$lib/ipc/corvus/imber-schedule';
import { createImberSchedulesStore } from '../imber-schedules.svelte';
// `imberStore` calls back into this store when a run ends; both only use each other inside functions.
import { imberStore } from './imber.svelte';
import { tabsStore } from './tabs.svelte';

export const corvusImberSchedules = createImberSchedulesStore({
  ...corvusImberScheduleApi,
  product: 'corvus',
  productName: 'Corvus',
  actions: ['awake', 'prompt', 'fetch'],
  allowModeChoice: true,
  defaultRoot: () => tabsStore.activeTab?.path ?? null,
  currentConversationId: () => imberStore.current?.id ?? null,
});
