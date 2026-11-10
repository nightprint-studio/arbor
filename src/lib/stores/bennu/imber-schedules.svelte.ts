/** Imber's schedules in Bennu: the shared store, wired to `bennu-be` and the active project. Always read-only, no `fetch`. */

import { bennuImberScheduleApi } from '$lib/ipc/bennu/imber-schedule';
import { createImberSchedulesStore } from '../imber-schedules.svelte';
// `bennuAssistantStore` calls back into this store when a run ends; both only use each other inside functions.
import { bennuAssistantStore } from './assistant.svelte';
import { projectStore } from './project.svelte';
import { bennuUiStore } from './ui.svelte';

export const bennuImberSchedules = createImberSchedulesStore({
  ...bennuImberScheduleApi,
  product: 'bennu',
  productName: 'Bennu',
  actions: ['awake', 'prompt'],
  allowModeChoice: false,
  defaultRoot: () => projectStore.project?.root ?? null,
  // A conversation nothing was said in is not saved yet: there is nothing to wake.
  currentConversationId: () => (bennuAssistantStore.active?.messages.length ? bennuAssistantStore.activeId : null),
  async openConversation(conversationId) {
    if (bennuUiStore.rightPanel !== 'assistant') bennuUiStore.toggleRight('assistant');
    await bennuAssistantStore.select(conversationId);
  },
});
