/**
 * Stops of the program the assistant is debugging that arrived while its conversation was idle: a card per
 * conversation ("Stopped at X:line"), with a way to send the assistant to look at it now — and, only when the user
 * allowed it (the setting, or the conversation's own switch), a run that starts by itself. The backend already
 * took the snapshot and queued it for the conversation's next message; the backend also keeps the hourly limit.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ASSISTANT_STOP_EVENT, assistantStopAutoOk } from '$lib/ipc/bennu/assistant';
import { cardOf, stopPrompt, withCard, withoutCard, type StopCard, type StopEventPayload } from '$lib/components/bennu/assistant/assistant-stop';
import { bennuAssistantStore } from './assistant.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

function createAssistantStopStore() {
  let cards = $state<StopCard[]>([]);
  let unlisten: UnlistenFn | null = null;
  let started = false;

  async function onStop(e: StopEventPayload) {
    const card = cardOf(e);
    cards = withCard(cards, card);
    const own = bennuAssistantStore.autoContinueFor(card.conversationId);
    const allowed = await assistantStopAutoOk(card.conversationId, bennuAssistantStore.autoContinueOnStop, own ?? null).catch(() => false);
    if (allowed) await sendNow(card.conversationId);
  }

  async function sendNow(conversationId: string) {
    const card = cards.find((c) => c.conversationId === conversationId);
    if (!card) return;
    if (bennuAssistantStore.running) {
      toastStore.show('Wait for Imber to finish, or stop it first.', 'warning');
      return;
    }
    cards = withoutCard(cards, conversationId);
    await bennuAssistantStore.select(conversationId);
    await bennuAssistantStore.send(stopPrompt(card));
  }

  return {
    cardFor(conversationId: string | null | undefined): StopCard | null {
      return conversationId ? cards.find((c) => c.conversationId === conversationId) ?? null : null;
    },
    /** Listen once. */
    async start() {
      if (started) return;
      started = true;
      unlisten = await listen<StopEventPayload>(ASSISTANT_STOP_EVENT, (ev) => { void onStop(ev.payload); });
    },
    stopListening() { unlisten?.(); unlisten = null; started = false; },
    sendNow,
    dismiss(conversationId: string) { cards = withoutCard(cards, conversationId); },
  };
}

export const assistantStopStore = createAssistantStopStore();
